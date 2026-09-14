# Open issues — cygnus-contracts

Unclaimed work in this repository, ordered easiest to hardest. Each carries a
difficulty label matching the scheme in `CONTRIBUTING.md`: `good first issue`,
`intermediate`, `advanced`. This list is the source of truth for what is open;
`CONTRIBUTING.md` mirrors it. To claim one, comment on the matching GitHub issue.

The largest unclaimed piece here is **panel and oracle approvers** (#6, #7),
which change the authorisation model and touch the SDK.

---

## 1. Add a `get_milestone` view — `good first issue`

`get_programme` returns the whole programme. A caller that wants one milestone
still decodes everything.

**Acceptance criteria**
- `get_milestone(programme_id, index) -> Option<(Milestone, MilestoneState)>`.
- Returns `None` for an unknown programme or an out-of-range index.
- A test covers a valid index, an out-of-range index, and an unknown programme.

## 2. Test the single-leaf batch on chain — `good first issue`

The Merkle rules say a one-leaf tree has that leaf as its root, with an empty
proof. This is covered in Rust unit tests but not against a deployed contract.

**Acceptance criteria**
- A test disburses a batch of count 1 and verifies its inclusion with an empty
  proof through `verify_inclusion`.
- The test runs under the Testnet integration gate, skipped without a funded key.

## 3. Bound milestone and batch counts — `intermediate`

Nothing caps the number of milestones per programme or batches per programme, so
a single programme's storage can grow without limit.

**Acceptance criteria**
- A documented maximum for milestones per programme and batches per programme,
  enforced with distinct errors.
- Tests for the boundary and one past it.
- `docs/protocol.md` states the limits and the reasoning.

## 4. Add `cancel_programme` before funding — `intermediate`

A sponsor who created a programme in error cannot close it until the end date. A
programme with zero funding should be cancellable immediately by the sponsor.

**Acceptance criteria**
- `cancel_programme(programme_id)`, sponsor-authorised, allowed only when
  `funded == 0`, moving the programme to `Closed`.
- A distinct error when funding is non-zero.
- Unauthorised-path and funded-path tests.

## 5. Property-based Merkle tests — `intermediate`

The Merkle tests use fixed sizes. Random trees would catch ordering and
odd-node bugs the fixed cases miss.

**Acceptance criteria**
- Property tests over random leaf counts and indices confirm every leaf verifies
  and that a tampered proof or leaf fails.
- Runs in CI without network access.

## 6. Panel approver with threshold — `advanced`

`ApproverConfig::Panel(Vec<Address>, threshold)` is declared but returns
`ApproverKindUnimplemented`. Implement threshold approval: a milestone releases
only once `threshold` distinct panel members have approved it.

**Acceptance criteria**
- `approve` and `reject` accept panel members and track distinct approvals per
  milestone until the threshold is met.
- Duplicate approvals by the same member do not count twice.
- The SDK's types and the cross-check are updated in a linked pull request.
- Tests for below-threshold, at-threshold, duplicate, and non-member paths.
- `docs/protocol.md` and `docs/threat-model.md` updated.

## 7. Oracle approver — `advanced`

`ApproverConfig::Oracle(Address)` is declared but unimplemented. A milestone
releases on a signed attestation from the named monitoring provider.

**Acceptance criteria**
- `approve` verifies an attestation authorised by the oracle address.
- The attestation format is specified in `docs/protocol.md`.
- Tests for a valid attestation, a wrong signer, and a replayed attestation.
- The SDK gains a helper to produce the attestation, in a linked pull request.

## 8. Storage TTL and cost review — `advanced`

Programme state uses a fixed TTL bump. Long-running programmes and large batch
histories need a considered TTL and archival story, plus a measurement of
per-call cost.

**Acceptance criteria**
- A short report of per-call cost and storage growth for a realistic programme.
- TTL constants justified in `docs/protocol.md`, with a documented path for
  reading a programme whose entries have expired.

## 9. Independent invariant and privacy review — `advanced`

Before any mainnet discussion, the balance invariants and the privacy properties
need review by someone who did not write them.

**Acceptance criteria**
- A written review covering the `disbursed <= released <= funded` invariant, the
  authorisation matrix, and the correlation risks in `docs/threat-model.md`.
- Findings filed as their own issues.
