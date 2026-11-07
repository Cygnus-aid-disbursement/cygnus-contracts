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
pub struct MilestonePanelApproved {
    #[topic]
    pub programme_id: u64,
    pub milestone_index: u32,
    pub approver: Address,
    pub approvals_count: u32,
    pub threshold: u32,
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

#[contractevent]
pub struct ProgrammeCancelled {
    #[topic]
    pub programme_id: u64,
    pub sponsor: Address,
}

#[contracttype]
pub(crate) enum DataKey {
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

/// Maximum number of milestones permitted per programme to bound persistent storage.
pub const MAX_MILESTONES: u32 = 25;
/// Maximum number of disbursement batches permitted per programme.
pub const MAX_BATCHES: u32 = 1_000;

#[contract]
pub struct ProgrammeContract;

#[contractimpl]
impl ProgrammeContract {
    /// Register a programme. Returns its id. The sponsor authorises creation so
    /// nobody can bind another account as the funder of a programme.
    // The argument list mirrors docs/protocol.md section 3 exactly; a builder
    // would hide the contract's real interface from generated bindings.
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
        if milestones.is_empty() {
            return Err(Error::EmptyMilestones);
        }
        if milestones.len() > MAX_MILESTONES {
            return Err(Error::TooManyMilestones);
        }
        for m in milestones.iter() {
            if m.amount <= 0 {
                return Err(Error::InvalidAmount);
            }
        }

        match &approver {
            ApproverConfig::Single(_) | ApproverConfig::Oracle(_) => {}
            ApproverConfig::Panel(members, threshold) => {
                if *threshold == 0 || *threshold > members.len() || members.is_empty() {
                    return Err(Error::InvalidApproverConfig);
                }
                for i in 0..members.len() {
                    let m_i = members.get_unchecked(i);
                    for j in (i + 1)..members.len() {
                        if m_i == members.get_unchecked(j) {
                            return Err(Error::InvalidApproverConfig);
                        }
                    }
                }
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
                approvals: Vec::new(&env),
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
        st.approvals = Vec::new(&env);
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
    /// Under Single and Oracle approvers, the designated approver is authenticated.
    /// Under Panel approvers, members must call `approve_by` directly.
    pub fn approve(env: Env, programme_id: u64, milestone_index: u32) -> Result<(), Error> {
        let p = load(&env, programme_id)?;
        match &p.approver {
            ApproverConfig::Single(a) | ApproverConfig::Oracle(a) => {
                Self::approve_by(env, programme_id, milestone_index, a.clone())
            }
            ApproverConfig::Panel(_, _) => Err(Error::NotApprover),
        }
    }

    /// Explicit approver releases or votes to release a claimed milestone's budget.
    /// Under Single and Oracle approvers, `approver` must match the configured address.
    /// Under Panel approvers, `approver` must be an unrecorded panel member. When
    /// member approvals reach the threshold, the milestone status becomes Approved
    /// and the budget is released.
    pub fn approve_by(
        env: Env,
        programme_id: u64,
        milestone_index: u32,
        approver: Address,
    ) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;

        match &p.approver {
            ApproverConfig::Single(ref single) => {
                if &approver != single {
                    return Err(Error::NotApprover);
                }
                approver.require_auth();

                let mut st = milestone_state(&p, milestone_index)?;
                resolvable(&st)?;

                let amount = p.milestones.get_unchecked(milestone_index).amount;
                if p.released + amount > p.funded {
                    return Err(Error::InsufficientFunding);
                }
                p.released += amount;
                st.status = MilestoneStatus::Approved;
                st.approvals.push_back(approver);
                p.milestone_states.set(milestone_index, st);
                save(&env, &p);

                MilestoneApproved {
                    programme_id,
                    milestone_index,
                }
                .publish(&env);
                Ok(())
            }
            ApproverConfig::Oracle(ref oracle) => {
                if &approver != oracle {
                    return Err(Error::NotApprover);
                }
                approver.require_auth();

                let mut st = milestone_state(&p, milestone_index)?;
                resolvable(&st)?;

                let amount = p.milestones.get_unchecked(milestone_index).amount;
                if p.released + amount > p.funded {
                    return Err(Error::InsufficientFunding);
                }
                p.released += amount;
                st.status = MilestoneStatus::Approved;
                st.approvals.push_back(approver);
                p.milestone_states.set(milestone_index, st);
                save(&env, &p);

                MilestoneApproved {
                    programme_id,
                    milestone_index,
                }
                .publish(&env);
                Ok(())
            }
            ApproverConfig::Panel(ref members, threshold) => {
                let mut is_member = false;
                for m in members.iter() {
                    if m == approver {
                        is_member = true;
                        break;
                    }
                }
                if !is_member {
                    return Err(Error::NotApprover);
                }
                approver.require_auth();

                let mut st = milestone_state(&p, milestone_index)?;
                resolvable(&st)?;

                for existing in st.approvals.iter() {
                    if existing == approver {
                        return Err(Error::AlreadyApprovedByMember);
                    }
                }

                st.approvals.push_back(approver.clone());
                let approvals_count = st.approvals.len();

                MilestonePanelApproved {
                    programme_id,
                    milestone_index,
                    approver,
                    approvals_count,
                    threshold: *threshold,
                }
                .publish(&env);

                if approvals_count >= *threshold {
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
                } else {
                    p.milestone_states.set(milestone_index, st);
                    save(&env, &p);
                }

                Ok(())
            }
        }
    }

    /// Approver rejects a claimed milestone with a reason hash. The budget is
    /// not released; the implementer may re-claim.
    /// Under Single and Oracle approvers, the designated approver is authenticated.
    /// Under Panel approvers, members must call `reject_by` directly.
    pub fn reject(
        env: Env,
        programme_id: u64,
        milestone_index: u32,
        reason_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let p = load(&env, programme_id)?;
        match &p.approver {
            ApproverConfig::Single(a) | ApproverConfig::Oracle(a) => {
                Self::reject_by(env, programme_id, milestone_index, a.clone(), reason_hash)
            }
            ApproverConfig::Panel(_, _) => Err(Error::NotApprover),
        }
    }

    /// Explicit approver rejects a claimed milestone with a reason hash.
    /// Under Single and Oracle approvers, `approver` must match the configured address.
    /// Under Panel approvers, any member may reject the claim, clearing any accumulated
    /// approvals and transitioning the milestone to Rejected.
    pub fn reject_by(
        env: Env,
        programme_id: u64,
        milestone_index: u32,
        approver: Address,
        reason_hash: BytesN<32>,
    ) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;

        match &p.approver {
            ApproverConfig::Single(ref single) => {
                if &approver != single {
                    return Err(Error::NotApprover);
                }
                approver.require_auth();
            }
            ApproverConfig::Oracle(ref oracle) => {
                if &approver != oracle {
                    return Err(Error::NotApprover);
                }
                approver.require_auth();
            }
            ApproverConfig::Panel(ref members, _) => {
                let mut is_member = false;
                for m in members.iter() {
                    if m == approver {
                        is_member = true;
                        break;
                    }
                }
                if !is_member {
                    return Err(Error::NotApprover);
                }
                approver.require_auth();
            }
        }

        let mut st = milestone_state(&p, milestone_index)?;
        resolvable(&st)?;
        st.status = MilestoneStatus::Rejected;
        st.reason_hash = Some(reason_hash.clone());
        st.approvals = Vec::new(&env);
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
        if p.batch_count >= MAX_BATCHES {
            return Err(Error::TooManyBatches);
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
        save_batch(&env, programme_id, &batch);

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

    /// Sponsor cancels an erroneously created programme before any funding has
    /// been deposited. Closes the programme immediately.
    pub fn cancel_programme(env: Env, programme_id: u64) -> Result<(), Error> {
        let mut p = load(&env, programme_id)?;
        active(&p)?;
        p.sponsor.require_auth();

        if p.funded > 0 {
            return Err(Error::FundingAlreadyReceived);
        }

        p.status = ProgrammeStatus::Closed;
        save(&env, &p);

        ProgrammeCancelled {
            programme_id,
            sponsor: p.sponsor,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_programme(env: Env, programme_id: u64) -> Option<Programme> {
        load(&env, programme_id).ok()
    }

    /// Return a single milestone and its state without decoding the entire
    /// programme record. Returns None for an unknown programme or invalid index.
    pub fn get_milestone(
        env: Env,
        programme_id: u64,
        milestone_index: u32,
    ) -> Option<(Milestone, MilestoneState)> {
        let p = load(&env, programme_id).ok()?;
        let m = p.milestones.get(milestone_index)?;
        let s = p.milestone_states.get(milestone_index)?;
        Some((m, s))
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

fn save_batch(env: &Env, programme_id: u64, batch: &DisbursementBatch) {
    let key = DataKey::Batch(programme_id, batch.index);
    env.storage().persistent().set(&key, batch);
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

#[cfg(test)]
mod test;
