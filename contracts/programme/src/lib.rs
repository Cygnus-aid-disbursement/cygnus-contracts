#![no_std]

//! Cygnus programme contract.
//!
//! A donor funds a programme, an implementer claims against milestones, an
//! approver releases each milestone budget, and the implementer disburses to
//! beneficiaries in batches. Individual payouts never reach the chain: a batch
//! carries a Merkle root, a total and a count, nothing more.
//!
//! The normative description of states, transitions, the batch format and the
//! Merkle construction is `docs/protocol.md`. Code follows that document.

mod error;
mod merkle;
mod types;

pub use error::Error;
pub use types::*;

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, token, Address, BytesN, Env, Vec,
};

#[contractevent]
pub struct ProgrammeCreated {
    #[topic]
    pub programme_id: u64,
    pub sponsor: Address,
    pub implementer: Address,
    pub total: i128,
}

#[contractevent]
pub struct ProgrammeFunded {
    #[topic]
    pub programme_id: u64,
    pub amount: i128,
}

#[contractevent]
pub struct MilestoneClaimed {
    #[topic]
    pub programme_id: u64,
    pub milestone_index: u32,
    pub evidence_hash: BytesN<32>,
    pub claimed_late: bool,
}

#[contractevent]
pub struct MilestoneApproved {
    #[topic]
    pub programme_id: u64,
    pub milestone_index: u32,
}

#[contractevent]
pub struct MilestoneRejected {
    #[topic]
    pub programme_id: u64,
    pub milestone_index: u32,
    pub reason_hash: BytesN<32>,
}

#[contractevent]
pub struct BatchDisbursed {
    #[topic]
    pub programme_id: u64,
    pub batch_index: u32,
    pub root: BytesN<32>,
    pub total: i128,
    pub count: u32,
}

#[contractevent]
pub struct ProgrammeRefunded {
    #[topic]
    pub programme_id: u64,
    pub amount: i128,
}

#[contracttype]
enum DataKey {
    // Monotonic programme id counter.
    Counter,
    Programme(u64),
    Batch(u64, u32),
}

// Persistent programme state is bumped well past a single milestone cycle so an
// audit can still read a finished programme long after its last write.
const LEDGERS_PER_DAY: u32 = 17_280;
const BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;
const BUMP_TO: u32 = LEDGERS_PER_DAY * 120;

#[contract]
pub struct ProgrammeContract;

#[contractimpl]
impl ProgrammeContract {
    /// Register a programme. Returns its id. The sponsor authorises creation so
    /// nobody can bind another account as the funder of a programme.
    // The argument list mirrors PRD section 4.1 exactly; a builder would hide
    // the contract's real interface from generated bindings.
    #[allow(clippy::too_many_arguments)]
    pub fn create_programme(
        env: Env,
        sponsor: Address,
        implementer: Address,
        asset: Address,
        total: i128,
        milestones: Vec<Milestone>,
        approver: ApproverConfig,
        metadata_hash: BytesN<32>,
    ) -> Result<u64, Error> {
        sponsor.require_auth();

        if total <= 0 {
            return Err(Error::InvalidAmount);
        }
        for m in milestones.iter() {
            if m.amount <= 0 {
                return Err(Error::InvalidAmount);
            }
        }

        let id = env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::Counter)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::Counter, &(id + 1));

        let mut states = Vec::new(&env);
        let mut end_by = 0u64;
        for m in milestones.iter() {
            if m.due_by > end_by {
                end_by = m.due_by;
            }
            states.push_back(MilestoneState {
                status: MilestoneStatus::Pending,
                evidence_hash: None,
                reason_hash: None,
                claimed_at: 0,
                claimed_late: false,
            });
        }

        let programme = Programme {
            id,
            sponsor: sponsor.clone(),
            implementer: implementer.clone(),
            asset,
            total,
            funded: 0,
            released: 0,
            disbursed: 0,
            approver,
            metadata_hash,
            milestones,
            milestone_states: states,
            status: ProgrammeStatus::Active,
            end_by,
            batch_count: 0,
            created_at: env.ledger().timestamp(),
        };
        save(&env, &programme);

        ProgrammeCreated {
            programme_id: id,
            sponsor,
            implementer,
            total,
        }
        .publish(&env);
        Ok(id)
    }

    /// Move `amount` of the asset from the sponsor into the programme. Funding
    /// may be partial and repeated up to the declared total.
    pub fn fund(env: Env, programme_id: u64, amount: i128) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        p.sponsor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        token::Client::new(&env, &p.asset).transfer(
            &p.sponsor,
            env.current_contract_address(),
            &amount,
        );
        p.funded += amount;
        save(&env, &p);

        ProgrammeFunded {
            programme_id,
            amount,
        }
        .publish(&env);
        Ok(())
    }

    /// Implementer claims a milestone with an evidence hash. A claim after the
    /// due date is flagged but not blocked. A rejected milestone may be
    /// re-claimed with fresh evidence.
    pub fn claim(
        env: Env,
        programme_id: u64,
        milestone_index: u32,
        evidence_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        p.implementer.require_auth();

        let mut st = milestone_state(&p, milestone_index)?;
        match st.status {
            MilestoneStatus::Pending | MilestoneStatus::Rejected => {}
            _ => return Err(Error::MilestoneNotClaimable),
        }

        let now = env.ledger().timestamp();
        let due_by = p.milestones.get_unchecked(milestone_index).due_by;
        st.status = MilestoneStatus::Claimed;
        st.evidence_hash = Some(evidence_hash.clone());
        st.reason_hash = None;
        st.claimed_at = now;
        st.claimed_late = now > due_by;
        p.milestone_states.set(milestone_index, st.clone());
        save(&env, &p);

        MilestoneClaimed {
            programme_id,
            milestone_index,
            evidence_hash,
            claimed_late: st.claimed_late,
        }
        .publish(&env);
        Ok(())
    }

    /// Approver releases a claimed milestone's budget into the spendable pool.
    pub fn approve(env: Env, programme_id: u64, milestone_index: u32) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        approver_single(&p)?.require_auth();

        let mut st = milestone_state(&p, milestone_index)?;
        resolvable(&st)?;

        let amount = p.milestones.get_unchecked(milestone_index).amount;
        if p.released + amount > p.funded {
            return Err(Error::InsufficientFunding);
        }
        p.released += amount;
        st.status = MilestoneStatus::Approved;
        p.milestone_states.set(milestone_index, st);
        save(&env, &p);

        MilestoneApproved {
            programme_id,
            milestone_index,
        }
        .publish(&env);
        Ok(())
    }

    /// Approver rejects a claimed milestone with a reason hash. The budget is
    /// not released; the implementer may re-claim.
    pub fn reject(
        env: Env,
        programme_id: u64,
        milestone_index: u32,
        reason_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        approver_single(&p)?.require_auth();

        let mut st = milestone_state(&p, milestone_index)?;
        resolvable(&st)?;
        st.status = MilestoneStatus::Rejected;
        st.reason_hash = Some(reason_hash.clone());
        p.milestone_states.set(milestone_index, st);
        save(&env, &p);

        MilestoneRejected {
            programme_id,
            milestone_index,
            reason_hash,
        }
        .publish(&env);
        Ok(())
    }

    /// Implementer disburses a batch. The chain records the root, total and
    /// count and moves `total` to the implementer for last-mile payout. It does
    /// not record who was paid, and it does not prove they received anything.
    pub fn disburse(
        env: Env,
        programme_id: u64,
        batch_root: BytesN<32>,
        total: i128,
        count: u32,
    ) -> Result<u32, Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        p.implementer.require_auth();

        if total <= 0 || count == 0 {
            return Err(Error::InvalidBatch);
        }
        if p.disbursed + total > p.released {
            return Err(Error::InsufficientReleased);
        }

        let index = p.batch_count;
        let batch = DisbursementBatch {
            index,
            root: batch_root.clone(),
            total,
            count,
            disbursed_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Batch(programme_id, index), &batch);

        token::Client::new(&env, &p.asset).transfer(
            &env.current_contract_address(),
            &p.implementer,
            &total,
        );
        p.disbursed += total;
        p.batch_count += 1;
        save(&env, &p);

        BatchDisbursed {
            programme_id,
            batch_index: index,
            root: batch_root,
            total,
            count,
        }
        .publish(&env);
        Ok(index)
    }

    /// After the end date the sponsor recovers whatever was funded but never
    /// disbursed, including released-but-unspent budget. Closes the programme.
    pub fn refund_remainder(env: Env, programme_id: u64) -> Result<i128, Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        p.sponsor.require_auth();

        if env.ledger().timestamp() <= p.end_by {
            return Err(Error::NotEnded);
        }
        let remainder = p.funded - p.disbursed;
        if remainder <= 0 {
            return Err(Error::NothingToRefund);
        }

        token::Client::new(&env, &p.asset).transfer(
            &env.current_contract_address(),
            &p.sponsor,
            &remainder,
        );
        p.status = ProgrammeStatus::Closed;
        save(&env, &p);

        ProgrammeRefunded {
            programme_id,
            amount: remainder,
        }
        .publish(&env);
        Ok(remainder)
    }

    pub fn get_programme(env: Env, programme_id: u64) -> Option<Programme> {
        load(&env, programme_id).ok()
    }

    pub fn get_batch(env: Env, programme_id: u64, batch_index: u32) -> Option<DisbursementBatch> {
        env.storage()
            .persistent()
            .get(&DataKey::Batch(programme_id, batch_index))
    }

    /// Verify a beneficiary's inclusion in a published batch. `leaf` is the leaf
    /// hash produced by the SDK from a salted recipient reference and an amount;
    /// see `docs/protocol.md`. Returns false for an unknown programme or batch.
    pub fn verify_inclusion(
        env: Env,
        programme_id: u64,
        batch_index: u32,
        leaf: BytesN<32>,
        proof: Vec<BytesN<32>>,
    ) -> bool {
        match env
            .storage()
            .persistent()
            .get::<_, DisbursementBatch>(&DataKey::Batch(programme_id, batch_index))
        {
            Some(batch) => merkle::verify(&env, &leaf, &proof, &batch.root),
            None => false,
        }
    }

    /// Canonical leaf hash. Exposed so the SDK cross-check test and external
    /// auditors can confirm they reproduce the contract's encoding exactly.
    pub fn hash_leaf(env: Env, recipient_ref: BytesN<32>, amount: i128) -> BytesN<32> {
        merkle::hash_leaf(&env, &recipient_ref, amount)
    }
}

fn save(env: &Env, p: &Programme) {
    let key = DataKey::Programme(p.id);
    env.storage().persistent().set(&key, p);
    env.storage()
        .persistent()
        .extend_ttl(&key, BUMP_THRESHOLD, BUMP_TO);
}

fn load(env: &Env, id: u64) -> Result<Programme, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Programme(id))
        .ok_or(Error::ProgrammeNotFound)
}

fn active(p: &Programme) -> Result<(), Error> {
    if p.status == ProgrammeStatus::Active {
        Ok(())
    } else {
        Err(Error::AlreadyClosed)
    }
}

fn milestone_state(p: &Programme, index: u32) -> Result<MilestoneState, Error> {
    if index >= p.milestone_states.len() {
        return Err(Error::MilestoneIndexOutOfRange);
    }
    Ok(p.milestone_states.get_unchecked(index))
}

// A claimed milestone is the only one an approver may act on.
fn resolvable(st: &MilestoneState) -> Result<(), Error> {
    match st.status {
        MilestoneStatus::Claimed => Ok(()),
        MilestoneStatus::Pending => Err(Error::MilestoneNotClaimed),
        _ => Err(Error::MilestoneAlreadyResolved),
    }
}

fn approver_single(p: &Programme) -> Result<Address, Error> {
    match &p.approver {
        ApproverConfig::Single(a) => Ok(a.clone()),
        _ => Err(Error::ApproverKindUnimplemented),
    }
}

#[cfg(test)]
mod test;
