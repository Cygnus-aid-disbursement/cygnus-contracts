// Self-contained Merkle helper for the demo script. It reproduces the leaf and
// node encoding from docs/protocol.md using only Node's crypto, so the contracts
// repo can run a full demo without depending on the SDK. The SDK carries the
// production implementation and a test that cross-checks it against the contract.
//
// Usage: node scripts/merkle_root.mjs <count> <amountPerLeaf> [sampleIndex]
// Prints JSON: { root, total, count, leaf, proof } for the sample leaf.

import { createHash } from "node:crypto";

const LEAF_DOMAIN = 0x00;
const NODE_DOMAIN = 0x01;

function sha256(buf) {
  return createHash("sha256").update(buf).digest();
}

// Stand-in for a per-programme salted hash of a beneficiary reference. A raw
// identifier is never used; this hashes an opaque synthetic reference.
function recipientRef(i) {
  return sha256(Buffer.from(`cygnus-demo-salt:beneficiary:${i}`, "utf8"));
}

function amountBe(amount) {
  const b = Buffer.alloc(16);
  let v = BigInt(amount);
  for (let i = 15; i >= 0; i--) {
    b[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return b;
}

function hashLeaf(ref, amount) {
  return sha256(Buffer.concat([Buffer.from([LEAF_DOMAIN]), ref, amountBe(amount)]));
}

function hashNode(a, b) {
  const [lo, hi] = Buffer.compare(a, b) <= 0 ? [a, b] : [b, a];
  return sha256(Buffer.concat([Buffer.from([NODE_DOMAIN]), lo, hi]));
}

function build(leaves) {
  const levels = [leaves];
  let level = leaves;
  while (level.length > 1) {
    const next = [];
    for (let i = 0; i < level.length; i += 2) {
      if (i + 1 < level.length) next.push(hashNode(level[i], level[i + 1]));
      else next.push(level[i]); // odd tail carried up unchanged
    }
    levels.push(next);
    level = next;
  }
  return levels;
}

function proofFor(levels, index) {
  const proof = [];
  let idx = index;
  for (let l = 0; l < levels.length - 1; l++) {
    const level = levels[l];
    const isRight = idx % 2 === 1;
    const sib = isRight ? idx - 1 : idx + 1;
    if (sib < level.length) proof.push(level[sib].toString("hex"));
    idx = Math.floor(idx / 2);
  }
  return proof;
}

const count = parseInt(process.argv[2] ?? "50", 10);
const per = BigInt(process.argv[3] ?? "12");
const sample = parseInt(process.argv[4] ?? "0", 10);

const leaves = [];
for (let i = 0; i < count; i++) leaves.push(hashLeaf(recipientRef(i), per));
const levels = build(leaves);
const root = levels[levels.length - 1][0];

process.stdout.write(
  JSON.stringify({
    root: root.toString("hex"),
    total: (per * BigInt(count)).toString(),
    count,
    leaf: leaves[sample].toString("hex"),
    proof: proofFor(levels, sample),
  })
);
