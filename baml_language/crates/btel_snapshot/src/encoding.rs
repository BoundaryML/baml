//! CAS blob format 4. Explicit little-endian scalar tags, not Rust layouts.
//!
//! Header: `BTELCAS\0`, u32 version, 16 digest bytes, u32 child count and each
//! child's 16-byte ID, u32 object count, root, then object definitions in
//! blob-local order. Root is tag 0/value or tag 1/u64 parameter count/value
//! sequence. Sequences and UTF-8 strings use u32 lengths. Values inline
//! immutable leaf contents; graph objects use u32 references, by local
//! number. Numbers follow each object's first reference in this byte order,
//! so a blob has exactly one encoding, and the reader rejects any other
//! numbering. This also makes leaf-arena sharing irrelevant to the bytes.
//!
//! A value stored in another blob is written as tag 11 and the u32 slot of
//! that blob in the child table (the value is the child's root value), or as
//! tag 12, slot and u32 local number for an object of the child other than
//! its root. Slots follow first use in this byte order, like object numbers.
//! A blob's own root value, names, types and declarations are
//! always written in place.
//! Map keys and values both use the generic value encoding. Instance field
//! names remain inline strings. Readers also accept format 3 string-only keys.
//!
//! A media object (tag 9) writes its kind, whether a MIME type follows and
//! that type, then a source tag: 0 with a URL, 1 with a path, each followed by
//! whether content follows, or 2, which always has content. Content is its
//! u64 text length, then the base64 text as a value, so large content can be a
//! child blob. The MIME type, URL and path are strings written in place.
//!
//! Value and object tags mirror hash format 4. Type/declaration metadata uses
//! its Borsh representation. Floats use raw bits (including NaNs). Bigints use
//! sign (0 negative, 1 zero, 2 positive), u64 bit length, then ceil(bits/64)
//! little-endian u64 magnitude limbs. Any encoding change needs a version bump.
use std::io::{self, Write};

use baml_type::{DeclarationName, typetag::TypeTag};
use borsh::BorshSerialize;
pub use btel_settings::snapshot::{BLOB_MAGIC, BLOB_VERSION};
use num_bigint::BigInt;

#[cfg(debug_assertions)]
use crate::walk::Length;
use crate::{
    BexStr, Blob, CasId,
    graph::{BigintId, Graph, ObjectId, OwnedType, StringId},
    hash::{Digest, TypeLeaf},
    shape::{BlobIndex, HEADER_BYTES, Home, Shape, UNNUMBERED},
    tags::{self, RootTag, ValueTag},
    walk::{self, Reference, Resolver, Visitor},
};

fn size(w: &mut impl Write, n: usize) -> io::Result<()> {
    u32::try_from(n)
        .map_err(|_| io::Error::other("snapshot sequence exceeds u32"))?
        .serialize(w)
}
fn original_len(w: &mut impl Write, n: usize) -> io::Result<()> {
    u64::try_from(n)
        .map_err(|_| io::Error::other("snapshot length exceeds u64"))?
        .serialize(w)
}
fn string(w: &mut impl Write, value: &BexStr) -> io::Result<()> {
    size(w, value.len())?;
    w.write_all(value.as_bytes())
}

/// What a blob that is one string writes before the string: the header, no
/// children, no objects, and a root value that is a string of some length.
pub(crate) const STRING_BLOB_HEADER_BYTES: usize = 8 + 4 + 16 + 4 + 4 + 1 + 1 + 4;
const _: () = assert!(STRING_BLOB_HEADER_BYTES as u64 == HEADER_BYTES + 4 + 4 + 1 + 1 + 4);

/// The bytes of the blob `id` that is one string of `len` bytes, up to the
/// string itself.
pub(crate) fn string_blob_header(id: CasId, len: u32) -> [u8; STRING_BLOB_HEADER_BYTES] {
    let mut header = [0; STRING_BLOB_HEADER_BYTES];
    let mut rest: &mut [u8] = &mut header;
    let mut put = |bytes: &[u8]| {
        let (to, after) = std::mem::take(&mut rest).split_at_mut(bytes.len());
        to.copy_from_slice(bytes);
        rest = after;
    };
    put(&BLOB_MAGIC);
    put(&BLOB_VERSION.to_le_bytes());
    put(id.as_bytes());
    // No children and no objects.
    put(&0_u32.to_le_bytes());
    put(&0_u32.to_le_bytes());
    put(&[RootTag::Value as u8, ValueTag::String as u8]);
    put(&len.to_le_bytes());
    debug_assert!(rest.is_empty(), "the header is exactly this long");
    header
}

/// Reusable maps from a capture's objects and blobs to their numbers in the
/// blob being written. Any snapshot's blobs may share one; it holds nothing
/// between writes.
#[derive(Default)]
pub struct BlobScratch {
    /// Object to local number, [`UNNUMBERED`] for every other object and
    /// between writes.
    local: Vec<u32>,
    /// Blob to its slot in the child table, likewise.
    slots: Vec<u32>,
}

impl Blob<'_> {
    /// Stream this blob's canonical bytes without cloning owners or
    /// allocating an encoded payload. The writer chooses its own buffering.
    pub fn write(&self, scratch: &mut BlobScratch, w: &mut impl Write) -> io::Result<()> {
        let s = &self.snapshot.0.graph;
        let shape = &self.snapshot.0.shape;
        let entry = self.entry();
        let members = &shape.members[entry.members.indexes()];
        let children = &shape.children[entry.children.indexes()];
        w.write_all(&BLOB_MAGIC)?;
        BLOB_VERSION.serialize(w)?;
        w.write_all(entry.id.as_bytes())?;
        size(w, children.len())?;
        for child in children {
            w.write_all(shape.blobs[child.0 as usize].id.as_bytes())?;
        }
        size(w, members.len())?;
        let mut stored = Stored::new(scratch, s, shape, members, children);
        let mut writer = Writer(w);
        walk::root(&mut writer, &mut stored, s, entry.root)?;
        for id in members {
            walk::object(&mut writer, &mut stored, s, &s.objects[id.0 as usize])?;
        }
        #[cfg(debug_assertions)]
        {
            // Shaping measured this blob; a second walk checks that writing
            // agrees, without costing release builds anything.
            let mut length = Length::default();
            walk::infallible(walk::root(&mut length, &mut stored, s, entry.root));
            for id in members {
                let object = &s.objects[id.0 as usize];
                walk::infallible(walk::object(&mut length, &mut stored, s, object));
            }
            assert_eq!(
                HEADER_BYTES + 4 + 16 * children.len() as u64 + 4 + length.0,
                entry.encoded_len,
                "a blob is as long as shaping measured"
            );
        }
        Ok(())
    }
}

/// The numbers shaping gave this blob's members and children. Leaves the
/// scratch clean when dropped, whether or not the write succeeded.
struct Stored<'a> {
    shape: &'a Shape,
    scratch: &'a mut BlobScratch,
    members: &'a [ObjectId],
    children: &'a [BlobIndex],
}
impl<'a> Stored<'a> {
    fn new(
        scratch: &'a mut BlobScratch,
        graph: &Graph,
        shape: &'a Shape,
        members: &'a [ObjectId],
        children: &'a [BlobIndex],
    ) -> Self {
        if scratch.local.len() < graph.objects.len() {
            scratch.local.resize(graph.objects.len(), UNNUMBERED);
        }
        if scratch.slots.len() < shape.blobs.len() {
            scratch.slots.resize(shape.blobs.len(), UNNUMBERED);
        }
        for (number, id) in members.iter().enumerate() {
            scratch.local[id.0 as usize] = u32::try_from(number).expect("bounded objects");
        }
        for (slot, child) in children.iter().enumerate() {
            scratch.slots[child.0 as usize] = u32::try_from(slot).expect("bounded blob count");
        }
        Self {
            shape,
            scratch,
            members,
            children,
        }
    }
    fn slot(&self, blob: BlobIndex) -> u32 {
        let slot = self.scratch.slots[blob.0 as usize];
        debug_assert_ne!(slot, UNNUMBERED, "a blob names only its children");
        slot
    }
}
impl Drop for Stored<'_> {
    fn drop(&mut self) {
        for id in self.members {
            self.scratch.local[id.0 as usize] = UNNUMBERED;
        }
        for child in self.children {
            self.scratch.slots[child.0 as usize] = UNNUMBERED;
        }
    }
}
impl Resolver for Stored<'_> {
    fn member(&mut self, id: ObjectId) -> u32 {
        let number = self.scratch.local[id.0 as usize];
        debug_assert_ne!(
            number, UNNUMBERED,
            "a blob holds what it must write in place"
        );
        number
    }
    fn object(&mut self, id: ObjectId) -> Reference {
        let number = self.scratch.local[id.0 as usize];
        if number != UNNUMBERED {
            return Reference::Local(number);
        }
        match self.shape.object_home(id) {
            Some(Home { blob, node: 0 }) => Reference::Child(self.slot(blob)),
            Some(Home { blob, node }) => Reference::ChildNode {
                slot: self.slot(blob),
                node,
            },
            None => unreachable!("an object outside a blob has a blob of its own"),
        }
    }
    fn string(&mut self, id: StringId) -> Option<u32> {
        self.shape.string_home(id).map(|blob| self.slot(blob))
    }
    fn bigint(&mut self, id: BigintId) -> Option<u32> {
        self.shape.bigint_home(id).map(|blob| self.slot(blob))
    }
    fn content_len(&mut self, s: &Graph, id: StringId) -> usize {
        match self.shape.string_home(id) {
            // The blob is the string and what is written before it.
            Some(blob) => {
                let blob = &self.shape.blobs[blob.0 as usize];
                usize::try_from(blob.encoded_len)
                    .expect("a captured string fits memory")
                    .checked_sub(STRING_BLOB_HEADER_BYTES)
                    .unwrap_or_else(|| unreachable!("a string blob has its header"))
            }
            None => s.strings[id.0 as usize].len(),
        }
    }
}

struct Writer<'a, W>(&'a mut W);
impl<W: Write> Visitor for Writer<'_, W> {
    type Error = io::Error;
    fn byte(&mut self, byte: u8) -> io::Result<()> {
        self.0.write_all(&[byte])
    }
    fn int(&mut self, value: i64) -> io::Result<()> {
        self.0.write_all(&value.to_le_bytes())
    }
    fn bits(&mut self, bits: u64) -> io::Result<()> {
        self.0.write_all(&bits.to_le_bytes())
    }
    fn index(&mut self, index: u32) -> io::Result<()> {
        self.0.write_all(&index.to_le_bytes())
    }
    fn length(&mut self, length: usize) -> io::Result<()> {
        original_len(self.0, length)
    }
    fn string(&mut self, text: &BexStr) -> io::Result<()> {
        string(self.0, text)
    }
    fn key(&mut self, text: &BexStr) -> io::Result<()> {
        string(self.0, text)
    }
    fn bigint(&mut self, value: &BigInt, _: Digest) -> io::Result<()> {
        tags::bigint_sign(value.sign()).serialize(self.0)?;
        value.bits().serialize(self.0)?;
        for digit in value.iter_u64_digits() {
            digit.serialize(self.0)?;
        }
        Ok(())
    }
    fn ty(&mut self, ty: &OwnedType, _: TypeLeaf) -> io::Result<()> {
        ty.serialize(self.0)
    }
    fn bytes(&mut self, bytes: &[u8], _: Digest) -> io::Result<()> {
        size(self.0, bytes.len())?;
        self.0.write_all(bytes)
    }
    fn declaration(&mut self, tag: TypeTag, name: &DeclarationName) -> io::Result<()> {
        tag.serialize(self.0)?;
        name.serialize(self.0)
    }
    fn begin_range(&mut self, len: usize) -> io::Result<()> {
        size(self.0, len)
    }
}

#[cfg(test)]
mod tests {
    use borsh::BorshDeserialize;

    use super::*;
    use crate::{Limits, Shaper, Snapshot, SnapshotObject as O, SnapshotPool, SnapshotValue as V};

    fn root_blob(snapshot: &Snapshot) -> Vec<u8> {
        let mut bytes = Vec::new();
        snapshot
            .root_blob()
            .write(&mut BlobScratch::default(), &mut bytes)
            .unwrap();
        bytes
    }

    #[test]
    fn binary_values_preserve_omission_nan_bigints_cycles_and_first_reference_order() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        // Reserved first, referenced second: it is numbered second.
        let later = b.leaves().object(O::Cell(V::Null)).unwrap();
        let cycle = b.leaves().reserve().unwrap();
        let cycle_id = cycle.id();
        b.fill(cycle, O::Cell(V::Object(cycle_id)));
        let text = b.leaves().string_value(&"content".into());
        let bigint = b
            .leaves()
            .bigint(&std::sync::Arc::new(num_bigint::BigInt::from(-123)));
        let nan = 0x7ff8_0000_0000_1234;
        let slots = [
            V::Null,
            V::OmittedArg,
            text,
            V::Float(f64::from_bits(nan)),
            bigint,
            V::Object(cycle_id),
            V::Object(later),
        ];
        let args = b.arguments(slots.into_iter(), |_, value| value);
        let snapshot = b.finish(args, &mut Shaper::default());
        assert_eq!(snapshot.blobs().len(), 1);
        let bytes = root_blob(&snapshot);
        let mut r = bytes.as_slice();
        assert_eq!(<[u8; 8]>::deserialize_reader(&mut r).unwrap(), BLOB_MAGIC);
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), BLOB_VERSION);
        assert_eq!(
            <[u8; 16]>::deserialize_reader(&mut r).unwrap(),
            *snapshot.root_id().as_bytes()
        );
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 0); // child blobs
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 2); // objects
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 1); // argument root
        assert_eq!(u64::deserialize_reader(&mut r).unwrap(), 7);
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 7);
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 0); // null
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 1); // omitted
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 5);
        assert_eq!(String::deserialize_reader(&mut r).unwrap(), "content");
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 4);
        assert_eq!(u64::deserialize_reader(&mut r).unwrap(), nan);
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 6);
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 0); // negative
        assert_eq!(u64::deserialize_reader(&mut r).unwrap(), 7); // significant bits
        assert_eq!(u64::deserialize_reader(&mut r).unwrap(), 123);
        for local in [0, 1] {
            assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 7); // object ref
            assert_eq!(u32::deserialize_reader(&mut r).unwrap(), local);
        }
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 5); // cell definition
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 7);
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 0); // self cycle
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 5); // cell definition
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 0); // null
        assert!(r.is_empty());
    }
    #[test]
    fn leaf_storage_sharing_does_not_change_blob_bytes() {
        let pool = SnapshotPool::new(2, Limits::default());
        let make = |reuse| {
            let mut b = pool.try_acquire().unwrap();
            let first = b.leaves().string(&"same".into()).unwrap();
            let second = if reuse {
                first
            } else {
                b.leaves().string(&"same".into()).unwrap()
            };
            let slots = [V::String(first), V::String(second)];
            let args = b.arguments(slots.into_iter(), |_, value| value);
            b.finish(args, &mut Shaper::default())
        };
        let a = make(true);
        let b = make(false);
        assert_eq!(a.root_id(), b.root_id());
        assert_eq!(root_blob(&a), root_blob(&b));
    }
}
