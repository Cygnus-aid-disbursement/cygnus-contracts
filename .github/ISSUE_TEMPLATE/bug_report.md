---
name: Bug Report
about: Report a defect, test failure, or specification divergence in contracts
title: 'fix(contracts): '
labels: ['bug', 'triage']
assignees: ''
---

## Description
A clear and concise description of the bug or unexpected behavior.

## Location
- Contract / File: `contracts/programme/src/...`
- Function / Entry point: (e.g. `fund`, `claim`, `approve_by`, `disburse`, `refund_remainder`)

## Steps to Reproduce
1. In `contracts/programme/src/test.rs`, call `...`
2. Assert `...`
3. Observe failure or error code: `...`

## Expected Behavior
What the protocol specification (`docs/protocol.md`) states should happen.

## Relevant Invariants
Does this affect any of the core invariants?
- [ ] Three-balance invariant: `disbursed <= released <= funded`
- [ ] Authorization / Signer requirements
- [ ] Beneficiary privacy / Merkle verification
- [ ] Storage bounds (`MAX_MILESTONES`, `MAX_BATCHES`)
- [ ] Milestone lifecycle (`Pending` -> `Claimed` -> `Approved` / `Rejected`)

## Environment
- Soroban SDK Version: `27`
- Rust Toolchain: `stable`
- Operating System:
