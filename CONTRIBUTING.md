# Contributing to cygnus-contracts

Welcome. This repository holds the on-chain heart of Cygnus, and a first-time
contributor who has never touched Stellar should be able to read this file and
open a useful pull request the same day. If anything here is unclear, that is a
bug in this document; please say so.

## What this repository is, and the other two

Cygnus is a humanitarian aid disbursement protocol. A sponsor funds a programme,
an implementer (an NGO or field office) claims funds against milestones, an
approver releases each milestone budget, and the implementer disburses to
beneficiaries in batches that are publicly auditable without naming anyone.

Cygnus ships as three repositories:

- **cygnus-contracts** (this one) — the Rust programme contract, the deploy
  scripts, the deployment truth in `deployments/`, and the normative protocol
  specification.
- [cygnus-sdk](https://github.com/Cygnus-aid-disbursement/cygnus-sdk) — the
  TypeScript client, Merkle batch construction, and hashing conventions.
- [cygnus-app](https://github.com/Cygnus-aid-disbursement/cygnus-app) — the
  public dashboard and the implementer console.

Dependencies point one way: **contracts, then SDK, then app, never reversed.**
This repository depends on nothing else in the project. See
[docs/multi-repo.md](docs/multi-repo.md) for the full contract between the three.

## The domain in two minutes

Four ideas carry most of the design:

- **Programme.** A funded agreement with a sponsor, an implementer, an approver,
  and a list of milestones. Funds sit in the contract, never in a platform
  account.
- **Milestone.** A unit of work with an amount and a due date. The implementer
  claims it with an evidence hash; the approver approves or rejects it. Approval
  moves that milestone's budget from merely funded to spendable.
- **Disbursement batch.** A payout to many beneficiaries at once, published as a
  Merkle root, a total, and a count. Individual payouts never go on chain.
- **What the chain proves.** That funds moved under a stated rule. Not that goods
  arrived or that the right people were paid. Never write copy that implies the
  second.

The normative document is [docs/protocol.md](docs/protocol.md). The code follows
it; where they disagree, the document and the contract's tests decide.

## Repository map

```
contracts/programme/
  src/lib.rs        contract entry points and storage
  src/types.rs      Programme, Milestone, batch, and enum types
  src/merkle.rs     leaf and node hashing, root, and proof verification
  src/error.rs      the error-code table (mirrors docs/protocol.md section 6)
  src/test.rs       unit tests, no network needed
docs/
  protocol.md       normative specification
  threat-model.md   attacker model and non-goals
  multi-repo.md     the three-repository contract
scripts/
  deploy_testnet.sh deploys and writes deployments/testnet.json
  demo.sh           a full programme cycle on Testnet
  merkle_root.mjs   self-contained Merkle helper for the demo
deployments/
  testnet.json      deployment truth; the SDK vendors this
```

Nothing here is generated. The SDK generates its bindings from this contract's
interface; that happens in the SDK repository, not here.

## Getting set up

Prerequisites:

- Rust stable (1.84 or later) with the `wasm32v1-none` target.
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli) 27 or later.
- Node 20 or later, used only by the demo's Merkle helper.

From a clone to a passing test run:

```bash
rustup target add wasm32v1-none
cargo test
```

`cargo test` runs the full unit suite with no network and no deployment of your
own. That is the fastest proof your setup works.

To exercise the contract on Testnet you need a funded key. The Stellar CLI can
create and fund one in a single step:

```bash
stellar keys generate cygnus-deployer --network testnet --fund
bash scripts/deploy_testnet.sh    # writes deployments/testnet.json
bash scripts/demo.sh              # create, fund, claim, approve, disburse, verify, refund
```

`scripts/demo.sh` runs a whole programme cycle: it creates a programme, funds it,
claims and approves a milestone, disburses a batch of 50 synthetic beneficiaries,
verifies one inclusion proof against the on-chain root, and refunds the
remainder. When it prints "Demo complete", your environment is fully working.

## Where to start

The core protocol milestones delivered in v0.1.0 include:
- `get_milestone` granular view endpoint
- Single-leaf and odd-count Merkle batch inclusion proofs
- Storage resource bounds (`MAX_MILESTONES = 25`, `MAX_BATCHES = 1000`)
- Early programme cancellation (`cancel_programme`) for unfunded programmes
- Property-based fuzz tests across arbitrary Merkle tree shapes and proof mutations
- Multi-party threshold panel approvers (`ApproverConfig::Panel`) with vote tracking
- External oracle approvers (`ApproverConfig::Oracle`)
- Storage TTL extension and rent lifecycle management

### Open contribution opportunities

Active contribution opportunities are tracked in [GitHub Issues](https://github.com/Cygnus-aid-disbursement/cygnus-contracts/issues) with difficulty labels:
1. **Automated stream funding adapter** — `intermediate`. Integrate with recurring donor payment streams.
2. **Indexer event benchmarks** — `good first issue`. Create reference parsers for `MilestonePanelApproved` and batch disbursement events.
3. **Formal verification & Certora/Halmos proofs** — `advanced`. Mathematically prove the invariant `disbursed <= released <= funded`.
4. **Cross-contract verification helper** — `advanced`. Build a companion contract that consumes `hash_leaf` and `verify_inclusion`.

To claim an issue, comment on it. For anything that changes the protocol, open a
discussion first: protocol changes need a written proposal and maintainer
sign-off before implementation, per [docs/multi-repo.md](docs/multi-repo.md).

## Rules that matter here

A reviewer will send a pull request back for any of these:

- **No beneficiary identifier on chain.** No function may accept or emit a raw or
  unsalted beneficiary reference. Batches carry a root, a total, and a count
  only. This protects real people; it is not negotiable.
- **The balance invariant holds.** `disbursed <= released <= funded` at all
  times. Any path that breaks it is a critical bug.
- **Every new state-changing function needs an unauthorised-path test.** If you
  add a call, prove that the wrong caller cannot make it.
- **Amounts are integer `i128`.** No floats anywhere in money handling.
- **Ledger time only.** Use `env.ledger().timestamp()`; never trust a
  caller-supplied time.
- **No wording implies delivery.** No comment, error, or event may suggest the
  chain proves goods arrived or that recipients received aid.

## Code style

Rust, formatted with `rustfmt`, linted with `clippy`, tested with the built-in
test harness. Commits follow [Conventional Commits](https://www.conventionalcommits.org):
`feat:`, `fix:`, `docs:`, `test:`, `chore:`, and so on. Branch names read like
`feat/panel-approver` or `fix/refund-rounding`.

CI runs exactly these, and they must pass:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --target wasm32v1-none --release
```

## Pull request checklist

- [ ] `cargo fmt --all --check` is clean.
- [ ] `cargo clippy --all-targets -- -D warnings` is clean.
- [ ] `cargo test` passes.
- [ ] `cargo build --target wasm32v1-none --release` succeeds.
- [ ] New or changed behaviour has tests, including an unauthorised path for any
      new state-changing function.
- [ ] `docs/protocol.md` is updated if the interface, errors, or events changed.
- [ ] If the change touches the Merkle or hashing conventions, the SDK's
      construction and cross-check are updated in a linked pull request.
- [ ] If the change affects an authorisation check, the description names the
      `docs/threat-model.md` item.
- [ ] Any dependent pull request in another repository is linked.

## Releases

Maintainers cut releases. A release is a git tag such as `v0.1.0`; tagging builds
the WASM, attaches it to a GitHub release with its sha256, and lets the SDK point
at that protocol version. Contributors must not edit files in `deployments/` in a
feature pull request; deployment is a maintainer step in the release flow
described in docs/multi-repo.md.

## Security

Report vulnerabilities privately per [SECURITY.md](SECURITY.md), never in a
public issue. The sensitive surfaces here are authorisation, balance accounting,
Merkle verification, and beneficiary privacy. This project is unaudited and
Testnet only.

## Community

All contributors are expected to uphold our [Code of Conduct](CODE_OF_CONDUCT.md).
Design discussion happens in GitHub Discussions in this repository; protocol
changes start there. Two merged pull requests earn triage rights on request.
Commit rights on a protocol repository are granted slowly, only after contract
review work, because a bug here moves money. Application repositories grant commit
rights more readily; this one does not. Be kind, be specific, and keep pull
requests small so they can be reviewed quickly.
