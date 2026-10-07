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

/// What a hash's input starts with, before its domain byte. Leaf encodings
/// are unchanged since v3; only blob identities change in v4 and v5.
fn prefix(domain: HashDomain, version: u32) -> &'static [u8] {
    match domain {
        HashDomain::Blob if version >= 5 => b"baml.snapshot.xxh3-128.v5\0",
        HashDomain::Blob if version == 4 => b"baml.snapshot.xxh3-128.v4\0",
        _ => LEAF_PREFIX,
    }
}

pub(crate) struct Hasher(Xxh3);
impl Hasher {
    pub(crate) fn new(domain: HashDomain) -> Self {
        Self::versioned(domain, crate::BLOB_VERSION)
    }
    pub(crate) fn versioned(domain: HashDomain, version: u32) -> Self {
        let mut h = Self(Xxh3::new());
        h.absorb(prefix(domain, version));
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

/// A destination for a blob's hash input (see `walk`): its pieces, and
/// Borsh encodings written as they are.
pub(crate) trait HashSink: Absorb {
    fn borsh(&mut self, value: &impl BorshSerialize);
}
impl HashSink for Hasher {
    fn borsh(&mut self, value: &impl BorshSerialize) {
        Hasher::borsh(self, value);
    }
}

/// A blob ID's hash input, gathered and hashed in one call. Its digest is
/// the streaming [`Hasher`]'s over the same pieces (an XXH3 stream digests as
/// one call over everything it was given), without an update per piece,
/// which is most of what a small blob costs to hash. Past
/// [`GATHERED_HASH_BYTES`](btel_settings::snapshot::GATHERED_HASH_BYTES) of
/// input it streams, so what it gathers stays bounded.
pub(crate) struct BlobHasher<'a> {
    /// The input not yet streamed: at most the bound between pieces.
    input: &'a mut Vec<u8>,
    /// The input past the bound, once there is any.
    stream: Option<Box<Xxh3>>,
}
impl<'a> BlobHasher<'a> {
    /// Hash a blob of the current format, gathering into `input`.
    pub(crate) fn new(input: &'a mut Vec<u8>) -> Self {
        input.clear();
        input.extend_from_slice(prefix(HashDomain::Blob, crate::BLOB_VERSION));
        input.push(HashDomain::Blob as u8);
        Self {
            input,
            stream: None,
        }
    }
    pub(crate) fn finish(self) -> Digest {
        let digest = match self.stream {
            None => xxhash_rust::xxh3::xxh3_128(self.input),
            Some(mut stream) => {
                stream.update(self.input);
                stream.digest128()
            }
        };
        Digest(digest.to_le_bytes())
    }
    /// Stream what was gathered, then `bytes`, which would pass the bound.
    #[cold]
    #[inline(never)]
    fn spill(&mut self, bytes: &[u8]) {
        let stream = self.stream.get_or_insert_with(|| Box::new(Xxh3::new()));
        stream.update(self.input);
        self.input.clear();
        stream.update(bytes);
    }
}
impl Absorb for BlobHasher<'_> {
    #[inline]
    fn absorb(&mut self, bytes: &[u8]) {
        // Between pieces the input is within the bound: no underflow.
        if bytes.len() > btel_settings::snapshot::GATHERED_HASH_BYTES - self.input.len() {
            self.spill(bytes);
        } else {
            self.input.extend_from_slice(bytes);
        }
    }
}
impl HashSink for BlobHasher<'_> {
    /// Written straight into the gathered input, which is much faster than
    /// piece by piece through [`Write`]; one such encoding (a declaration's
    /// identity) may take the input past the bound before it streams.
    #[inline]
    fn borsh(&mut self, value: &impl BorshSerialize) {
        value
            .serialize(&mut *self.input)
            .expect("writing to memory");
        if self.input.len() > btel_settings::snapshot::GATHERED_HASH_BYTES {
            self.spill(&[]);
        }
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
    static CONSTANT: [OnceLock<TypeLeaf>; 14] = [const { OnceLock::new() }; 14];
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

#[cfg(test)]
mod tests {
    use btel_settings::snapshot::GATHERED_HASH_BYTES;

    use super::*;

    /// Past the bound a blob hasher streams, and still digests what a
    /// streaming hasher does over the same pieces: small pieces that cross
    /// the bound, one that nearly fills it, one larger than the bound, Borsh
    /// encodings (one larger than the bound), and a remainder after the last
    /// spill.
    #[test]
    fn a_blob_hasher_past_its_bound_digests_as_one_stream() {
        let piece = |n: usize, seed: u8| -> Vec<u8> {
            (0..n)
                .map(|at| at.to_le_bytes()[0].wrapping_mul(31) ^ seed)
                .collect()
        };
        let pieces = [
            vec![piece(5, 1); GATHERED_HASH_BYTES / 5 + 3],
            vec![piece(GATHERED_HASH_BYTES - 11, 2)],
            vec![piece(3 * GATHERED_HASH_BYTES + 7, 3)],
            vec![piece(1, 4); 1000],
        ]
        .concat();
        let mut input = Vec::new();
        let mut gathered = BlobHasher::new(&mut input);
        let mut streamed = Hasher::new(HashDomain::Blob);
        let mut whole = prefix(HashDomain::Blob, crate::BLOB_VERSION).to_vec();
        whole.push(HashDomain::Blob as u8);
        for piece in &pieces {
            gathered.absorb(piece);
            streamed.absorb(piece);
            whole.extend_from_slice(piece);
            assert!(gathered.input.len() <= GATHERED_HASH_BYTES);
        }
        // A Borsh encoding that takes the input past the bound streams too.
        let long = "n".repeat(GATHERED_HASH_BYTES);
        for name in [long.as_str(), "Person"] {
            let name = baml_type::DeclarationName::Anonymous(baml_type::Name::new(name));
            HashSink::borsh(&mut gathered, &name);
            HashSink::borsh(&mut streamed, &name);
            name.serialize(&mut whole).unwrap();
            assert!(gathered.input.len() <= GATHERED_HASH_BYTES);
        }
        assert!(gathered.stream.is_some(), "the input passed the bound");
        let streamed = streamed.finish();
        assert_eq!(gathered.finish(), streamed);
        assert_eq!(
            streamed,
            Digest(xxhash_rust::xxh3::xxh3_128(&whole).to_le_bytes())
        );
    }

    /// Under the bound nothing streams: one call hashes the whole input.
    #[test]
    fn a_small_blob_hasher_digests_in_one_call() {
        let mut input = Vec::new();
        let mut gathered = BlobHasher::new(&mut input);
        let mut streamed = Hasher::new(HashDomain::Blob);
        for n in 0..100_u64 {
            gathered.number(n);
            streamed.number(n);
        }
        assert!(gathered.stream.is_none());
        assert_eq!(gathered.finish(), streamed.finish());
    }
}
