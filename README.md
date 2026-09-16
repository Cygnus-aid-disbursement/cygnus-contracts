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

Deployed Testnet contract id: `CBN6MNOWZISG6EXVCUQSGKASFSIBVDBZSVASI6FZYBLBMLHMQWL6SAL7`.

What works today: the full single-approver lifecycle. A sponsor creates and funds
a programme, an implementer claims a milestone, a single named approver approves
or rejects it, the implementer disburses a batch as a Merkle root with a total
and a count, anyone can verify an inclusion proof against the on-chain root, and
the sponsor refunds the remainder after the end date. This path is covered by
unit tests and by a Testnet run reproduced below.

What does not work yet: the panel approver (`ApproverConfig::Panel`) and the
oracle approver (`ApproverConfig::Oracle`) are declared in the type but return
`ApproverKindUnimplemented`; only `ApproverConfig::Single` is live. There are no
per-programme caps on milestone or batch counts, no `cancel_programme` for a
zero-funded programme, and no independent review of the balance invariants or the
privacy properties. These gaps are tracked as GitHub issues.

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

## Verified Testnet run

`scripts/demo.sh` run against the deployed contract on 2026-09-16. The transcript
below is the Stellar CLI output with its emoji decoration removed; the values,
transaction hashes and events are unchanged. Each hash resolves on
stellar.expert. The claim lands after the milestone due date because the demo
sets due dates in the past so a single run can reach the refund step; the
contract flags this as `claimed_late: true` rather than blocking it.

```
== Cygnus demo ==
contract:    CBN6MNOWZISG6EXVCUQSGKASFSIBVDBZSVASI6FZYBLBMLHMQWL6SAL7
asset:       CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC (native)
sponsor:     GBFV5WVMLXMBQ7TAGR77YMP7MKI7SUVQRAUFZSSCYJNY73VB5CKUEYQ7
implementer: GCDKCNNQFZCT5TC5XVSAQ3RXUNRD3F6UU4YR2AJPKUFGRGN5I62FVSM4
approver:    GANM7DX62H46B7IPIQPLL6D5FOJNM27NM6HZINDCVNZOCTLVKFPOHBDL

1. create_programme (total 1000, two milestones 600 + 400)
   tx: https://stellar.expert/explorer/testnet/tx/6b7104ec5a52da8b3cbaa138c7bd6c4f982f4b99f6234ba8bf1a4932df055270
   Event: ProgrammeCreated, programme_id: 4, total: "1000"
   programme id: 4

2. fund (1000)
   tx: https://stellar.expert/explorer/testnet/tx/3d61467f5dda0e10101ca331505b0aa4c03415be8c1c1af34b3dad4e505afe99
   Event: transfer 1000 native -> contract
   Event: ProgrammeFunded, programme_id: 4, amount: "1000"

3. claim milestone 0 (implementer)
   tx: https://stellar.expert/explorer/testnet/tx/43b862acfef0c8e1ffa32476352f550df49b9bdeb93cfed90dbcd9821a137f94
   Event: MilestoneClaimed, programme_id: 4, milestone_index: 0, claimed_late: true

4. approve milestone 0 (approver, releases 600)
   tx: https://stellar.expert/explorer/testnet/tx/0b935da09527bc7e8b147516d77b731dc8f2648546726e6b11f0987151c9594c
   Event: MilestoneApproved, programme_id: 4, milestone_index: 0

5. build a 50-beneficiary batch (600 total, 12 each) and disburse
   root:  9ff7668509b1bea2e6d98749045844906b94ad275dce24dbe2105418456c0446
   total: 600  count: 50
   tx: https://stellar.expert/explorer/testnet/tx/fb79c45ab5f7d3f74e2ebb6dab5223630bf9d912c8555e668ef916b530996487
   Event: transfer 600 native contract -> implementer
   Event: BatchDisbursed, programme_id: 4, batch_index: 0, total: "600", count: 50
   batch index: 0

6. verify one beneficiary's inclusion proof (must be true)
   verify_inclusion: true

7. refund_remainder (sponsor recovers 400)
   tx: https://stellar.expert/explorer/testnet/tx/602da0f7c2aebdd52c307093313fe4793506a5b8e4564f1951d2917154f7d8bd
   Event: transfer 400 native contract -> sponsor
   Event: ProgrammeRefunded, programme_id: 4, amount: "400"
   refunded: "400"

== Demo complete for programme 4 ==
```

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Open unclaimed work is tracked in
[ISSUES.md](ISSUES.md). Report security issues privately per
[SECURITY.md](SECURITY.md), never in a public issue.

## License

Apache-2.0. See [LICENSE](LICENSE).
