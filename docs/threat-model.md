# Threat model

This document names the attackers the Cygnus programme contract is built to
resist, what each can and cannot do, and where the protocol deliberately stops.
A pull request that changes an authorisation check must say which item here it
affects.

## Assets worth protecting

- Programme funds held in the contract between funding and disbursement.
- The integrity of the public record: balances, milestone states, and batch
  roots that an auditor relies on.
- Beneficiary privacy: the fact that a specific person received a payout.

## Actors and trust

The sponsor, implementer, and approver are named at programme creation and
authorise their own actions with their keys. None of them is trusted blindly;
the contract enforces what each may do and when. There is no platform account
and no administrator who can move funds.

## Approver capture

Whoever approves milestones controls when money is released. A captured or
colluding approver can release a milestone that was not genuinely met.

The contract limits the damage rather than preventing it: funds can only reach an
implementer address the sponsor agreed to at creation, every approval is public
with its evidence hash, and released funds still cannot exceed what was funded.
The stronger mitigations, panel and oracle approvers with a threshold, are
declared in `ApproverConfig` but not built at the 65% line; only the `Single`
approver is active, and the others return `ApproverKindUnimplemented`. A
single-approver programme carries exactly the trust its sponsor placed in that
one approver.

## Dishonest implementer

An implementer might claim a milestone that was not met, or disburse a batch
whose payouts never happened.

The contract cannot know what happened off chain, so it does not pretend to. It
ensures an implementer cannot disburse more than an approver released, cannot
disburse before approval, and cannot change the sponsor or the recipient of
refunds. Everything an implementer does is public and attributable. Whether the
claimed work occurred is a question for monitors and auditors, and the README
says so plainly. This is the boundary of what the chain proves.

## Correlation attacks on beneficiary privacy

An observer wants to learn who received aid. Publishing a list would be the
obvious failure, and the contract has no function that accepts or emits a
beneficiary identifier.

A subtler attack correlates the same beneficiary across programmes or infers
identities from batch metadata. Mitigations: only a Merkle root, a total, and a
count are published per batch; recipient references are salted before hashing, so
identical references produce different leaves in different programmes; and the
leaf and node encodings use distinct domain separators so an internal node can
never be replayed as a leaf.

Residual risk remains. A batch of count one reveals that exactly one person was
paid a known amount. Small counts and distinctive amounts leak information even
without identifiers. Timing and amount patterns can narrow a set. The protocol
reduces exposure; it does not make a determined, well-resourced correlation
attack impossible, and an implementer running very small batches should
understand that.

## Sanctions and regulatory exposure

Aid flows cross borders and touch sanctions regimes. Cygnus is non-custodial
software; the implementer remains the responsible party. The contract does not
obscure counterparties: the sponsor, implementer, approver, and every on-chain
movement are public. The protocol deliberately adds no feature whose purpose is
to hide who is transacting. Beneficiary privacy is about protecting recipients
from a published list, not about concealing the programme's operators.

## What the protocol does not defend against

- Off-chain reality. It cannot verify that goods arrived or that recipients
  were real or eligible.
- A compromised key. If a sponsor, implementer, or approver key is stolen, the
  thief has that party's powers. Key custody is out of scope.
- A malicious asset contract. A programme is only as sound as the token it uses;
  use a well-known asset.
- Global correlation by a powerful observer combining on-chain data with
  off-chain sources.
- Denial of service at the network layer or Testnet resets and rate limits.

## Reporting

Report a suspected vulnerability privately per `SECURITY.md`, never in a public
issue.
