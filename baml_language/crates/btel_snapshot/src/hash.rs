//! Snapshot hash formats 5 and 6: XXH3-128, seed zero, little-endian digest
//! bytes. Format 6 differs only in its blob prefix and its redacted values,
//! which hash as their tag, body length and raw body.
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
//! without its children. A type's digest is of its description as
//! `btel_core::cas::encode` writes it. Hash equality is not proof of delivery.
//!
//! A class or enum named by its recorded definition carries that
//! definition's blob ID in place, in a type head or a declaration, so its
//! name, not its runtime tag, and its definition are what the hash covers:
//! identical runtime classes hash identically. A definition blob's ID is the
//! hash of its root tag and its content bytes as written, which hold no
//! leaves to replace, then its child table and an object count of zero,
//! under a prefix of its own: the definition encoding's version, not the
//! blob format's. The ID does not cover the blob's header, so a later blob
//! format that writes definitions the same way gives each the same ID, and a
//! `def_id` compares across BAML releases. Changing how a definition is
//! encoded, the type descriptions in its fields included, requires a new
//! definition version.
//!
//! The hasher, the prefixes and every digest's input are `btel-core`'s
//! (`btel_core::cas::hash`); what is here is how this writer gathers and
//! caches them.
use std::{cell::RefCell, sync::OnceLock};

/// Content identity of one blob, as `btel-core` defines it.
pub use btel_core::cas::CasId;
pub(crate) use btel_core::cas::hash::{Absorb, Digest, Hasher};
use btel_core::cas::{
    self,
    encode::{DefinitionIdentity, encode_type, infallible},
    hash::start,
};
use num_bigint::BigInt;
use xxhash_rust::xxh3::Xxh3;

use super::{BexStr, OwnedType, describe::Segments, tags::HashDomain};

/// A destination for a blob's hash input (see `walk`): its pieces, and a
/// declaration's identity as `btel-core` encodes it.
pub(crate) trait HashSink: Absorb {
    fn declaration(&mut self, identity: DefinitionIdentity<'_, Segments<'_>>);
}
impl HashSink for Hasher {
    fn declaration(&mut self, identity: DefinitionIdentity<'_, Segments<'_>>) {
        infallible(identity.write(&mut Absorbing(self)));
    }
}

/// Hash input as an encoder's sink.
struct Absorbing<'h, H>(&'h mut H);
impl<H: Absorb> cas::encode::Sink for Absorbing<'_, H> {
    type Error = std::convert::Infallible;
    #[inline]
    fn put(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.absorb(bytes);
        Ok(())
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
        start(input, HashDomain::Blob, cas::VERSION);
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
    /// piece by piece; one identity may take the input past the bound before
    /// it streams.
    #[inline]
    fn declaration(&mut self, identity: DefinitionIdentity<'_, Segments<'_>>) {
        infallible(identity.write(&mut *self.input));
        if self.input.len() > btel_settings::snapshot::GATHERED_HASH_BYTES {
            self.spill(&[]);
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
    cas::hash::bigint(
        super::tags::bigint_sign(n.sign()),
        n.bits(),
        n.iter_u64_digits(),
    )
}
/// A type description's digest and the length of its encoding, both found by
/// encoding it once.
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
    static CONSTANT: [OnceLock<TypeLeaf>; 13] = [const { OnceLock::new() }; 13];
    let constant = match ty {
        T::Int => 0,
        T::Bigint => 1,
        T::Float => 2,
        T::String => 3,
        T::Bool => 4,
        T::Null => 5,
        T::Uint8Array => 6,
        T::Type => 8,
        T::Resource => 9,
        T::PromptAst => 10,
        T::Unknown => 11,
        T::Never => 12,
        T::Media(_)
        | T::Literal(..)
        | T::Class(..)
        | T::Interface(..)
        | T::Enum(_)
        | T::EnumVariant(..)
        | T::List(_)
        | T::Tuple(_)
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
        start(input, HashDomain::Type, cas::VERSION);
        let at = input.len();
        infallible(encode_type(crate::describe::Value(ty), input));
        TypeLeaf {
            digest: cas::hash::digest(input),
            encoded_len: u32::try_from(input.len() - at).expect("type description exceeds u32"),
        }
    })
}

#[cfg(test)]
mod tests {
    use btel_settings::snapshot::GATHERED_HASH_BYTES;

    use super::*;

    /// Past the bound a blob hasher streams, and still digests what a
    /// streaming hasher does over the same pieces: small pieces that cross
    /// the bound, one that nearly fills it, one larger than the bound,
    /// declaration identities (one larger than the bound), and a remainder
    /// after the last spill.
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
        let mut streamed = Hasher::versioned(HashDomain::Blob, cas::VERSION);
        let mut whole = Vec::new();
        start(&mut whole, HashDomain::Blob, cas::VERSION);
        for piece in &pieces {
            gathered.absorb(piece);
            streamed.absorb(piece);
            whole.extend_from_slice(piece);
            assert!(gathered.input.len() <= GATHERED_HASH_BYTES);
        }
        // An identity that takes the input past the bound streams too.
        let long = "n".repeat(GATHERED_HASH_BYTES);
        for name in [long.as_str(), "Person"] {
            let name = baml_type::DeclarationName::Anonymous(baml_type::Name::new(name));
            let identity = || DefinitionIdentity::Tagged(7, crate::describe::name(&name));
            HashSink::declaration(&mut gathered, identity());
            HashSink::declaration(&mut streamed, identity());
            infallible(identity().write(&mut whole));
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
        let mut streamed = Hasher::versioned(HashDomain::Blob, cas::VERSION);
        for n in 0..100_u64 {
            gathered.number(n);
            streamed.number(n);
        }
        assert!(gathered.stream.is_none());
        assert_eq!(gathered.finish(), streamed.finish());
    }

    /// A definition group's ID follows the definition encoding, so a blob
    /// format change leaves it alone; a value blob's follows the blob format.
    #[test]
    fn a_definition_id_does_not_depend_on_the_blob_version() {
        let digest = |domain, version| {
            let mut h = Hasher::versioned(domain, version);
            h.absorb(b"the same content");
            h.finish()
        };
        let definition = digest(HashDomain::Definitions, cas::VERSION);
        assert_eq!(definition, digest(HashDomain::Definitions, 4));
        assert_eq!(
            definition,
            digest(HashDomain::Definitions, cas::VERSION + 1)
        );
        assert_ne!(definition, digest(HashDomain::Blob, cas::VERSION));
        assert_ne!(
            digest(HashDomain::Blob, 4),
            digest(HashDomain::Blob, cas::VERSION)
        );
    }
}
