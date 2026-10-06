//! Snapshot hash format 5: XXH3-128, seed zero, little-endian digest bytes.
//!
//! Leaf digests depend only on their content and are computed once, at
//! capture: a string's is its content hash, which the string caches; bigints,
//! types and copied bytes are hashed as they are captured. A blob's ID is one
//! hash over its content in encoding order (see `walk`): the root, then each
//! object definition in blob-local order, then the child table and the object
//! count. Strings, including enum variant and descriptive names, bigints and
//! types are replaced by their digests; field and declaration names are
//! hashed in place; a reference is its blob-local number, or the slot of the
//! child blob it names. A blob's identity therefore depends on its reachable
//! content and never on capture discovery order, heap addresses, arena
//! capacities or string rope shape. Map and argument order matter. Cycles
//! need no recursive Merkle dependencies. Every hashed input is either a byte
//! of the blob or a digest recomputed from its bytes, so a blob verifies
//! without its children. Borsh's attribute-free type encoding is part of
//! version 5: changes to it require a hash-format version change. Hash
//! equality is not proof of delivery.
//!
//! A class or enum named by its recorded definition carries that
//! definition's blob ID in place, in a type head or a declaration, so its
//! name, not its runtime tag, and its definition are what the hash covers:
//! identical runtime classes hash identically. A definition blob's ID is the
//! hash of its root tag and its content bytes as written, which hold no
//! leaves to replace, then its child table and an object count of zero.
use std::{
    cell::RefCell,
    io::{self, Write},
    sync::OnceLock,
};

use borsh::BorshSerialize;
use num_bigint::BigInt;
use xxhash_rust::xxh3::Xxh3;

use super::{BexStr, OwnedType, TypeIdentity, tags::HashDomain};

/// What every leaf's hash input starts with, before its domain.
const LEAF_PREFIX: &[u8] = b"baml.snapshot.xxh3-128.v3\0";

/// Content identity of one blob. A zero digest is a valid hash, never absence.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        Self::versioned(domain, crate::BLOB_VERSION)
    }
    pub(crate) fn versioned(domain: HashDomain, version: u32) -> Self {
        let mut h = Self(Xxh3::new());
        // Leaf encodings are unchanged; only blob identities change in v4
        // and v5.
        if matches!(domain, HashDomain::Blob) && version >= 5 {
            h.absorb(b"baml.snapshot.xxh3-128.v5\0");
        } else if matches!(domain, HashDomain::Blob) && version == 4 {
            h.absorb(b"baml.snapshot.xxh3-128.v4\0");
        } else {
            h.absorb(LEAF_PREFIX);
        }
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
            Self::Defined(definition) => {
                (TypeIdentityTag::Defined as u8).serialize(w)?;
                definition.serialize(w)
            }
        }
    }
}
/// A string's digest is its content hash; the tag before it in the hash
/// input keeps it apart from every other leaf.
pub(crate) fn string(s: &BexStr) -> Digest {
    Digest(s.content_hash().to_le_bytes())
}
/// The digest of a `uint8array` with no content.
pub(crate) fn no_bytes() -> Digest {
    Hasher::new(HashDomain::Uint8Array).finish()
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
///
/// The description is encoded whole and hashed in one call, which is the
/// same digest as hashing it piece by piece and much faster for the many
/// small pieces a type has. A type without parts has its leaf made once.
pub(crate) fn ty(ty: &OwnedType) -> TypeLeaf {
    use baml_type::RealizedTy as T;
    let constant = match ty {
        T::Int => 0,
        T::Bigint => 1,
        T::Float => 2,
        T::String => 3,
        T::Bool => 4,
        T::Null => 5,
        T::Uint8Array => 6,
        T::RustType => 7,
        T::Type => 8,
        T::Resource => 9,
        T::PromptAst => 10,
        T::Void => 11,
        T::Unknown => 12,
        T::Never => 13,
        T::Media(_)
        | T::Literal(..)
        | T::Class(..)
        | T::Interface(..)
        | T::Enum(_)
        | T::EnumVariant(..)
        | T::List(_)
        | T::Map { .. }
        | T::Union(_)
        | T::Function { .. }
        | T::Future(..)
        | T::TypeAlias(_) => return encoded(ty),
    };
    static CONSTANT: [OnceLock<TypeLeaf>; 14] = [const { OnceLock::new() }; 14];
    *CONSTANT[constant].get_or_init(|| encoded(ty))
}

thread_local! {
    /// A type description being hashed: the hash input, prefix included.
    static TYPE_INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn encoded(ty: &OwnedType) -> TypeLeaf {
    TYPE_INPUT.with_borrow_mut(|input| {
        input.clear();
        input.extend_from_slice(LEAF_PREFIX);
        input.push(HashDomain::Type as u8);
        let start = input.len();
        ty.serialize(input).expect("writing to memory");
        TypeLeaf {
            digest: Digest(xxhash_rust::xxh3::xxh3_128(input).to_le_bytes()),
            encoded_len: u32::try_from(input.len() - start).expect("type description exceeds u32"),
        }
    })
}
