# Changelog

All notable changes to the `cygnus-contracts` repository will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- Cross-contract interoperability helpers for automated donor streams.
- Additional property-based invariant fuzzing harnesses.

## [0.1.0] - 2026-11-09

### Added
- **Core Programme Contract**:
  - Implemented `create_programme` with sponsor authentication, non-positive amount checks, and empty milestone validation.
  - Implemented `fund` enabling partial and repeated funding up to declared total.
  - Implemented `claim` allowing implementers to submit milestone completion evidence hashes, with late-claim flagging.
  - Implemented `approve` and `approve_by` releasing milestone budgets into spendable balance.
  - Implemented `reject` and `reject_by` with reason hashes, supporting re-claim and re-review cycles.
  - Implemented `disburse` recording Merkle roots and paying out batches to implementers without publishing beneficiary lists.
  - Implemented `cancel_programme` allowing sponsors to cancel zero-funded programmes before maturity.
  - Implemented `refund_remainder` returning unspent funds to sponsors after programme end date.
- **Approver Configurations**:
  - `ApproverConfig::Single(Address)`: single authority approval/rejection.
  - `ApproverConfig::Panel(Vec<Address>, u32)`: M-of-N threshold panel where distinct members vote via `approve_by`.
  - `ApproverConfig::Oracle(Address)`: automated oracle approval and rejection.
  - Custom errors: `AlreadyApprovedByMember` (19), `NotApprover` (20), and `InvalidApproverConfig` (21).
  - Event `MilestonePanelApproved` emitted per member vote.
- **Beneficiary Privacy & Merkle Construction**:
  - Domain-separated leaf hashing: `sha256(0x00 || recipient_ref[32] || amount_be[16])`.
  - Domain-separated internal node hashing with lexicographical sorting: `sha256(0x01 || min(a,b) || max(a,b))`.
  - Odd tail propagation without duplication to prevent duplicate inclusion proofs.
  - Exposed `verify_inclusion` and canonical `hash_leaf` view entry points.
- **Safety, Resource Bounds & State Longevity**:
  - Bounded milestone vector via `MAX_MILESTONES = 25` (`TooManyMilestones = 15`).
  - Bounded batch storage via `MAX_BATCHES = 1000` (`TooManyBatches = 16`).
  - Strict preservation of the three-balance invariant: `disbursed <= released <= funded`.
  - Automatic persistent storage TTL extension: bumped to 120 days when below 30 days.
  - Granular `get_milestone` view returning `Option<(Milestone, MilestoneState)>`.
- **Testing & Verification**:
  - Expanded test suite from baseline 18 to 47 tests.
  - Property-based fuzz tests across 14 tree shapes (1..48 leaves) and mutation rejections.
  - Domain separator collision resistance tests.
  - MockAuth authorization pinning across all state-mutating functions.
- **Contributor & CI Infrastructure**:
  - GitHub Actions CI workflow covering formatting, clippy (`-D warnings`), unit tests, release WASM build, and secret checks.
  - Contributor Covenant Code of Conduct v2.1.
  - Bug report, feature request, and pull request templates.
  - Configuration template `.env.example`.

### Changed
- Refactored Merkle node hashing to avoid redundant stack conversions.
- Generalized test batch generation helpers for arbitrary leaf counts and payout values.
