//! Snapshot hash format 1: XXH3-128, seed zero, little-endian digest bytes.
//!
//! Values/entries are hashed as they are appended. A container definition uses
//! these range digests and explicit object-reference numbers. Freezing folds the
//! ordered object definitions with the root. It never rereads primitive arrays.
//! This graph format handles cycles without recursive Merkle dependencies.
//! Object numbering follows capture discovery order; map/argument order matters.
//! Leaf indexes, allocator addresses, capacities and string rope shape do not.
//! Borsh's current type/attribute encoding is part of version 1: changes to it
//! require a hash-format version change. Hash equality is not proof of delivery.
use std::io::{self, Write};

use borsh::BorshSerialize;
use xxhash_rust::xxh3::Xxh3;

use super::{
    BexStr, BigInt, Description, Limit, OwnedType, Range, SnapshotObject, SnapshotRoot,
    SnapshotValue, Storage, TypeIdentity,
};

/// Whole-snapshot content identity. A zero digest is a valid hash, never absence.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SnapshotId([u8; 16]);
impl SnapshotId {
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Digest(pub(super) [u8; 16]);
pub(super) struct Hasher(Xxh3);
impl Hasher {
    pub(super) fn new(kind: u8) -> Self {
        let mut h = Self(Xxh3::new());
        h.0.update(b"baml.snapshot.xxh3-128.v1\0");
        h.byte(kind);
        h
    }
    pub(super) fn byte(&mut self, n: u8) {
        self.0.update(&[n]);
    }
    pub(super) fn number(&mut self, n: u64) {
        self.0.update(&n.to_le_bytes());
    }
    pub(super) fn size(&mut self, n: usize) {
        self.number(u64::try_from(n).expect("snapshot length"));
    }
    pub(super) fn digest(&mut self, h: Digest) {
        self.0.update(&h.0);
    }
    pub(super) fn raw(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
    pub(super) fn string(&mut self, s: &BexStr) {
        self.size(s.len());
        self.raw(&s.content_hash().to_le_bytes());
    }
    pub(super) fn borsh(&mut self, value: &impl BorshSerialize) {
        value.serialize(self).expect("infallible hash writer");
    }
    pub(super) fn finish(&self) -> Digest {
        Digest(self.0.digest128().to_le_bytes())
    }
}
impl Write for Hasher {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl BorshSerialize for TypeIdentity {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match self {
            Self::Resolved(head) => {
                1_u8.serialize(w)?;
                head.tag().serialize(w)?;
                head.name().serialize(w)
            }
            Self::Unresolved(tag) => {
                0_u8.serialize(w)?;
                tag.serialize(w)
            }
        }
    }
}
pub(super) fn string(s: &BexStr) -> Digest {
    let mut h = Hasher::new(1);
    h.string(s);
    h.finish()
}
pub(super) fn bigint(n: &BigInt) -> Digest {
    let mut h = Hasher::new(2);
    h.byte(match n.sign() {
        num_bigint::Sign::Minus => 0,
        num_bigint::Sign::NoSign => 1,
        num_bigint::Sign::Plus => 2,
    });
    h.number(n.bits());
    for digit in n.iter_u64_digits() {
        h.number(digit);
    }
    h.finish()
}
pub(super) fn ty(ty: &OwnedType) -> Digest {
    let mut h = Hasher::new(3);
    h.borsh(ty);
    h.finish()
}
fn limit(l: Limit) -> u8 {
    match l {
        Limit::Values => 0,
        Limit::Objects => 1,
        Limit::Bytes => 2,
        Limit::Depth => 3,
    }
}
fn description(d: Description) -> u8 {
    match d {
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
    }
}
pub(super) fn value(
    h: &mut Hasher,
    v: SnapshotValue,
    strings: &[Digest],
    bigints: &[Digest],
    types: &[Digest],
) {
    match v {
        SnapshotValue::Null => h.byte(0),
        SnapshotValue::OmittedArg => h.byte(1),
        SnapshotValue::Bool(v) => {
            h.byte(2);
            h.byte(u8::from(v));
        }
        SnapshotValue::Int(v) => {
            h.byte(3);
            h.0.update(&v.to_le_bytes());
        }
        SnapshotValue::Float(v) => {
            h.byte(4);
            h.number(v.to_bits());
        }
        SnapshotValue::String(id) => {
            h.byte(5);
            h.digest(strings[id.0 as usize]);
        }
        SnapshotValue::Bigint(id) => {
            h.byte(6);
            h.digest(bigints[id.0 as usize]);
        }
        SnapshotValue::Object(id) => {
            h.byte(7);
            h.number(u64::from(id.0));
        }
        SnapshotValue::Type(id) => {
            h.byte(8);
            h.digest(types[id.0 as usize]);
        }
        SnapshotValue::Enum {
            declaration,
            variant,
            name,
        } => {
            h.byte(9);
            h.number(u64::from(declaration.0));
            h.number(u64::from(variant));
            h.digest(strings[name.0 as usize]);
        }
        SnapshotValue::Truncated(l) => {
            h.byte(10);
            h.byte(limit(l));
        }
    }
}
fn range<T>(h: &mut Hasher, r: Range<T>) {
    h.number(u64::from(r.len));
    h.digest(r.hash);
}
pub(super) fn object(object: &SnapshotObject, s: &Storage) -> Digest {
    let mut h = Hasher::new(4);
    match object {
        SnapshotObject::Bytes { data, original_len } => {
            h.byte(0);
            h.size(*original_len);
            range(&mut h, *data);
        }
        SnapshotObject::List {
            element_type,
            items,
            original_len,
        } => {
            h.byte(1);
            h.digest(s.type_hashes[element_type.0 as usize]);
            h.size(*original_len);
            range(&mut h, *items);
        }
        SnapshotObject::Map {
            key_type,
            value_type,
            entries,
            original_len,
        } => {
            h.byte(2);
            h.digest(s.type_hashes[key_type.0 as usize]);
            h.digest(s.type_hashes[value_type.0 as usize]);
            h.size(*original_len);
            range(&mut h, *entries);
        }
        SnapshotObject::Instance {
            type_arguments,
            declaration,
            fields,
            original_len,
        } => {
            h.byte(3);
            range(&mut h, *type_arguments);
            h.number(u64::from(declaration.0));
            h.size(*original_len);
            range(&mut h, *fields);
        }
        SnapshotObject::Declaration { name, tag, is_enum } => {
            h.byte(4);
            h.borsh(tag);
            h.borsh(name);
            h.byte(u8::from(*is_enum));
        }
        SnapshotObject::Cell(v) => {
            h.byte(5);
            value(
                &mut h,
                *v,
                &s.string_hashes,
                &s.bigint_hashes,
                &s.type_hashes,
            );
        }
        SnapshotObject::NonSnapshotableValue {} => h.byte(6),
        SnapshotObject::Descriptive { kind, name } => {
            h.byte(7);
            h.byte(description(*kind));
            h.byte(u8::from(name.is_some()));
            if let Some(name) = name {
                h.digest(s.string_hashes[name.0 as usize]);
            }
        }
        SnapshotObject::Truncated(l) => {
            h.byte(8);
            h.byte(limit(*l));
        }
    }
    h.finish()
}
pub(super) fn snapshot(s: &Storage) -> SnapshotId {
    let mut h = Hasher::new(5);
    match s.root.unwrap() {
        SnapshotRoot::Value(v) => {
            h.byte(0);
            value(
                &mut h,
                v,
                &s.string_hashes,
                &s.bigint_hashes,
                &s.type_hashes,
            );
        }
        SnapshotRoot::FunctionArgs(args) => {
            h.byte(1);
            h.size(args.parameter_count);
            range(&mut h, args.slots);
        }
    }
    h.byte(u8::from(s.stats.limited));
    h.size(s.object_hashes.len());
    for hash in &s.object_hashes {
        h.digest(*hash);
    }
    SnapshotId(h.finish().0)
}
