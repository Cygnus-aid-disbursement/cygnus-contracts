# cygnus-contracts

The programme contract for Cygnus, a humanitarian aid disbursement protocol on
Stellar's Soroban platform. A sponsor funds a programme, an implementer claims
against milestones, an approver releases each milestone budget, and the
implementer disburses to beneficiaries in batches. The whole flow is publicly
auditable without publishing who the beneficiaries are.

This is the protocol repository, the first of three:

- **cygnus-contracts** (here) — the Rust contract, deployment truth, and the
  protocol specification.
- [cygnus-sdk](https://github.com/Cygnus-aid-disbursement/cygnus-sdk) — the
  TypeScript client, Merkle batch construction, and hashing conventions.
- [cygnus-app](https://github.com/Cygnus-aid-disbursement/cygnus-app) — the
  public dashboard and implementer console.

Dependencies point one way: contracts, then SDK, then app. See
[docs/multi-repo.md](docs/multi-repo.md).

## What the chain proves, and what it does not

Cygnus records that funds moved from one party to another under a stated rule: a
programme was funded, a milestone was claimed and approved, and a batch of a
given total and count was disbursed. That is what the ledger proves.

The chain does **not** prove that goods were delivered, that services were
rendered, or that the right people received aid. A disbursement batch commits to
a list of payouts with a Merkle root; it says money left the programme for that
many recipients totalling that amount. It cannot say those recipients existed,
needed help, or received anything of value. Judging that remains the work of
auditors, monitors, and the communities served. A dashboard that implies
otherwise is transparency theatre, and it does real harm by lending unearned
credibility. We state the limit plainly here and everywhere in the product.

## Beneficiary privacy

Individual payouts never go on chain. A disbursement publishes a Merkle root, a
total, and a count. A beneficiary holds a receipt that lets them prove their own
inclusion if they ever need to; nobody can enumerate the list. Recipient
references are salted before they are hashed into a leaf, so the same person
cannot be correlated across programmes. In many contexts a public list of aid
recipients is a safety risk, so this is a hard design constraint, not a
preference. The contract has no function that accepts or emits a beneficiary
identifier, because none should exist on chain to accept or emit.

## Status

Unaudited. Testnet only. There is no mainnet configuration. Do not use this to
move real value.

## Layout

```
contracts/programme   the programme contract (create, fund, claim, approve,
                      reject, disburse, refund, Merkle verification)
docs/protocol.md      normative specification the code follows
docs/threat-model.md  attacker model and what the protocol does not defend
docs/multi-repo.md    the contract between the three repositories
scripts/              deploy and demo scripts
deployments/          deployment truth (testnet.json)
```

## Quick start

Prerequisites: Rust stable with the `wasm32v1-none` target, the
[Stellar CLI](https://developers.stellar.org/docs/tools/cli) 27 or later, and
Node 20 or later for the demo's Merkle helper.

```bash
rustup target add wasm32v1-none
cargo test                                   # 18 tests, no network needed
stellar keys generate cygnus-deployer --network testnet --fund
bash scripts/deploy_testnet.sh               # writes deployments/testnet.json
bash scripts/demo.sh                         # full cycle on Testnet
```

`scripts/demo.sh` creates a programme, funds it, claims and approves a milestone,
disburses a batch of 50 synthetic beneficiaries, verifies one inclusion proof
against the on-chain root, and refunds the remainder to the sponsor.

## Current Testnet deployment

See [deployments/testnet.json](deployments/testnet.json) for the live contract
id. The SDK vendors this file, so an application never hardcodes a contract id.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Open unclaimed work is tracked in
[ISSUES.md](ISSUES.md). Report security issues privately per
[SECURITY.md](SECURITY.md), never in a public issue.

## License

Apache-2.0. See [LICENSE](LICENSE).
