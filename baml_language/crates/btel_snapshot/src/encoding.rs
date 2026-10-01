//! CAS blob format 3. Explicit little-endian scalar tags, not Rust layouts.
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
//! A blob's own root value, map keys, names, types and declarations are
//! always written in place.
//!
//! A media object (tag 9) writes its kind, whether a MIME type follows and
//! that type, then a source tag: 0 with a URL, 1 with a path, each followed by
//! whether content follows, or 2, which always has content. Content is its
//! u64 text length, then the base64 text as a value, so large content can be a
//! child blob. The MIME type, URL and path are strings written in place.
//!
//! Value and object tags mirror hash format 3. Type/declaration metadata uses
//! its Borsh representation. Floats use raw bits (including NaNs). Bigints use
//! sign (0 negative, 1 zero, 2 positive), u64 bit length, then ceil(bits/64)
//! little-endian u64 magnitude limbs. Any encoding change needs a version bump.
use std::io::{self, Write};

use baml_type::{DeclarationName, typetag::TypeTag};
use borsh::BorshSerialize;
pub use btel_settings::snapshot::{BLOB_MAGIC, BLOB_VERSION};
use num_bigint::BigInt;

use crate::{
    BexStr, Blob,
    graph::{BigintId, Graph, ObjectId, OwnedType, StringId},
    hash::{Digest, TypeLeaf},
    shape::{BlobIndex, Home, Shape, UNNUMBERED},
    tags,
    walk::{self, Reference, Resolver, Visitor},
};
#[cfg(debug_assertions)]
use crate::{shape::HEADER_BYTES, walk::Length};

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
        let later = b.reserve_object().unwrap();
        let cycle = b.reserve_object().unwrap();
        b.set_object(later, O::Cell(V::Null));
        b.set_object(cycle, O::Cell(V::Object(cycle)));
        let text = b.string(&"content".into()).unwrap();
        let bigint = b
            .bigint(&std::sync::Arc::new(num_bigint::BigInt::from(-123)))
            .unwrap();
        let start = b.value_start();
        let nan = 0x7ff8_0000_0000_1234;
        for v in [
            V::Null,
            V::OmittedArg,
            V::String(text),
            V::Float(f64::from_bits(nan)),
            V::Bigint(bigint),
            V::Object(cycle),
            V::Object(later),
        ] {
            b.push_value(v);
        }
        let args = b.value_range(start);
        let snapshot = b.finish_args(7, args, &mut Shaper::default());
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
            let first = b.string(&"same".into()).unwrap();
            let second = if reuse {
                first
            } else {
                b.string(&"same".into()).unwrap()
            };
            let start = b.value_start();
            b.push_value(V::String(first));
            b.push_value(V::String(second));
            let args = b.value_range(start);
            b.finish_args(2, args, &mut Shaper::default())
        };
        let a = make(true);
        let b = make(false);
        assert_eq!(a.root_id(), b.root_id());
        assert_eq!(root_blob(&a), root_blob(&b));
    }
}
