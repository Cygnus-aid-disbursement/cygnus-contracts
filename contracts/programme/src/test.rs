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
fn refund_remainder_exact_boundary() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    let end_by = c.client.get_programme(&id).unwrap().end_by;
    assert_eq!(end_by, 20 * DAY);

    // 1 second before end_by must fail
    c.env.ledger().with_mut(|l| l.timestamp = end_by - 1);
    assert_eq!(
        c.client.try_refund_remainder(&id).err(),
        Some(Ok(Error::NotEnded))
    );

    // Exactly at end_by must still fail (<= end_by is disallowed)
    c.env.ledger().with_mut(|l| l.timestamp = end_by);
    assert_eq!(
        c.client.try_refund_remainder(&id).err(),
        Some(Ok(Error::NotEnded))
    );

    // Exactly 1 second past end_by must succeed
    c.env.ledger().with_mut(|l| l.timestamp = end_by + 1);
    let refunded = c.client.refund_remainder(&id);
    assert_eq!(refunded, 1_000);
    assert_eq!(
        c.client.get_programme(&id).unwrap().status,
        ProgrammeStatus::Closed
    );
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
fn claim_timestamp_due_by_boundary() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    let due0 = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestones
        .get_unchecked(0)
        .due_by;
    assert_eq!(due0, 10 * DAY);

    // Exactly at due_by (due0) should not be flagged late
    c.env.ledger().with_mut(|l| l.timestamp = due0);
    c.client.claim(&id, &0, &hash(&c.env, 10));
    let st0 = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestone_states
        .get_unchecked(0);
    assert_eq!(st0.status, MilestoneStatus::Claimed);
    assert!(!st0.claimed_late);

    // 1 second past due_by (due1 + 1) must be flagged late
    let due1 = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestones
        .get_unchecked(1)
        .due_by;
    assert_eq!(due1, 20 * DAY);

    c.env.ledger().with_mut(|l| l.timestamp = due1 + 1);
    c.client.claim(&id, &1, &hash(&c.env, 20));
    let st1 = c
        .client
        .get_programme(&id)
        .unwrap()
        .milestone_states
        .get_unchecked(1);
    assert_eq!(st1.status, MilestoneStatus::Claimed);
    assert!(st1.claimed_late);
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
fn multi_cycle_claim_reject_resubmission() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    let ev1 = hash(&c.env, 101);
    let rz1 = hash(&c.env, 102);
    let ev2 = hash(&c.env, 103);
    let rz2 = hash(&c.env, 104);
    let ev3 = hash(&c.env, 105);

    // Cycle 1: claim then reject
    c.client.claim(&id, &0, &ev1);
    let st = c.client.get_programme(&id).unwrap().milestone_states.get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Claimed);
    assert_eq!(st.evidence_hash, Some(ev1));
    assert_eq!(st.reason_hash, None);

    c.client.reject(&id, &0, &rz1);
    let st = c.client.get_programme(&id).unwrap().milestone_states.get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Rejected);
    assert_eq!(st.reason_hash, Some(rz1));

    // Cycle 2: re-claim with new evidence, reason_hash reset to None
    c.client.claim(&id, &0, &ev2);
    let st = c.client.get_programme(&id).unwrap().milestone_states.get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Claimed);
    assert_eq!(st.evidence_hash, Some(ev2));
    assert_eq!(st.reason_hash, None);

    c.client.reject(&id, &0, &rz2);
    let st = c.client.get_programme(&id).unwrap().milestone_states.get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Rejected);
    assert_eq!(st.reason_hash, Some(rz2));

    // Cycle 3: re-claim and finally approve
    c.client.claim(&id, &0, &ev3);
    c.client.approve(&id, &0);
    let st = c.client.get_programme(&id).unwrap().milestone_states.get_unchecked(0);
    assert_eq!(st.status, MilestoneStatus::Approved);
    assert_eq!(st.evidence_hash, Some(ev3));
    assert_eq!(c.client.get_programme(&id).unwrap().released, 600);
}

#[test]
fn three_balance_invariant_holds_at_each_transition() {
    let c = setup();
    let id = create(&c);
    let token = soroban_sdk::token::Client::new(&c.env, &c.asset);

    // Initial state
    let p = c.client.get_programme(&id).unwrap();
    assert!(p.disbursed <= p.released && p.released <= p.funded);
    assert_eq!(token.balance(&c.contract_id), 0);

    // Step 1: Fund 400
    c.client.fund(&id, &400);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.funded, 400);
    assert!(p.disbursed <= p.released && p.released <= p.funded);
    assert_eq!(token.balance(&c.contract_id), 400);

    // Step 2: Milestone 0 (amount 600) cannot be approved yet
    c.client.claim(&id, &0, &hash(&c.env, 10));
    assert_eq!(
        c.client.try_approve(&id, &0).err(),
        Some(Ok(Error::InsufficientFunding))
    );

    // Step 3: Fund another 300 (total funded = 700)
    c.client.fund(&id, &300);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.funded, 700);
    assert_eq!(token.balance(&c.contract_id), 700);

    // Step 4: Approve milestone 0 (releases 600)
    c.client.approve(&id, &0);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.released, 600);
    assert!(p.disbursed <= p.released && p.released <= p.funded);

    // Step 5: Disburse batch 0 of 250
    let (root0, _, total0, count0) = build_batch_with_amount(&c.env, 25, 1, 10);
    assert_eq!(total0, 250);
    c.client.disburse(&id, &root0, &total0, &count0);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.disbursed, 250);
    assert!(p.disbursed <= p.released && p.released <= p.funded);
    assert_eq!(token.balance(&c.contract_id), 450);
    assert_eq!(token.balance(&c.implementer), 250);

    // Step 6: Disburse batch 1 of 300
    let (root1, _, total1, count1) = build_batch_with_amount(&c.env, 30, 2, 10);
    assert_eq!(total1, 300);
    c.client.disburse(&id, &root1, &total1, &count1);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.disbursed, 550);
    assert!(p.disbursed <= p.released && p.released <= p.funded);
    assert_eq!(token.balance(&c.contract_id), 150);

    // Step 7: Attempting to disburse 100 exceeds released (550 + 100 > 600)
    let (root_fail, _, _, count_fail) = build_batch_with_amount(&c.env, 10, 3, 10);
    assert_eq!(
        c.client
            .try_disburse(&id, &root_fail, &100, &count_fail)
            .err(),
        Some(Ok(Error::InsufficientReleased))
    );

    // Step 8: Fund remaining 300 and approve milestone 1 (amount 400)
    c.client.claim(&id, &1, &hash(&c.env, 20));
    c.client.fund(&id, &300);
    c.client.approve(&id, &1);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.funded, 1_000);
    assert_eq!(p.released, 1_000);
    assert_eq!(p.disbursed, 550);
    assert!(p.disbursed <= p.released && p.released <= p.funded);
    assert_eq!(token.balance(&c.contract_id), 450);

    // Step 9: Disburse remaining 450 in batch 2
    let (root2, _, total2, count2) = build_batch_with_amount(&c.env, 45, 4, 10);
    assert_eq!(total2, 450);
    c.client.disburse(&id, &root2, &total2, &count2);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.disbursed, 1_000);
    assert!(p.disbursed <= p.released && p.released <= p.funded);
    assert_eq!(token.balance(&c.contract_id), 0);
    assert_eq!(token.balance(&c.implementer), 1_000);
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

#[test]
fn single_leaf_batch_verification_on_chain() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);

    // Build a batch with exactly 1 recipient
    let (root, leaves, total, count) = build_batch_with_amount(&c.env, 1, 99, 150);
    assert_eq!(count, 1);
    assert_eq!(total, 150);
    let single_leaf = leaves.get_unchecked(0);
    // In a 1-leaf tree, root is identical to the leaf
    assert_eq!(root, single_leaf);

    let batch_idx = c.client.disburse(&id, &root, &total, &count);
    assert_eq!(batch_idx, 0);

    // Single-leaf inclusion proof must be empty
    let empty_proof = Vec::new(&c.env);
    assert!(c
        .client
        .verify_inclusion(&id, &batch_idx, &single_leaf, &empty_proof));

    // A non-empty proof against a single-leaf tree must fail
    let spurious_proof = vec![&c.env, hash(&c.env, 55)];
    assert!(!c
        .client
        .verify_inclusion(&id, &batch_idx, &single_leaf, &spurious_proof));

    // A tampered leaf must fail
    let wrong_leaf = hash(&c.env, 88);
    assert!(!c
        .client
        .verify_inclusion(&id, &batch_idx, &wrong_leaf, &empty_proof));
}

#[test]
fn merkle_odd_counts_and_sibling_ordering() {
    let env = Env::default();

    // Verify all odd leaf counts up to 33 carry up tail without duplication
    for n in [3u32, 5, 7, 9, 13, 17, 33] {
        let (root, leaves, _, _) = build_batch(&env, n, 42);
        for i in 0..leaves.len() {
            let leaf = leaves.get_unchecked(i);
            let proof = merkle_proof(&env, &leaves, i);
            assert!(
                merkle::verify(&env, &leaf, &proof, &root),
                "leaf {i} of {n} failed"
            );

            // If proof has at least 2 siblings, swapping them must fail verification
            if proof.len() >= 2 {
                let mut swapped_proof = proof.clone();
                let s0 = swapped_proof.get_unchecked(0);
                let s1 = swapped_proof.get_unchecked(1);
                swapped_proof.set(0, s1);
                swapped_proof.set(1, s0);
                assert!(
                    !merkle::verify(&env, &leaf, &swapped_proof, &root),
                    "swapped siblings unexpectedly verified for leaf {i} of {n}"
                );
            }

            // Tampering with the first sibling must fail verification
            if proof.len() > 0 {
                let mut tampered_proof = proof.clone();
                let bad_sibling = hash(&env, 254);
                tampered_proof.set(0, bad_sibling);
                assert!(
                    !merkle::verify(&env, &leaf, &tampered_proof, &root),
                    "tampered sibling unexpectedly verified for leaf {i} of {n}"
                );
            }
        }
    }
}

#[test]
fn views_and_hash_leaf() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);

    // Test hash_leaf client endpoint matches merkle module
    let r_ref = hash(&c.env, 77);
    let expected_leaf = merkle::hash_leaf(&c.env, &r_ref, 250);
    let client_leaf = c.client.hash_leaf(&r_ref, &250);
    assert_eq!(client_leaf, expected_leaf);

    // Disburse batch 0
    let (root, _, total, count) = build_batch(&c.env, 2, 80);
    let batch_idx = c.client.disburse(&id, &root, &total, &count);
    assert_eq!(batch_idx, 0);

    // get_batch returns Some for existing batch, None for unknown
    let batch = c.client.get_batch(&id, &batch_idx).unwrap();
    assert_eq!(batch.index, 0);
    assert_eq!(batch.root, root);
    assert_eq!(batch.total, total);
    assert_eq!(batch.count, count);
    assert!(c.client.get_batch(&id, &99).is_none());
    assert!(c.client.get_batch(&999, &0).is_none());

    // get_programme returns Some for existing, None for unknown
    assert!(c.client.get_programme(&id).is_some());
    assert!(c.client.get_programme(&999).is_none());
}

#[test]
fn programme_not_found_on_all_mutations() {
    let c = setup();
    let missing_id = 999_u64;

    assert_eq!(
        c.client.try_fund(&missing_id, &100).err(),
        Some(Ok(Error::ProgrammeNotFound))
    );
    assert_eq!(
        c.client.try_claim(&missing_id, &0, &hash(&c.env, 1)).err(),
        Some(Ok(Error::ProgrammeNotFound))
    );
    assert_eq!(
        c.client.try_approve(&missing_id, &0).err(),
        Some(Ok(Error::ProgrammeNotFound))
    );
    assert_eq!(
        c.client.try_reject(&missing_id, &0, &hash(&c.env, 1)).err(),
        Some(Ok(Error::ProgrammeNotFound))
    );
    assert_eq!(
        c.client
            .try_disburse(&missing_id, &hash(&c.env, 1), &100, &1)
            .err(),
        Some(Ok(Error::ProgrammeNotFound))
    );
    assert_eq!(
        c.client.try_refund_remainder(&missing_id).err(),
        Some(Ok(Error::ProgrammeNotFound))
    );
}

#[test]
fn closed_programme_rejects_mutations() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    // Fast-forward past end date and refund to close
    c.env.ledger().with_mut(|l| l.timestamp = 25 * DAY);
    let refunded = c.client.refund_remainder(&id);
    assert_eq!(refunded, 1_000);
    assert_eq!(
        c.client.get_programme(&id).unwrap().status,
        ProgrammeStatus::Closed
    );

    // Every mutation must now be rejected with AlreadyClosed
    assert_eq!(
        c.client.try_fund(&id, &100).err(),
        Some(Ok(Error::AlreadyClosed))
    );
    assert_eq!(
        c.client.try_claim(&id, &0, &hash(&c.env, 10)).err(),
        Some(Ok(Error::AlreadyClosed))
    );
    assert_eq!(
        c.client.try_approve(&id, &0).err(),
        Some(Ok(Error::AlreadyClosed))
    );
    assert_eq!(
        c.client.try_reject(&id, &0, &hash(&c.env, 10)).err(),
        Some(Ok(Error::AlreadyClosed))
    );
    assert_eq!(
        c.client
            .try_disburse(&id, &hash(&c.env, 10), &100, &1)
            .err(),
        Some(Ok(Error::AlreadyClosed))
    );
    assert_eq!(
        c.client.try_refund_remainder(&id).err(),
        Some(Ok(Error::AlreadyClosed))
    );
}

#[test]
fn milestone_index_out_of_range() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    // Milestones length is 2 (valid indices: 0, 1)
    let oob_index = 2_u32;
    assert_eq!(
        c.client.try_claim(&id, &oob_index, &hash(&c.env, 1)).err(),
        Some(Ok(Error::MilestoneIndexOutOfRange))
    );
    assert_eq!(
        c.client.try_approve(&id, &oob_index).err(),
        Some(Ok(Error::MilestoneIndexOutOfRange))
    );
    assert_eq!(
        c.client.try_reject(&id, &oob_index, &hash(&c.env, 1)).err(),
        Some(Ok(Error::MilestoneIndexOutOfRange))
    );
}

#[test]
fn milestone_resolution_states() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    // Rejecting a pending milestone returns MilestoneNotClaimed
    assert_eq!(
        c.client.try_reject(&id, &0, &hash(&c.env, 99)).err(),
        Some(Ok(Error::MilestoneNotClaimed))
    );

    // Claim and approve milestone 0
    c.client.claim(&id, &0, &hash(&c.env, 10));
    c.client.approve(&id, &0);

    // Once Approved, both approve and reject return MilestoneAlreadyResolved
    assert_eq!(
        c.client.try_approve(&id, &0).err(),
        Some(Ok(Error::MilestoneAlreadyResolved))
    );
    assert_eq!(
        c.client.try_reject(&id, &0, &hash(&c.env, 99)).err(),
        Some(Ok(Error::MilestoneAlreadyResolved))
    );

    // Claim and reject milestone 1
    c.client.claim(&id, &1, &hash(&c.env, 20));
    c.client.reject(&id, &1, &hash(&c.env, 30));

    // Once Rejected, re-rejecting returns MilestoneAlreadyResolved
    assert_eq!(
        c.client.try_reject(&id, &1, &hash(&c.env, 31)).err(),
        Some(Ok(Error::MilestoneAlreadyResolved))
    );
}

#[test]
fn refund_remainder_nothing_to_refund() {
    let c = setup();
    let id = create(&c);
    c.client.fund(&id, &1_000);

    // Claim and approve milestone 0 (600) and milestone 1 (400)
    c.client.claim(&id, &0, &hash(&c.env, 10));
    c.client.approve(&id, &0);
    c.client.claim(&id, &1, &hash(&c.env, 20));
    c.client.approve(&id, &1);

    // Disburse all 1,000 across two batches (600 + 400)
    let (root0, _, total0, count0) = build_batch_with_amount(&c.env, 50, 10, 12);
    assert_eq!(total0, 600);
    c.client.disburse(&id, &root0, &total0, &count0);
    let (root1, _, total1, count1) = build_batch_with_amount(&c.env, 40, 20, 10);
    assert_eq!(total1, 400);
    c.client.disburse(&id, &root1, &total1, &count1);

    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.funded, 1_000);
    assert_eq!(p.disbursed, 1_000);

    // Fast-forward past end date
    c.env.ledger().with_mut(|l| l.timestamp = 25 * DAY);
    assert_eq!(
        c.client.try_refund_remainder(&id).err(),
        Some(Ok(Error::NothingToRefund))
    );
}

#[test]
fn non_positive_amounts_rejected() {
    let c = setup();

    // Negative total at programme creation
    let e = c
        .client
        .try_create_programme(
            &c.sponsor,
            &c.implementer,
            &c.asset,
            &-500,
            &milestones(&c.env),
            &ApproverConfig::Single(c.approver.clone()),
            &hash(&c.env, 9),
        )
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InvalidAmount));

    // Milestone with zero amount
    let zero_milestones = vec![
        &c.env,
        Milestone {
            description_hash: hash(&c.env, 1),
            amount: 0,
            due_by: 10 * DAY,
        },
    ];
    let e = c
        .client
        .try_create_programme(
            &c.sponsor,
            &c.implementer,
            &c.asset,
            &1_000,
            &zero_milestones,
            &ApproverConfig::Single(c.approver.clone()),
            &hash(&c.env, 9),
        )
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InvalidAmount));

    // Milestone with negative amount
    let neg_milestones = vec![
        &c.env,
        Milestone {
            description_hash: hash(&c.env, 1),
            amount: -100,
            due_by: 10 * DAY,
        },
    ];
    let e = c
        .client
        .try_create_programme(
            &c.sponsor,
            &c.implementer,
            &c.asset,
            &1_000,
            &neg_milestones,
            &ApproverConfig::Single(c.approver.clone()),
            &hash(&c.env, 9),
        )
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InvalidAmount));

    // Negative fund amount
    let id = create(&c);
    let e = c.client.try_fund(&id, &-100).err().unwrap();
    assert_eq!(e, Ok(Error::InvalidAmount));

    // Negative disburse total
    c.client.fund(&id, &1_000);
    c.client.claim(&id, &0, &hash(&c.env, 20));
    c.client.approve(&id, &0);
    let e = c
        .client
        .try_disburse(&id, &hash(&c.env, 50), &-50, &5)
        .err()
        .unwrap();
    assert_eq!(e, Ok(Error::InvalidBatch));
}

#[test]
fn claim_before_funding_and_approve_after_funding() {
    let c = setup();
    let id = create(&c);

    // Implementer can claim milestone 0 before any funding is sent
    c.client.claim(&id, &0, &hash(&c.env, 15));
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(
        p.milestone_states.get_unchecked(0).status,
        MilestoneStatus::Claimed
    );

    // Approver cannot approve yet because released(0) + amount(600) > funded(0)
    assert_eq!(
        c.client.try_approve(&id, &0).err(),
        Some(Ok(Error::InsufficientFunding))
    );

    // Sponsor funds 600
    c.client.fund(&id, &600);

    // Approver can now approve
    c.client.approve(&id, &0);
    let p = c.client.get_programme(&id).unwrap();
    assert_eq!(p.released, 600);
    assert_eq!(
        p.milestone_states.get_unchecked(0).status,
        MilestoneStatus::Approved
    );
}

// --- test-only Merkle helpers -------------------------------------------------

// Build a batch of `n` synthetic leaves with a specified amount per leaf.
fn build_batch_with_amount(
    env: &Env,
    n: u32,
    seed: u8,
    per: i128,
) -> (BytesN<32>, Vec<BytesN<32>>, i128, u32) {
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

// Build a batch of `n` synthetic leaves whose amounts sum to `total`. Returns
// (root, leaves, total, count). recipient_ref stands in for a salted hash.
fn build_batch(env: &Env, n: u32, seed: u8) -> (BytesN<32>, Vec<BytesN<32>>, i128, u32) {
    let per: i128 = if n == 50 { 12 } else { 10 };
    build_batch_with_amount(env, n, seed, per)
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
