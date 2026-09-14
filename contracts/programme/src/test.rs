#![cfg(test)]

use crate::{
    merkle, ApproverConfig, Error, Milestone, MilestoneStatus, ProgrammeContract,
    ProgrammeContractClient, ProgrammeStatus,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    vec, Address, BytesN, Env, IntoVal, Vec,
};

const DAY: u64 = 86_400;

struct Ctx {
    env: Env,
    client: ProgrammeContractClient<'static>,
    contract_id: Address,
    sponsor: Address,
    implementer: Address,
    approver: Address,
    asset: Address,
}

fn hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// Two milestones, 600 and 400, due on day 10 and day 20.
fn milestones(env: &Env) -> Vec<Milestone> {
    vec![
        env,
        Milestone {
            description_hash: hash(env, 1),
            amount: 600,
            due_by: 10 * DAY,
        },
        Milestone {
            description_hash: hash(env, 2),
            amount: 400,
            due_by: 20 * DAY,
        },
    ]
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = DAY);

    let sponsor = Address::generate(&env);
    let implementer = Address::generate(&env);
    let approver = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(sponsor.clone());
    let asset = sac.address();
    StellarAssetClient::new(&env, &asset).mint(&sponsor, &1_000_000);

    let contract_id = env.register(ProgrammeContract, ());
    let client = ProgrammeContractClient::new(&env, &contract_id);

    Ctx {
        env,
        client,
        contract_id,
        sponsor,
        implementer,
        approver,
        asset,
    }
}

fn create(c: &Ctx) -> u64 {
    c.client.create_programme(
        &c.sponsor,
        &c.implementer,
        &c.asset,
        &1_000,
        &milestones(&c.env),
        &ApproverConfig::Single(c.approver.clone()),
        &hash(&c.env, 9),
    )
}

#[test]
fn full_lifecycle() {
    let c = setup();
    let id = create(&c);

    c.client.fund(&id, &1_000);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.funded, 1_000);
    assert_eq!(p.status, ProgrammeStatus::Active);

    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.released, 600);
    assert_eq!(
        p.milestone_states.get_unchecked(0).status,
        MilestoneStatus::Approved
    );

    // Disburse 600 to 50 synthetic beneficiaries.
    let (root, leaves, total, count) = build_batch(&c.env, 50, 12);
    assert_eq!(total, 600);
    let batch_index = c.client.disburse(&id, &root, &total, &count);
    assert_eq!(batch_index, 0);

    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.disbursed, 600);
    // Funds moved out of the contract to the implementer.
    let token = soroban_sdk::token::Client::new(&c.env, &c.asset);
    assert_eq!(token.balance(&c.implementer), 600);
    assert_eq!(token.balance(&c.contract_id), 400);

    // Every leaf verifies against the on-chain root.
    for i in 0..leaves.len() {
        let leaf = leaves.get_unchecked(i);
        let proof = merkle_proof(&c.env, &leaves, i);
        assert!(c.client.verify_inclusion(&id, &batch_index, &leaf, &proof));
    }

    // Refund the remaining 400 after the end date.
    c.env.ledger().with_mut(|l| l.timestamp = 21 * DAY);
    let refunded = c.client.refund_remainder(&id);
    assert_eq!(refunded, 400);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.status, ProgrammeStatus::Closed);
    assert_eq!(token.balance(&c.sponsor), 1_000_000 - 600);
}

#[test]
fn partial_funding_accumulates() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &300);
    c.client.fund(&id, &250);
    assert_eq!(c.client.get_programme(&id).unwrap().funded, 550);
}

#[test]
fn claim_twice_fails() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    let e = c
        .client
        .try_claim(&id, &0, &hash(&c.env, 21))
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::MilestoneNotClaimable));
}

#[test]
fn approve_before_claim_fails() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    let e = c.client.try_approve(&id, &0).err().unwrap();
    assert_eq!(e, Ok(Error::MilestoneNotClaimed));
}

#[test]
fn approve_beyond_funded_fails() {
    let c = setup();
    let id = create(&c);
    // Milestone 0 needs 600; fund only 500.
    c.client.fund(&id, &500);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    let e = c.client.try_approve(&id, &0).err().unwrap();
    assert_eq!(e, Ok(Error::InsufficientFunding));
}

#[test]
fn disburse_above_released_fails() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0); // releases 600
    let (root, _, _, count) = build_batch(&c.env, 10, 30);
    let e = c
        .client
        .try_disburse(&id, &root, &700, &count)
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InsufficientReleased));
}

#[test]
fn refund_before_end_fails_after_end_succeeds() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    let e = c.client.try_refund_remainder(&id).err().unwrap();
    assert_eq!(e, Ok(Error::NotEnded));

    c.env.ledger().with_mut(|l| l.timestamp = 21 * DAY);
    let refunded = c.client.refund_remainder(&id);
    assert_eq!(refunded, 1_000);
}

#[test]
fn late_claim_flagged_not_blocked() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    // Milestone 0 is due on day 10; claim on day 15.
    c.env.ledger().with_mut(|l| l.timestamp = 15 * DAY);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    let st = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestone_states
        .get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Claimed);
    assert!(st.claimed_late);
}

#[test]
fn on_time_claim_not_flagged() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    let st = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestone_states
        .get_unchecked(0);
    assert!(!st.claimed_late);
}

#[test]
fn reject_then_reclaim() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.reject(&id, &0, &hash(&c.env, 40));
    let st = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestone_states
        .get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Rejected);
    assert_eq!(st.reason_hash, Some(hash(&c.env, 40)));

    // Re-claim is allowed after rejection.
    c.client.claim(&id, &0, &hash(&c.env, 21));
    c.client.approve(&id, &0);
    assert_eq!(c.client.get_programme(&id).unwrap().released, 600);
}

#[test]
fn panel_and_oracle_approvers_unimplemented() {
    let c = setup();
    let panel = c.client.try_create_programme(
        &c.sponsor,
        &c.implementer,
        &c.asset,
        &1_000,
        &milestones(&c.env),
        &ApproverConfig::Panel(vec![&c.env, c.approver.clone()], 1),
        &hash(&c.env, 9),
    );
    // Creation with a panel config is allowed; acting on it is what fails.
    let id = panel.unwrap().unwrap();
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    let e = c.client.try_approve(&id, &0).err().unwrap();
    assert_eq!(e, Ok(Error::ApproverKindUnimplemented));
}

#[test]
fn claim_requires_implementer_auth() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    let auths = c.env.auths();
    assert_eq!(auths.first().unwrap().0, c.implementer);
}

#[test]
fn approve_requires_approver_auth() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);
    let auths = c.env.auths();
    assert_eq!(auths.first().unwrap().0, c.approver);
}

#[test]
fn claim_by_stranger_is_rejected() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    let stranger = Address::generate(&c.env);
    let res = c
        .client
        .mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &c.contract_id,
                fn_name: "claim",
                args: (id, 0u32, hash(&c.env, 20)).into_val(&c.env),
                sub_invokes: &[],
            },
        }])
        .try_claim(&id, &0, &hash(&c.env, 20));
    assert!(res.is_err());
}

#[test]
fn invalid_amounts_rejected() {
    let c = setup();
    let e = c
        .client
        .try_create_programme(
            &c.sponsor,
            &c.implementer,
            &c.asset,
            &0,
            &milestones(&c.env),
            &ApproverConfig::Single(c.approver.clone()),
            &hash(&c.env, 9),
        )
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InvalidAmount));

    let id = create(&c);
    let e = c.client.try_fund(&id, &0).err().unwrap();
    assert_eq!(e, Ok(Error::InvalidAmount));
}

#[test]
fn invalid_batch_rejected() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);
    let e = c
        .client
        .try_disburse(&id, &hash(&c.env, 50), &600, &0)
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InvalidBatch));
}

#[test]
fn verify_inclusion_rejects_bad_proof_and_unknown_batch() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);
    let (root, leaves, total, count) = build_batch(&c.env, 4, 60);
    c.client.disburse(&id, &root, &total, &count);

    // Wrong proof fails.
    let leaf = leaves.get_unchecked(0);
    let bad = vec![&c.env, hash(&c.env, 200)];
    assert!(!c.client.verify_inclusion(&id, &0, &leaf, &bad));
    // Unknown batch index fails.
    let good = merkle_proof(&c.env, &leaves, 0);
    assert!(!c.client.verify_inclusion(&id, &7, &leaf, &good));
}

#[test]
fn merkle_tree_roundtrip() {
    let env = Env::default();
    for n in [1u32, 2, 3, 5, 8, 50] {
        let (root, leaves, _, _) = build_batch(&env, n, 100);
        for i in 0..leaves.len() {
            let leaf = leaves.get_unchecked(i);
            let proof = merkle_proof(&env, &leaves, i);
            assert!(
                merkle::verify(&env, &leaf, &proof, &root),
                "leaf {i} of {n} failed"
            );
        }
    }
}

// --- test-only Merkle helpers -------------------------------------------------

// Build a batch of `n` synthetic leaves whose amounts sum to `total`. Returns
// (root, leaves, total, count). recipient_ref stands in for a salted hash.
fn build_batch(env: &Env, n: u32, seed: u8) -> (BytesN<32>, Vec<BytesN<32>>, i128, u32) {
    let per: i128 = if n == 50 { 12 } else { 10 };
    let mut leaves = Vec::new(env);
    let mut total: i128 = 0;
    for i in 0..n {
        let mut r = [0u8; 32];
        r[0] = seed;
        r[1] = (i & 0xff) as u8;
        r[2] = (i >> 8) as u8;
        let recipient_ref = BytesN::from_array(env, &r);
        leaves.push_back(merkle::hash_leaf(env, &recipient_ref, per));
        total += per;
    }
    let root = merkle::compute_root(env, &leaves);
    (root, leaves, total, n)
}

// Sibling path for leaf `index`, matching compute_root: pairs are (2k, 2k+1)
// and an odd tail node is carried up unchanged.
fn merkle_proof(env: &Env, leaves: &Vec<BytesN<32>>, index: u32) -> Vec<BytesN<32>> {
    let mut proof = Vec::new(env);
    let mut level = leaves.clone();
    let mut idx = index;
    while level.len() > 1 {
        let mut next = Vec::new(env);
        let mut i = 0u32;
        while i < level.len() {
            if i + 1 < level.len() {
                let a = level.get_unchecked(i);
                let b = level.get_unchecked(i + 1);
                if i == idx {
                    proof.push_back(b.clone());
                } else if i + 1 == idx {
                    proof.push_back(a.clone());
                }
                next.push_back(merkle::hash_node(env, &a, &b));
                i += 2;
            } else {
                // Odd tail: carried up, contributes no sibling.
                next.push_back(level.get_unchecked(i));
                i += 1;
            }
        }
        idx /= 2;
        level = next;
    }
    proof
}
