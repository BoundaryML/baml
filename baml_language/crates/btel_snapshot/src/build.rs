//! Building a capture. A [`Builder`] fills one storage; finishing it shapes
//! the capture into blobs and yields the immutable [`Snapshot`].
use std::sync::Arc;

use baml_type::{DeclarationName, typetag::TypeTag};
use bex_str::BexStr;
use num_bigint::BigInt;

use crate::{
    Shaper, Snapshot, SnapshotPool,
    graph::{
        Bigint, BigintId, FunctionArgs, Limit, MapEntry, NameId, ObjectId, OwnedType, Range,
        SnapshotObject, SnapshotRoot, SnapshotValue, StringId, Type, TypeId, Uint8ArrayData,
    },
    hash::{self, Absorb as _},
    pool::{Lease, Limits, Storage},
    tags,
};

/// The most items an arena indexed by `u32` holds.
const ARENA_ITEMS: usize = u32::MAX as usize - 1;

/// A capture being built.
pub struct Builder(Lease);
impl Builder {
    pub(crate) fn new(lease: Lease) -> Self {
        Self(lease)
    }
    pub fn limits(&self) -> Limits {
        self.0.limits
    }
    pub fn remaining_values(&self) -> usize {
        self.0
            .limits
            .max_values
            .unwrap_or(ARENA_ITEMS)
            .saturating_sub(self.0.graph.values.len())
    }
    pub fn remaining_entries(&self) -> usize {
        self.0
            .limits
            .max_values
            .unwrap_or(ARENA_ITEMS)
            .saturating_sub(self.0.graph.entries.len())
    }
    /// An object's identity, before its content: `None` once the object
    /// limit is reached. Until it is set it reads as truncated.
    pub fn reserve_object(&mut self) -> Option<ObjectId> {
        let Storage {
            graph,
            meter,
            limits,
            ..
        } = &mut *self.0;
        if graph.objects.len() >= limits.max_objects.unwrap_or(ARENA_ITEMS) {
            return None;
        }
        let id = ObjectId(u32::try_from(graph.objects.len()).expect("bounded objects"));
        graph
            .objects
            .push(SnapshotObject::Truncated(Limit::Objects), meter);
        Some(id)
    }
    pub fn set_object(&mut self, id: ObjectId, object: SnapshotObject) {
        self.0.graph.objects[id.0 as usize] = object;
    }
    pub fn value_start(&mut self) -> usize {
        self.0.graph.values.len()
    }
    /// Reserve capacity once for a container, without initializing unused slots.
    pub fn reserve_values(&mut self, count: usize) {
        assert!(count <= self.remaining_values());
        let Storage { graph, meter, .. } = &mut *self.0;
        graph.values.reserve(count, meter);
    }
    pub fn push_value(&mut self, value: SnapshotValue) {
        assert!(self.remaining_values() > 0);
        let Storage { graph, meter, .. } = &mut *self.0;
        graph.values.push(value, meter);
    }
    pub fn value_range(&mut self, start: usize) -> Range<SnapshotValue> {
        Range::new(start, self.0.graph.values.len() - start)
    }
    pub fn entry_start(&mut self) -> usize {
        self.0.graph.entries.len()
    }
    pub fn reserve_entries(&mut self, count: usize) {
        assert!(count <= self.remaining_entries());
        let Storage { graph, meter, .. } = &mut *self.0;
        graph.entries.reserve(count, meter);
    }
    pub fn entry(&mut self, key: &BexStr, value: SnapshotValue) {
        assert!(self.remaining_entries() > 0);
        let Storage { graph, meter, .. } = &mut *self.0;
        graph.entries.push(
            MapEntry {
                key: key.clone(),
                value,
            },
            meter,
        );
    }
    pub fn entry_range(&mut self, start: usize) -> Range<MapEntry> {
        Range::new(start, self.0.graph.entries.len() - start)
    }
    /// Hold the string by handle. `None` when no more strings can be numbered.
    pub fn string(&mut self, value: &BexStr) -> Option<StringId> {
        let Storage { graph, meter, .. } = &mut *self.0;
        if graph.strings.len() >= ARENA_ITEMS {
            return None;
        }
        let id = StringId(u32::try_from(graph.strings.len()).expect("bounded strings"));
        graph.strings.push(value.clone(), meter);
        Some(id)
    }
    /// Hold the bigint by handle. `None` when no more bigints can be numbered.
    pub fn bigint(&mut self, value: &Arc<BigInt>) -> Option<BigintId> {
        let Storage { graph, meter, .. } = &mut *self.0;
        if graph.bigints.len() >= ARENA_ITEMS {
            return None;
        }
        let id = BigintId(u32::try_from(graph.bigints.len()).expect("bounded bigints"));
        graph.bigints.push(
            Bigint {
                digest: hash::bigint(value),
                value: Arc::clone(value),
            },
            meter,
        );
        Some(id)
    }
    pub fn type_start(&mut self) -> usize {
        self.0.graph.types.len()
    }
    pub fn push_type(&mut self, ty: OwnedType) -> TypeId {
        let Storage { graph, meter, .. } = &mut *self.0;
        let id = TypeId(u32::try_from(graph.types.len()).expect("type arena exhausted"));
        graph.types.push(
            Type {
                leaf: hash::ty(&ty),
                ty,
            },
            meter,
        );
        id
    }
    pub fn type_range(&mut self, start: usize) -> Range<OwnedType> {
        Range::new(start, self.0.graph.types.len() - start)
    }
    /// A `uint8array`: its bytes, copied and hashed on the way, or only its
    /// length when the arena cannot index them all.
    pub fn bytes(&mut self, bytes: &[u8]) -> SnapshotObject {
        let Storage { graph, meter, .. } = &mut *self.0;
        if bytes.len() > ARENA_ITEMS.saturating_sub(graph.bytes.len()) {
            return SnapshotObject::Uint8ArrayTruncated {
                original_len: bytes.len(),
            };
        }
        let start = graph.bytes.len();
        graph.bytes.reserve(bytes.len(), meter);
        let mut digest = hash::Hasher::new(tags::HashDomain::Uint8Array);
        for chunk in bytes.chunks(btel_settings::snapshot::COPY_HASH_BATCH_BYTES) {
            digest.absorb(chunk);
            graph.bytes.extend_from_slice(chunk, meter);
        }
        SnapshotObject::Uint8Array {
            data: Uint8ArrayData {
                range: Range::new(start, bytes.len()),
                digest: digest.finish(),
            },
        }
    }
    /// A class or enum declaration. Its name is held beside the objects.
    pub fn declaration(
        &mut self,
        name: &DeclarationName,
        tag: TypeTag,
        is_enum: bool,
    ) -> SnapshotObject {
        let Storage { graph, meter, .. } = &mut *self.0;
        let id = NameId(u32::try_from(graph.names.len()).expect("a name per object"));
        graph.names.push(name.clone(), meter);
        SnapshotObject::Declaration {
            name: id,
            tag,
            is_enum,
        }
    }
    pub fn finish_value(self, value: SnapshotValue, shaper: &mut Shaper) -> Snapshot {
        self.finish(SnapshotRoot::Value(value), shaper)
    }
    /// Argument slots are written before processing deferred object children.
    pub fn finish_args(
        self,
        parameter_count: usize,
        slots: Range<SnapshotValue>,
        shaper: &mut Shaper,
    ) -> Snapshot {
        self.finish(
            SnapshotRoot::FunctionArgs(FunctionArgs {
                parameter_count,
                slots,
            }),
            shaper,
        )
    }
    /// Shape the capture into blobs; the snapshot is immutable afterwards.
    fn finish(mut self, root: SnapshotRoot, shaper: &mut Shaper) -> Snapshot {
        shaper.shape(&mut self.0, root);
        Snapshot::new(self.0)
    }
}

/// A `map<string, string>` snapshot built outside any VM, such as a
/// project's sources keyed by path. Entries are taken in order while their
/// keys and values total at most `max_bytes`; the rest are dropped, and the
/// map's recorded length says so. `None` when the pool has no slot.
pub fn string_map(
    pool: &SnapshotPool,
    entries: &[(String, String)],
    max_bytes: usize,
) -> Option<Snapshot> {
    let mut b = pool.try_acquire()?;
    let mut shaper = Shaper::default();
    let Some(map) = b.reserve_object() else {
        return Some(b.finish_value(SnapshotValue::Truncated(Limit::Objects), &mut shaper));
    };
    let key_type = b.push_type(OwnedType::string());
    let value_type = b.push_type(OwnedType::string());
    let start = b.entry_start();
    let count = entries.len().min(b.remaining_entries());
    b.reserve_entries(count);
    let mut room = max_bytes;
    for (key, value) in entries.iter().take(count) {
        let Some(left) = room.checked_sub(key.len().saturating_add(value.len())) else {
            break;
        };
        room = left;
        let value = match b.string(&BexStr::from(value.as_str())) {
            Some(id) => SnapshotValue::String(id),
            None => SnapshotValue::Truncated(Limit::Bytes),
        };
        b.entry(&BexStr::from(key.as_str()), value);
    }
    let entries_range = b.entry_range(start);
    b.set_object(
        map,
        SnapshotObject::Map {
            key_type,
            value_type,
            entries: entries_range,
            original_len: entries.len(),
        },
    );
    Some(b.finish_value(SnapshotValue::Object(map), &mut shaper))
}
