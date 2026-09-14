# Security policy

Cygnus is unaudited and Testnet only. There is no mainnet configuration. Do not
use it to move real value.

## Reporting a vulnerability

Report privately to **security@cygnus-aid.org**. Do not open a public issue for a
suspected vulnerability; a public report can put beneficiaries or funds at risk
before a fix exists.

Include what you were doing, what you observed, and how to reproduce it. If your
report concerns an authorisation check, name the item in `docs/threat-model.md`
it affects. We will acknowledge your report and keep you updated as we work on a
fix. Please give us reasonable time to respond before any public disclosure.

## Sensitive surfaces in this repository

- **Authorisation.** Every state-changing call requires the right party's
  signature: sponsor for `fund` and `refund_remainder`, implementer for `claim`
  and `disburse`, approver for `approve` and `reject`. A missing or wrong check
  is a critical bug.
- **Balance accounting.** The invariant `disbursed <= released <= funded` must
  hold. Any path that lets disbursement exceed released funds, or release exceed
  funding, is critical.
- **Merkle verification.** `verify_inclusion` and the leaf and node encoding in
  `docs/protocol.md` are the boundary of beneficiary privacy and inclusion
  proofs. A mismatch with the SDK's construction breaks proofs; a flaw in domain
  separation could allow a forged inclusion.
- **Beneficiary privacy.** No function may accept or emit a beneficiary
  identifier. A change that puts a raw or unsalted reference on chain is a
  privacy breach, not a feature.

## Scope

This policy covers the programme contract and its scripts in this repository.
The SDK and app carry their own `SECURITY.md` with the same reporting address.
