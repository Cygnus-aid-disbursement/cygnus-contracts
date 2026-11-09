# Open Issues & Roadmap

Unclaimed work is tracked in GitHub Issues:
https://github.com/Cygnus-aid-disbursement/cygnus-contracts/issues

Each issue carries a difficulty label (`good first issue`, `intermediate`, `advanced`) and area labels. To claim an issue, comment on it. See [CONTRIBUTING.md](CONTRIBUTING.md) for how the labels work and the rules a pull request must meet.

---

### Roadmap Backlog (~10% Remaining Scope)

#### 1. Automated Stream Funding Adapter
- **Labels**: `enhancement`, `intermediate`
- **Description**: Design an adapter contract that permits recurring funding from continuous donation streams or smart escrows directly into a programme's spendable pool via `fund`.
- **Acceptance Criteria**:
  - Implements authorization checks preserving sponsor control.
  - Maintains the three-balance invariant: `disbursed <= released <= funded`.
  - Comprehensive unit tests simulating continuous and discrete funding flows.

#### 2. Event Parsing Reference & Indexer Benchmarks
- **Labels**: `good first issue`, `documentation`
- **Description**: Provide standalone TypeScript and Rust reference parsers for contract events (`MilestonePanelApproved`, `BatchDisbursed`, `ProgrammeCancelled`, `ProgrammeRefunded`) to assist downstream indexer builders.
- **Acceptance Criteria**:
  - Example scripts decoding event data from transaction envelopes.
  - Verification against Testnet deployment history in `deployments/testnet.json`.

#### 3. Formal Invariant Verification (Halmos / Certora)
- **Labels**: `security`, `advanced`
- **Description**: Formulate mathematical proofs using Halmos or Certora Prover establishing that `disbursed <= released <= funded` holds across all reachable contract states and arbitrarily ordered function invocations.
- **Acceptance Criteria**:
  - Formal specification rules in a `specs/` directory.
  - Proof verification runs in CI on pull requests touching balance logic.

#### 4. Multi-Token Disbursement Baskets Proposal
- **Labels**: `protocol`, `advanced`
- **Description**: RFC and architectural draft for programmes requiring disbursements in multiple stable assets (e.g., USDC + local fiat-backed tokens) under a single milestone agreement.
- **Acceptance Criteria**:
  - Written proposal in `docs/` detailing data structure changes and storage footprint.
  - Maintaining Merkle privacy guarantees without cross-asset linkage.
