# Cygnus programme protocol

This is the normative specification for the Cygnus programme contract. The Rust
code in `contracts/programme` follows this document; the SDK's Merkle and hashing
code must match section 5 byte for byte. Where the two disagree, this document
and the contract's own tests decide.

## 1. Actors

- **Sponsor** — funds the programme and recovers any remainder after the end
  date. Authorises `create_programme`, `fund` and `refund_remainder`.
- **Implementer** — the NGO or field office. Authorises `claim` and `disburse`.
- **Approver** — releases a claimed milestone's budget. At the 65% line only the
  `Single` approver is built. Authorises `approve` and `reject`.

There is no platform account. Funds sit in the contract under the programme's own
rules and move only on the calls below.

## 2. Programme lifecycle

A programme is `Active` from creation until the sponsor refunds the remainder,
after which it is `Closed` and rejects every state-changing call.

```
create_programme -> Active
Active --refund_remainder (after end date)--> Closed
```

Each milestone carries its own status, independent of the others:

```
Pending --claim--> Claimed --approve--> Approved
                   Claimed --reject--> Rejected --claim--> Claimed
```

`Approved` is terminal. `Rejected` returns to `Claimed` on a fresh claim, so an
implementer can fix evidence and resubmit.

## 3. Transitions

Every row lists the caller who must authorise the call and the precondition the
contract enforces. A violated precondition returns the error in section 6.

| Call | Actor | Precondition |
|---|---|---|
| `create_programme` | sponsor | `total > 0`; every milestone `amount > 0` |
| `fund` | sponsor | programme `Active`; `amount > 0` |
| `claim` | implementer | programme `Active`; milestone `Pending` or `Rejected` |
| `approve` | approver | programme `Active`; milestone `Claimed`; `released + amount <= funded` |
| `reject` | approver | programme `Active`; milestone `Claimed` |
| `disburse` | implementer | programme `Active`; `total > 0`; `count > 0`; `disbursed + total <= released` |
| `refund_remainder` | sponsor | programme `Active`; `now > end_by`; `funded - disbursed > 0` |

The programme's `end_by` is the latest `due_by` across its milestones. It is not
a separate parameter.

## 4. Milestones and balances

A `Milestone` is `{ description_hash, amount, due_by }`. `description_hash` is a
hash; the description itself stays off chain.

Three running balances govern the money:

- `funded` — total received through `fund`.
- `released` — sum of approved milestone amounts. `approve` moves a milestone's
  budget from merely funded to spendable.
- `disbursed` — sum of batch totals paid out through `disburse`.

The invariant `disbursed <= released <= funded` holds at all times. A claim after
`due_by` sets `claimed_late = true` on the milestone; it is recorded, never
blocked, because a missed date does not mean the work was not done.

`disburse` moves `total` of the asset from the contract to the implementer, who
performs the last-mile payout off chain. `refund_remainder` returns
`funded - disbursed` to the sponsor, which includes any released-but-unspent
budget.

## 5. Disbursement batches and Merkle construction

A disbursement publishes a Merkle root, a total and a count. The list of
individual payouts is never published. A beneficiary holds a receipt that lets
them prove their own inclusion; nobody can enumerate the set.

### 5.1 Leaf encoding

```
leaf = sha256( 0x00 || recipient_ref[32] || amount_be[16] )
```

- `0x00` is the leaf domain separator.
- `recipient_ref` is a 32-byte salted hash of the beneficiary reference. A raw
  identifier must never be hashed here. Salting per programme stops the same
  beneficiary being correlated across programmes.
- `amount_be` is the payout amount as a 16-byte big-endian two's-complement
  `i128`.

### 5.2 Internal nodes

```
node = sha256( 0x01 || min(a, b) || max(a, b) )
```

- `0x01` is the node domain separator, distinct from the leaf separator so an
  internal node can never be presented as a leaf.
- The two children are sorted by raw big-endian byte value before hashing. An
  inclusion proof therefore carries siblings only, with no left/right flags.

### 5.3 Tree shape and duplicate handling

Nodes are paired left to right: `(0,1), (2,3), ...`. When a level has an odd
number of nodes, the last node is carried up to the next level unchanged. It is
**not** duplicated, because duplicating the tail leaf would let a prover forge a
second inclusion for it. A single-leaf tree has that leaf as its root.

### 5.4 Verification

`verify_inclusion(programme_id, batch_index, leaf, proof)` loads the stored root
for that batch, folds the proof (`acc = node(acc, sibling)` for each sibling in
order) and returns whether the fold equals the root. It returns `false` for an
unknown programme or batch rather than trapping.

`hash_leaf(recipient_ref, amount)` is exposed on the contract so the SDK and any
external auditor can confirm they reproduce the leaf encoding exactly.

## 6. Error codes

| Code | Name | Cause |
|---|---|---|
| 1 | `ProgrammeNotFound` | no programme with that id |
| 2 | `AlreadyClosed` | programme is `Closed` |
| 3 | `MilestoneIndexOutOfRange` | milestone index past the end |
| 4 | `MilestoneNotClaimable` | claim on a `Claimed` or `Approved` milestone |
| 5 | `MilestoneNotClaimed` | approve/reject on a `Pending` milestone |
| 6 | `MilestoneAlreadyResolved` | approve/reject on an `Approved` or `Rejected` milestone |
| 7 | `InsufficientFunding` | approve would push `released` above `funded` |
| 8 | `InsufficientReleased` | disburse would push `disbursed` above `released` |
| 9 | `NotEnded` | refund before `end_by` |
| 10 | `InvalidAmount` | non-positive total or milestone amount |
| 11 | `InvalidBatch` | disburse with non-positive total or zero count |
| 12 | `ApproverKindUnimplemented` | approve/reject under a `Panel` or `Oracle` approver |
| 13 | `NothingToRefund` | refund when `funded - disbursed` is zero |
| 14 | `BatchIndexOutOfRange` | reserved for callers that index batches directly |

## 7. Events

Events are declared with the `#[contractevent]` macro, so each event's name is a
topic and appears in the contract interface specification. `programme_id` is a
`#[topic]` on every event, which lets an indexer subscribe per programme. The
remaining fields are event data.

| Event | Topics | Data |
|---|---|---|
| `ProgrammeCreated` | `programme_id` | `sponsor, implementer, total` |
| `ProgrammeFunded` | `programme_id` | `amount` |
| `MilestoneClaimed` | `programme_id` | `milestone_index, evidence_hash, claimed_late` |
| `MilestoneApproved` | `programme_id` | `milestone_index` |
| `MilestoneRejected` | `programme_id` | `milestone_index, reason_hash` |
| `BatchDisbursed` | `programme_id` | `batch_index, root, total, count` |
| `ProgrammeRefunded` | `programme_id` | `amount` |

An indexer can reconstruct a programme's full public history from these events
plus `get_programme` and `get_batch`. No event carries a beneficiary identifier,
because none exists on chain to carry.
