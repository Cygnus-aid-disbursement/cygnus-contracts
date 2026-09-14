use soroban_sdk::{Bytes, BytesN, Env, Vec};

// Domain separators keep a leaf hash from ever colliding with an internal-node
// hash, which is what stops a second-preimage attack that passes an internal
// node off as a leaf.
const LEAF_DOMAIN: u8 = 0x00;
const NODE_DOMAIN: u8 = 0x01;

/// Canonical leaf hash for one payout.
///
/// leaf = sha256( 0x00 || recipient_ref[32] || amount_be[16] )
///
/// `recipient_ref` is already a salted hash of the beneficiary reference; a raw
/// identifier must never reach this function. `amount` is encoded as a 16-byte
/// big-endian two's-complement i128. The SDK reproduces these exact bytes, so
/// any change here is a breaking change that must land in both repositories at
/// once with the cross-check test updated.
pub fn hash_leaf(env: &Env, recipient_ref: &BytesN<32>, amount: i128) -> BytesN<32> {
    let mut buf = Bytes::new(env);
    buf.push_back(LEAF_DOMAIN);
    buf.append(&Bytes::from_array(env, &recipient_ref.to_array()));
    buf.append(&Bytes::from_array(env, &amount.to_be_bytes()));
    env.crypto().sha256(&buf).to_bytes()
}

/// Internal-node hash of two children.
///
/// node = sha256( 0x01 || min(a,b) || max(a,b) )
///
/// Children are sorted before hashing so an inclusion proof carries siblings
/// only, never left/right flags. Ordering is by raw big-endian byte value.
pub fn hash_node(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let (lo, hi) = if a.to_array() <= b.to_array() {
        (a, b)
    } else {
        (b, a)
    };
    let mut buf = Bytes::new(env);
    buf.push_back(NODE_DOMAIN);
    buf.append(&Bytes::from_array(env, &lo.to_array()));
    buf.append(&Bytes::from_array(env, &hi.to_array()));
    env.crypto().sha256(&buf).to_bytes()
}

/// Root of a tree built from already-hashed leaves.
///
/// An odd node at any level is carried up unchanged rather than duplicated.
/// Duplicating the last leaf would let a prover forge a second inclusion for
/// it, so we do not. The contract verifies proofs but never builds trees on
/// chain; construction lives in the SDK, so this is compiled only for tests
/// that cross-check the two against each other.
#[cfg(test)]
pub fn compute_root(env: &Env, leaves: &Vec<BytesN<32>>) -> BytesN<32> {
    let mut level = leaves.clone();
    while level.len() > 1 {
        let mut next = Vec::new(env);
        let mut i = 0u32;
        while i < level.len() {
            if i + 1 < level.len() {
                let a = level.get_unchecked(i);
                let b = level.get_unchecked(i + 1);
                next.push_back(hash_node(env, &a, &b));
                i += 2;
            } else {
                next.push_back(level.get_unchecked(i));
                i += 1;
            }
        }
        level = next;
    }
    level.get_unchecked(0)
}

/// Fold an inclusion proof and test it against `root`.
///
/// `leaf` is the leaf hash from `hash_leaf`. `proof` is the ordered list of
/// sibling hashes from the leaf up to the root.
pub fn verify(env: &Env, leaf: &BytesN<32>, proof: &Vec<BytesN<32>>, root: &BytesN<32>) -> bool {
    let mut acc = leaf.clone();
    for sibling in proof.iter() {
        acc = hash_node(env, &acc, &sibling);
    }
    acc == *root
}
