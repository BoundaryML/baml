//! Shaping a finished capture into content-addressed blobs.
//!
//! A blob's objects are numbered in first-reference order: walking the blob in
//! encoding order from its root, each object takes the next number the first
//! time a reference to it is met. The same walk hashes the root and every
//! object definition with those numbers, so numbering and hashing cannot
//! disagree about order, and a blob's bytes and identity never depend on the
//! order in which capture discovered its objects. Objects the root cannot
//! reach are not part of the blob.
//!
//! Each capture is one blob: its root and everything reachable from it.
use super::{
    BlobEntry, CasId, MapEntry, ObjectId, OwnedType, Range, SnapshotObject, SnapshotRoot,
    SnapshotValue, Storage, grow,
    hash::{Absorb, Digest, Hasher},
    tags::{self, HashDomain, ObjectTag, RootTag, ValueTag},
};

/// Reusable shaping scratch. Holds no capture data between captures; keep one
/// per capturing thread.
#[derive(Default)]
pub struct Shaper {
    /// Capture `ObjectId` to blob-local number, [`UNNUMBERED`] outside the blob.
    local: Vec<u32>,
    /// Blob members in local order.
    order: Vec<ObjectId>,
    /// Object digests in local order.
    digests: Vec<Digest>,
    /// The root's hash input, replayed after the object count it follows.
    root: Vec<u8>,
}

/// Local number of an object outside the blob being shaped or written.
pub(crate) const UNNUMBERED: u32 = u32::MAX;

impl Shaper {
    pub(crate) fn shape(&mut self, s: &mut Storage, root: SnapshotRoot) {
        debug_assert!(s.blobs.is_empty(), "a capture is shaped once");
        let id = self.number_and_hash(s, root, &[]);
        let pool = s
            .owner
            .as_ref()
            .unwrap_or_else(|| unreachable!("builder storage has an owner"));
        let start = s.members.len();
        grow(&mut s.members, self.order.len(), pool, &mut s.charged);
        s.members.extend_from_slice(&self.order);
        grow(&mut s.blobs, 1, pool, &mut s.charged);
        s.blobs.push(BlobEntry {
            id,
            root,
            members: Range::new(start, self.order.len()),
            children: Range::empty(),
        });
    }

    /// Number the blob rooted at `root` into `self.order` and return its ID.
    fn number_and_hash(&mut self, s: &Storage, root: SnapshotRoot, children: &[CasId]) -> CasId {
        let Self {
            local,
            order,
            digests,
            root: root_input,
        } = self;
        local.clear();
        local.resize(s.objects.len(), UNNUMBERED);
        order.clear();
        digests.clear();
        root_input.clear();
        let mut numbering = Numbering { local, order };
        match root {
            SnapshotRoot::Value(value) => {
                root_input.byte(RootTag::Value as u8);
                value_input(root_input, value, s, &mut numbering);
            }
            SnapshotRoot::FunctionArgs(args) => {
                root_input.byte(RootTag::FunctionArgs as u8);
                root_input.size(args.parameter_count);
                values_input(root_input, args.slots, s, &mut numbering);
            }
        }
        // Hashing a definition numbers its new references, which extends
        // `order` with the objects to hash next.
        let mut next = 0;
        while let Some(&id) = numbering.order.get(next) {
            digests.push(object_digest(&s.objects[id.0 as usize], s, &mut numbering));
            next += 1;
        }
        let mut h = Hasher::new(HashDomain::Blob);
        h.size(children.len());
        for child in children {
            h.absorb(child.as_bytes());
        }
        h.size(numbering.order.len());
        h.absorb(root_input);
        for digest in digests.iter() {
            h.digest(*digest);
        }
        CasId::from_bytes(h.finish().0)
    }
}

struct Numbering<'a> {
    local: &'a mut [u32],
    order: &'a mut Vec<ObjectId>,
}
impl Numbering<'_> {
    fn number(&mut self, id: ObjectId) -> u64 {
        let slot = &mut self.local[id.0 as usize];
        if *slot == UNNUMBERED {
            *slot = u32::try_from(self.order.len()).expect("bounded objects");
            self.order.push(id);
        }
        u64::from(*slot)
    }
}

fn value_input(h: &mut impl Absorb, value: SnapshotValue, s: &Storage, n: &mut Numbering<'_>) {
    match value {
        SnapshotValue::Null => h.byte(ValueTag::Null as u8),
        SnapshotValue::OmittedArg => h.byte(ValueTag::OmittedArg as u8),
        SnapshotValue::Bool(v) => {
            h.byte(ValueTag::Bool as u8);
            h.byte(u8::from(v));
        }
        SnapshotValue::Int(v) => {
            h.byte(ValueTag::Int as u8);
            h.absorb(&v.to_le_bytes());
        }
        SnapshotValue::Float(v) => {
            h.byte(ValueTag::Float as u8);
            h.number(v.to_bits());
        }
        SnapshotValue::String(id) => {
            h.byte(ValueTag::String as u8);
            h.digest(s.string_hashes[id.0 as usize]);
        }
        SnapshotValue::Bigint(id) => {
            h.byte(ValueTag::Bigint as u8);
            h.digest(s.bigint_hashes[id.0 as usize]);
        }
        SnapshotValue::Object(id) => {
            h.byte(ValueTag::Object as u8);
            h.number(n.number(id));
        }
        SnapshotValue::Type(id) => {
            h.byte(ValueTag::Type as u8);
            h.digest(s.type_hashes[id.0 as usize]);
        }
        SnapshotValue::Enum {
            declaration,
            variant,
            name,
        } => {
            h.byte(ValueTag::Enum as u8);
            h.number(n.number(declaration));
            h.number(u64::from(variant));
            h.digest(s.string_hashes[name.0 as usize]);
        }
        SnapshotValue::Truncated(limit) => {
            h.byte(ValueTag::Truncated as u8);
            h.byte(tags::limit(limit));
        }
    }
}

fn values_input(
    h: &mut impl Absorb,
    range: Range<SnapshotValue>,
    s: &Storage,
    n: &mut Numbering<'_>,
) {
    let mut items = Hasher::new(HashDomain::Range);
    for value in &s.values[range.indexes()] {
        value_input(&mut items, *value, s, n);
    }
    h.size(range.len());
    h.digest(items.finish());
}

fn entries_input(h: &mut impl Absorb, range: Range<MapEntry>, s: &Storage, n: &mut Numbering<'_>) {
    let mut items = Hasher::new(HashDomain::Range);
    for entry in &s.entries[range.indexes()] {
        items.string_parts(entry.key.len(), entry.key.content_hash());
        value_input(&mut items, entry.value, s, n);
    }
    h.size(range.len());
    h.digest(items.finish());
}

fn types_input(h: &mut impl Absorb, range: Range<OwnedType>, s: &Storage) {
    let mut items = Hasher::new(HashDomain::Range);
    for digest in &s.type_hashes[range.indexes()] {
        items.digest(*digest);
    }
    h.size(range.len());
    h.digest(items.finish());
}

fn object_digest(object: &SnapshotObject, s: &Storage, n: &mut Numbering<'_>) -> Digest {
    let mut h = Hasher::new(HashDomain::Object);
    match object {
        SnapshotObject::Uint8Array { data, original_len } => {
            h.byte(ObjectTag::Uint8Array as u8);
            h.size(*original_len);
            h.size(data.len());
            h.digest(data.digest);
        }
        SnapshotObject::List {
            element_type,
            items,
            original_len,
        } => {
            h.byte(ObjectTag::List as u8);
            h.digest(s.type_hashes[element_type.0 as usize]);
            h.size(*original_len);
            values_input(&mut h, *items, s, n);
        }
        SnapshotObject::Map {
            key_type,
            value_type,
            entries,
            original_len,
        } => {
            h.byte(ObjectTag::Map as u8);
            h.digest(s.type_hashes[key_type.0 as usize]);
            h.digest(s.type_hashes[value_type.0 as usize]);
            h.size(*original_len);
            entries_input(&mut h, *entries, s, n);
        }
        SnapshotObject::Instance {
            type_arguments,
            declaration,
            fields,
            original_len,
        } => {
            h.byte(ObjectTag::Instance as u8);
            types_input(&mut h, *type_arguments, s);
            h.number(n.number(*declaration));
            h.size(*original_len);
            entries_input(&mut h, *fields, s, n);
        }
        SnapshotObject::Declaration { name, tag, is_enum } => {
            h.byte(ObjectTag::Declaration as u8);
            h.borsh(tag);
            h.borsh(name);
            h.byte(u8::from(*is_enum));
        }
        SnapshotObject::Cell(value) => {
            h.byte(ObjectTag::Cell as u8);
            value_input(&mut h, *value, s, n);
        }
        SnapshotObject::NonSnapshotableValue {} => h.byte(ObjectTag::NonSnapshotableValue as u8),
        SnapshotObject::Descriptive { kind, name } => {
            h.byte(ObjectTag::Descriptive as u8);
            h.byte(tags::description(*kind));
            h.byte(u8::from(name.is_some()));
            if let Some(name) = name {
                h.digest(s.string_hashes[name.0 as usize]);
            }
        }
        SnapshotObject::Truncated(limit) => {
            h.byte(ObjectTag::Truncated as u8);
            h.byte(tags::limit(*limit));
        }
    }
    h.finish()
}
