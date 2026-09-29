//! CAS blob format 3. Explicit little-endian scalar tags, not Rust layouts.
//!
//! Header: `BTELCAS\0`, u32 version, 16 digest bytes, u32 child count and each
//! child's 16-byte ID, u32 object count, root, then object definitions in
//! blob-local order. Root is tag 0/value or tag 1/u64 parameter count/value
//! sequence. Sequences and UTF-8 strings use u32 lengths. Values inline
//! immutable leaf contents; only graph objects use u32 references, by local
//! number. Numbers follow each object's first reference in this byte order,
//! so a blob has exactly one encoding and the reader rejects any other. This
//! also makes leaf-arena sharing irrelevant to the bytes.
//!
//! Value and object tags mirror hash format 3. Type/declaration metadata uses
//! its Borsh representation. Floats use raw bits (including NaNs). Bigints use
//! sign (0 negative, 1 zero, 2 positive), u64 bit length, then ceil(bits/64)
//! little-endian u64 magnitude limbs. Any encoding change needs a version bump.
use std::io::{self, Write};

use borsh::BorshSerialize;
pub use btel_settings::snapshot::{BLOB_MAGIC, BLOB_VERSION};

use crate::{
    Blob, ObjectId, Snapshot, SnapshotObject, SnapshotRoot, SnapshotValue,
    shape::UNNUMBERED,
    tags::{self, ObjectTag, RootTag, ValueTag},
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
fn string(w: &mut impl Write, value: &bex_str::BexStr) -> io::Result<()> {
    size(w, value.len())?;
    w.write_all(value.as_bytes())
}

/// Reusable map from capture objects to blob-local numbers. Any snapshot's
/// blobs may share one; it holds nothing between writes.
#[derive(Default)]
pub struct BlobScratch {
    local: Vec<u32>,
}

impl Blob<'_> {
    /// Stream this blob's canonical bytes without cloning owners or
    /// allocating an encoded payload. The writer chooses its own buffering.
    pub fn write(&self, scratch: &mut BlobScratch, w: &mut impl Write) -> io::Result<()> {
        let s = &self.snapshot.0;
        let entry = self.entry();
        let members = &s.members[entry.members.indexes()];
        let local = &mut scratch.local;
        local.clear();
        local.resize(s.objects.len(), UNNUMBERED);
        for (number, id) in members.iter().enumerate() {
            local[id.0 as usize] = u32::try_from(number).expect("bounded objects");
        }
        let encoder = Encoder {
            snapshot: self.snapshot,
            local,
        };
        w.write_all(&BLOB_MAGIC)?;
        BLOB_VERSION.serialize(w)?;
        w.write_all(entry.id.as_bytes())?;
        let children = &s.blob_children[entry.children.indexes()];
        size(w, children.len())?;
        for child in children {
            w.write_all(s.blobs[child.0 as usize].id.as_bytes())?;
        }
        size(w, members.len())?;
        match entry.root {
            SnapshotRoot::Value(value) => {
                (RootTag::Value as u8).serialize(w)?;
                encoder.value(w, value)?;
            }
            SnapshotRoot::FunctionArgs(args) => {
                (RootTag::FunctionArgs as u8).serialize(w)?;
                original_len(w, args.parameter_count)?;
                encoder.values(w, self.snapshot.values(args.slots))?;
            }
        }
        for id in members {
            encoder.object(w, self.snapshot.object(*id))?;
        }
        Ok(())
    }
}

struct Encoder<'a> {
    snapshot: &'a Snapshot,
    local: &'a [u32],
}

impl Encoder<'_> {
    fn reference(&self, w: &mut impl Write, id: ObjectId) -> io::Result<()> {
        let number = self.local[id.0 as usize];
        debug_assert_ne!(number, UNNUMBERED, "a blob references only its members");
        number.serialize(w)
    }
    fn object(&self, w: &mut impl Write, object: &SnapshotObject) -> io::Result<()> {
        let snapshot = self.snapshot;
        match object {
            SnapshotObject::Uint8Array {
                data,
                original_len: n,
            } => {
                (ObjectTag::Uint8Array as u8).serialize(w)?;
                original_len(w, *n)?;
                let bytes = snapshot.bytes(*data);
                size(w, bytes.len())?;
                w.write_all(bytes)
            }
            SnapshotObject::List {
                element_type,
                items,
                original_len: n,
            } => {
                (ObjectTag::List as u8).serialize(w)?;
                snapshot.ty(*element_type).serialize(w)?;
                original_len(w, *n)?;
                self.values(w, snapshot.values(*items))
            }
            SnapshotObject::Map {
                key_type,
                value_type,
                entries,
                original_len: n,
            } => {
                (ObjectTag::Map as u8).serialize(w)?;
                snapshot.ty(*key_type).serialize(w)?;
                snapshot.ty(*value_type).serialize(w)?;
                original_len(w, *n)?;
                self.entries(w, snapshot.entries(*entries))
            }
            SnapshotObject::Instance {
                type_arguments,
                declaration,
                fields,
                original_len: n,
            } => {
                (ObjectTag::Instance as u8).serialize(w)?;
                snapshot.type_arguments(*type_arguments).serialize(w)?;
                self.reference(w, *declaration)?;
                original_len(w, *n)?;
                self.entries(w, snapshot.entries(*fields))
            }
            SnapshotObject::Declaration { name, tag, is_enum } => {
                (ObjectTag::Declaration as u8).serialize(w)?;
                tag.serialize(w)?;
                name.serialize(w)?;
                is_enum.serialize(w)
            }
            SnapshotObject::Cell(v) => {
                (ObjectTag::Cell as u8).serialize(w)?;
                self.value(w, *v)
            }
            SnapshotObject::NonSnapshotableValue {} => {
                (ObjectTag::NonSnapshotableValue as u8).serialize(w)
            }
            SnapshotObject::Descriptive { kind, name } => {
                (ObjectTag::Descriptive as u8).serialize(w)?;
                tags::description(*kind).serialize(w)?;
                name.is_some().serialize(w)?;
                match name {
                    Some(id) => string(w, snapshot.string(*id)),
                    None => Ok(()),
                }
            }
            SnapshotObject::Truncated(reason) => {
                (ObjectTag::Truncated as u8).serialize(w)?;
                tags::limit(*reason).serialize(w)
            }
        }
    }
    fn values(&self, w: &mut impl Write, values: &[SnapshotValue]) -> io::Result<()> {
        size(w, values.len())?;
        for value in values {
            self.value(w, *value)?;
        }
        Ok(())
    }
    fn entries(&self, w: &mut impl Write, entries: &[crate::MapEntry]) -> io::Result<()> {
        size(w, entries.len())?;
        for entry in entries {
            string(w, &entry.key)?;
            self.value(w, entry.value)?;
        }
        Ok(())
    }
    fn value(&self, w: &mut impl Write, value: SnapshotValue) -> io::Result<()> {
        let snapshot = self.snapshot;
        match value {
            SnapshotValue::Null => (ValueTag::Null as u8).serialize(w),
            SnapshotValue::OmittedArg => (ValueTag::OmittedArg as u8).serialize(w),
            SnapshotValue::Bool(v) => {
                (ValueTag::Bool as u8).serialize(w)?;
                v.serialize(w)
            }
            SnapshotValue::Int(v) => {
                (ValueTag::Int as u8).serialize(w)?;
                v.serialize(w)
            }
            SnapshotValue::Float(v) => {
                (ValueTag::Float as u8).serialize(w)?;
                v.to_bits().serialize(w)
            }
            SnapshotValue::String(id) => {
                (ValueTag::String as u8).serialize(w)?;
                string(w, snapshot.string(id))
            }
            SnapshotValue::Bigint(id) => {
                (ValueTag::Bigint as u8).serialize(w)?;
                let n = snapshot.bigint(id);
                tags::bigint_sign(n.sign()).serialize(w)?;
                n.bits().serialize(w)?;
                for digit in n.iter_u64_digits() {
                    digit.serialize(w)?;
                }
                Ok(())
            }
            SnapshotValue::Object(id) => {
                (ValueTag::Object as u8).serialize(w)?;
                self.reference(w, id)
            }
            SnapshotValue::Type(id) => {
                (ValueTag::Type as u8).serialize(w)?;
                snapshot.ty(id).serialize(w)
            }
            SnapshotValue::Enum {
                declaration,
                variant,
                name,
            } => {
                (ValueTag::Enum as u8).serialize(w)?;
                self.reference(w, declaration)?;
                variant.serialize(w)?;
                string(w, snapshot.string(name))
            }
            SnapshotValue::Truncated(reason) => {
                (ValueTag::Truncated as u8).serialize(w)?;
                tags::limit(reason).serialize(w)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use borsh::BorshDeserialize;

    use super::*;
    use crate::{Limits, Shaper, SnapshotObject as O, SnapshotPool, SnapshotValue as V};

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
