//! Snapshot hash format 3: XXH3-128, seed zero, little-endian digest bytes.
//!
//! Leaf digests depend only on their content and are computed once, at
//! capture: a string's is its content hash, which the string caches; bigints,
//! types and copied bytes are hashed as they are captured. A blob's ID is one
//! hash over its content in encoding order (see `walk`): the root, then each
//! object definition in blob-local order, then the child table and the object
//! count. Strings, including enum variant and descriptive names, bigints and
//! types are replaced by their digests; map keys and declaration names are
//! hashed in place; a reference is its blob-local number, or the slot of the
//! child blob it names. A blob's identity therefore depends on its reachable
//! content and never on capture discovery order, heap addresses, arena
//! capacities or string rope shape. Map and argument order matter. Cycles
//! need no recursive Merkle dependencies. Every hashed input is either a byte
//! of the blob or a digest recomputed from its bytes, so a blob verifies
//! without its children. Borsh's attribute-free type encoding is part of
//! version 3: changes to it require a hash-format version change. Hash
//! equality is not proof of delivery.
use std::io::{self, Write};

use borsh::BorshSerialize;
use xxhash_rust::xxh3::Xxh3;

use super::{BexStr, BigInt, OwnedType, TypeIdentity, tags::HashDomain};

/// Content identity of one blob. A zero digest is a valid hash, never absence.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CasId([u8; 16]);
impl CasId {
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Digest(pub(crate) [u8; 16]);

/// A destination for hash input.
pub(crate) trait Absorb {
    fn absorb(&mut self, bytes: &[u8]);
    fn byte(&mut self, n: u8) {
        self.absorb(&[n]);
    }
    fn number(&mut self, n: u64) {
        self.absorb(&n.to_le_bytes());
    }
    fn size(&mut self, n: usize) {
        self.number(u64::try_from(n).expect("snapshot length"));
    }
    fn digest(&mut self, h: Digest) {
        self.absorb(&h.0);
    }
}

pub(crate) struct Hasher(Xxh3);
impl Hasher {
    pub(crate) fn new(domain: HashDomain) -> Self {
        let mut h = Self(Xxh3::new());
        h.absorb(b"baml.snapshot.xxh3-128.v3\0");
        h.byte(domain as u8);
        h
    }
    /// Absorb a value's Borsh encoding and return its length.
    pub(crate) fn borsh(&mut self, value: &impl BorshSerialize) -> usize {
        let mut counted = Counted {
            hasher: self,
            written: 0,
        };
        value
            .serialize(&mut counted)
            .expect("infallible hash writer");
        counted.written
    }
    pub(crate) fn finish(&self) -> Digest {
        Digest(self.0.digest128().to_le_bytes())
    }
}
impl Absorb for Hasher {
    fn absorb(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}
struct Counted<'a> {
    hasher: &'a mut Hasher,
    written: usize,
}
impl Write for Counted<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hasher.0.update(bytes);
        self.written += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// A writer that only measures.
pub(crate) struct Counter(pub(crate) usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl BorshSerialize for TypeIdentity {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        use super::tags::TypeIdentityTag;
        match self {
            Self::Resolved(head) => {
                (TypeIdentityTag::Resolved as u8).serialize(w)?;
                head.tag().serialize(w)?;
                head.name().serialize(w)
            }
            Self::Unresolved(tag) => {
                (TypeIdentityTag::Unresolved as u8).serialize(w)?;
                tag.serialize(w)
            }
        }
    }
}
/// A string's digest is its content hash; the tag before it in the hash
/// input keeps it apart from every other leaf.
pub(crate) fn string(s: &BexStr) -> Digest {
    Digest(s.content_hash().to_le_bytes())
}
pub(crate) fn bigint(n: &BigInt) -> Digest {
    let mut h = Hasher::new(HashDomain::Bigint);
    h.byte(super::tags::bigint_sign(n.sign()));
    h.number(n.bits());
    for digit in n.iter_u64_digits() {
        h.number(digit);
    }
    h.finish()
}
/// A type description's digest and the length of its encoding, both found by
/// serializing it once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TypeLeaf {
    pub(crate) digest: Digest,
    pub(crate) encoded_len: u32,
}
pub(crate) fn ty(ty: &OwnedType) -> TypeLeaf {
    let mut h = Hasher::new(HashDomain::Type);
    let encoded_len = u32::try_from(h.borsh(ty)).expect("type description exceeds u32");
    TypeLeaf {
        digest: h.finish(),
        encoded_len,
    }
}
