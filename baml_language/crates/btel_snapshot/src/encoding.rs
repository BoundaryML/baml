//! CAS blob format 2. Explicit little-endian scalar tags, not Rust layouts.
//!
//! Header: `BTELCAS\0`, u32 version, 16 digest bytes, bool limited, u32 object
//! count, root, then object definitions in `ObjectId` order. Root is tag 0/value
//! or tag 1/u64 parameter count/value sequence. Sequences and UTF-8 strings use
//! u32 lengths. Values inline immutable leaf contents; only graph objects use
//! u32 references. This makes leaf-arena sharing irrelevant to the bytes.
//!
//! Value and object tags mirror hash format 2. Type/declaration metadata uses
//! its Borsh representation. Floats use raw bits (including NaNs). Bigints use
//! sign (0 negative, 1 zero, 2 positive), u64 bit length, then ceil(bits/64)
//! little-endian u64 magnitude limbs. Any encoding change needs a version bump.
use std::io::{self, Write};

use borsh::BorshSerialize;
pub use btel_settings::snapshot::{BLOB_MAGIC, BLOB_VERSION};

use crate::{Description, Limit, Snapshot, SnapshotObject, SnapshotRoot, SnapshotValue};

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
fn limit(w: &mut impl Write, value: Limit) -> io::Result<()> {
    let tag: u8 = match value {
        Limit::Values => 0,
        Limit::Objects => 1,
        Limit::Bytes => 2,
        Limit::Depth => 3,
    };
    tag.serialize(w)
}
fn description(w: &mut impl Write, value: Description) -> io::Result<()> {
    let tag: u8 = match value {
        Description::Function => 0,
        Description::Closure => 1,
        Description::BoundMethod => 2,
        Description::GenericFunction => 3,
        Description::HostFunction => 4,
        Description::Future => 5,
        Description::UnscheduledFuture => 6,
        Description::Package => 7,
        Description::Interface => 8,
        Description::Implementation => 9,
        Description::TypeAlias => 10,
        Description::Sentinel => 11,
    };
    tag.serialize(w)
}
impl Snapshot {
    /// Stream a complete binary graph without cloning owners or allocating an
    /// intermediate encoded payload. The writer chooses its own buffering.
    pub fn write_blob(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&BLOB_MAGIC)?;
        BLOB_VERSION.serialize(w)?;
        w.write_all(self.id().as_bytes())?;
        self.stats().limited.serialize(w)?;
        size(w, self.object_count())?;
        match self.root() {
            SnapshotRoot::Value(value) => {
                0_u8.serialize(w)?;
                self.write_value(w, value)?;
            }
            SnapshotRoot::FunctionArgs(args) => {
                1_u8.serialize(w)?;
                original_len(w, args.parameter_count)?;
                self.write_values(w, self.values(args.slots))?;
            }
        }
        for object in &self.0.objects {
            match object {
                SnapshotObject::Bytes {
                    data,
                    original_len: n,
                } => {
                    0_u8.serialize(w)?;
                    original_len(w, *n)?;
                    let bytes = self.bytes(*data);
                    size(w, bytes.len())?;
                    w.write_all(bytes)?;
                }
                SnapshotObject::List {
                    element_type,
                    items,
                    original_len: n,
                } => {
                    1_u8.serialize(w)?;
                    self.ty(*element_type).serialize(w)?;
                    original_len(w, *n)?;
                    self.write_values(w, self.values(*items))?;
                }
                SnapshotObject::Map {
                    key_type,
                    value_type,
                    entries,
                    original_len: n,
                } => {
                    2_u8.serialize(w)?;
                    self.ty(*key_type).serialize(w)?;
                    self.ty(*value_type).serialize(w)?;
                    original_len(w, *n)?;
                    self.write_entries(w, self.entries(*entries))?;
                }
                SnapshotObject::Instance {
                    type_arguments,
                    declaration,
                    fields,
                    original_len: n,
                } => {
                    3_u8.serialize(w)?;
                    self.type_arguments(*type_arguments).serialize(w)?;
                    declaration.0.serialize(w)?;
                    original_len(w, *n)?;
                    self.write_entries(w, self.entries(*fields))?;
                }
                SnapshotObject::Declaration { name, tag, is_enum } => {
                    4_u8.serialize(w)?;
                    tag.serialize(w)?;
                    name.serialize(w)?;
                    is_enum.serialize(w)?;
                }
                SnapshotObject::Cell(v) => {
                    5_u8.serialize(w)?;
                    self.write_value(w, *v)?;
                }
                SnapshotObject::NonSnapshotableValue {} => {
                    6_u8.serialize(w)?;
                }
                SnapshotObject::Descriptive { kind, name } => {
                    7_u8.serialize(w)?;
                    description(w, *kind)?;
                    name.is_some().serialize(w)?;
                    if let Some(id) = name {
                        string(w, self.string(*id))?;
                    }
                }
                SnapshotObject::Truncated(reason) => {
                    8_u8.serialize(w)?;
                    limit(w, *reason)?;
                }
            }
        }
        Ok(())
    }
    fn write_values(&self, w: &mut impl Write, values: &[SnapshotValue]) -> io::Result<()> {
        size(w, values.len())?;
        for value in values {
            self.write_value(w, *value)?;
        }
        Ok(())
    }
    fn write_entries(&self, w: &mut impl Write, entries: &[crate::MapEntry]) -> io::Result<()> {
        size(w, entries.len())?;
        for entry in entries {
            string(w, &entry.key)?;
            self.write_value(w, entry.value)?;
        }
        Ok(())
    }
    fn write_value(&self, w: &mut impl Write, value: SnapshotValue) -> io::Result<()> {
        match value {
            SnapshotValue::Null => 0_u8.serialize(w),
            SnapshotValue::OmittedArg => 1_u8.serialize(w),
            SnapshotValue::Bool(v) => {
                2_u8.serialize(w)?;
                v.serialize(w)
            }
            SnapshotValue::Int(v) => {
                3_u8.serialize(w)?;
                v.serialize(w)
            }
            SnapshotValue::Float(v) => {
                4_u8.serialize(w)?;
                v.to_bits().serialize(w)
            }
            SnapshotValue::String(id) => {
                5_u8.serialize(w)?;
                string(w, self.string(id))
            }
            SnapshotValue::Bigint(id) => {
                6_u8.serialize(w)?;
                let n = self.bigint(id);
                let sign: u8 = match n.sign() {
                    num_bigint::Sign::Minus => 0,
                    num_bigint::Sign::NoSign => 1,
                    num_bigint::Sign::Plus => 2,
                };
                sign.serialize(w)?;
                n.bits().serialize(w)?;
                for digit in n.iter_u64_digits() {
                    digit.serialize(w)?;
                }
                Ok(())
            }
            SnapshotValue::Object(id) => {
                7_u8.serialize(w)?;
                id.0.serialize(w)
            }
            SnapshotValue::Type(id) => {
                8_u8.serialize(w)?;
                self.ty(id).serialize(w)
            }
            SnapshotValue::Enum {
                declaration,
                variant,
                name,
            } => {
                9_u8.serialize(w)?;
                declaration.0.serialize(w)?;
                variant.serialize(w)?;
                string(w, self.string(name))
            }
            SnapshotValue::Truncated(reason) => {
                10_u8.serialize(w)?;
                limit(w, reason)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use borsh::BorshDeserialize;

    use super::*;
    use crate::{Limits, SnapshotObject as O, SnapshotPool, SnapshotValue as V};

    #[test]
    fn binary_values_preserve_omission_nan_bigints_and_cycles() {
        let pool = SnapshotPool::new(1, Limits::default());
        let mut b = pool.try_acquire().unwrap();
        let object = b.reserve_object().unwrap();
        b.set_object(object, O::Cell(V::Object(object)));
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
            V::Object(object),
        ] {
            b.push_value(v);
        }
        let args = b.value_range(start);
        let snapshot = b.finish_args(6, args);
        let mut bytes = Vec::new();
        snapshot.write_blob(&mut bytes).unwrap();
        let mut r = bytes.as_slice();
        assert_eq!(<[u8; 8]>::deserialize_reader(&mut r).unwrap(), BLOB_MAGIC);
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), BLOB_VERSION);
        assert_eq!(
            <[u8; 16]>::deserialize_reader(&mut r).unwrap(),
            *snapshot.id().as_bytes()
        );
        assert!(!bool::deserialize_reader(&mut r).unwrap());
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 1); // object table
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 1); // argument root
        assert_eq!(u64::deserialize_reader(&mut r).unwrap(), 6);
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 6);
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
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 7); // object ref
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 0);
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 5); // cell definition
        assert_eq!(u8::deserialize_reader(&mut r).unwrap(), 7);
        assert_eq!(u32::deserialize_reader(&mut r).unwrap(), 0); // self cycle
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
            b.finish_args(2, args)
        };
        let a = make(true);
        let b = make(false);
        assert_eq!(a.id(), b.id());
        let mut x = Vec::new();
        let mut y = Vec::new();
        a.write_blob(&mut x).unwrap();
        b.write_blob(&mut y).unwrap();
        assert_eq!(x, y);
    }
}
