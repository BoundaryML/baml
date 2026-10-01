//! A finished capture: its data and the blobs it was shaped into, immutable.
use std::sync::Arc;

use baml_type::DeclarationName;
use bex_str::BexStr;
use num_bigint::BigInt;

use crate::{
    CasId,
    graph::{
        BigintId, MapEntry, NameId, ObjectId, OwnedType, Range, SnapshotObject, SnapshotRoot,
        SnapshotValue, StringId, TypeId, Uint8ArrayData,
    },
    pool::Lease,
    shape::{BlobEntry, BlobIndex},
};

/// Exclusive pointer-sized owner, frozen before publication.
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<btel_snapshot::Snapshot>();
/// ```
pub struct Snapshot(pub(crate) Lease);
const _: () = assert!(std::mem::size_of::<Snapshot>() == std::mem::size_of::<usize>());
const _: () = assert!(std::mem::size_of::<Option<Snapshot>>() == std::mem::size_of::<usize>());
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let graph = &self.0.graph;
        f.debug_struct("Snapshot")
            .field("root", &self.root())
            .field("values", &&*graph.values)
            .field("objects", &&*graph.objects)
            .field("entries", &&*graph.entries)
            .field("bytes", &&*graph.bytes)
            .field("strings", &&*graph.strings)
            .field("bigints", &&*graph.bigints)
            .field("types", &&*graph.types)
            .field("names", &&*graph.names)
            .finish()
    }
}

/// What a capture's content says about how it was captured.
#[cfg(any(test, feature = "stats"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureStats {
    /// Leaf bytes copied into the capture: `uint8array` content and strings
    /// short enough to live in their handle.
    pub copied_bytes: usize,
    /// Leaf bytes held by handle: every other string, and bigints.
    pub shared_bytes: usize,
    /// Whether a limit cut any part of the capture.
    pub limited: bool,
}

/// One content-addressed blob of a shaped snapshot.
#[derive(Clone, Copy)]
pub struct Blob<'s> {
    pub(crate) snapshot: &'s Snapshot,
    index: BlobIndex,
}
impl std::fmt::Debug for Blob<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob").field("id", &self.id()).finish()
    }
}
impl<'s> Blob<'s> {
    pub fn id(&self) -> CasId {
        self.entry().id
    }
    pub fn index(&self) -> BlobIndex {
        self.index
    }
    /// Exact length of what [`Self::write`] writes.
    pub fn encoded_len(&self) -> u64 {
        self.entry().encoded_len
    }
    /// The blobs this one names, in the order of its child table.
    pub fn children(&self) -> impl ExactSizeIterator<Item = Blob<'s>> + use<'s> {
        let snapshot = self.snapshot;
        snapshot.0.shape.children[self.entry().children.indexes()]
            .iter()
            .map(move |index| Blob {
                snapshot,
                index: *index,
            })
    }
    pub(crate) fn entry(&self) -> &'s BlobEntry {
        &self.snapshot.0.shape.blobs[self.index.0 as usize]
    }
}
impl Snapshot {
    pub(crate) fn new(lease: Lease) -> Self {
        debug_assert!(!lease.shape.blobs.is_empty(), "a snapshot is shaped");
        Self(lease)
    }
    /// The blob a recording references for this capture.
    pub fn root_blob(&self) -> Blob<'_> {
        let last = self
            .0
            .shape
            .blobs
            .len()
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("a finished snapshot has a root blob"));
        Blob {
            snapshot: self,
            index: BlobIndex(u32::try_from(last).expect("bounded blob count")),
        }
    }
    pub fn root_id(&self) -> CasId {
        self.root_blob().id()
    }
    /// The blob at `index`, which must come from this snapshot.
    pub fn blob(&self, index: BlobIndex) -> Blob<'_> {
        assert!(
            (index.0 as usize) < self.0.shape.blobs.len(),
            "blob index from another snapshot"
        );
        Blob {
            snapshot: self,
            index,
        }
    }
    /// Every blob, each after the blobs it references; the root is last.
    pub fn blobs(&self) -> impl ExactSizeIterator<Item = Blob<'_>> + '_ {
        (0..self.0.shape.blobs.len()).map(|index| Blob {
            snapshot: self,
            index: BlobIndex(u32::try_from(index).expect("bounded blob count")),
        })
    }
    pub fn root(&self) -> SnapshotRoot {
        self.root_blob().entry().root
    }
    pub fn value(&self) -> Option<SnapshotValue> {
        match self.root() {
            SnapshotRoot::Value(value) => Some(value),
            SnapshotRoot::FunctionArgs(_) => None,
        }
    }
    pub fn arguments(&self) -> Option<&[SnapshotValue]> {
        match self.root() {
            SnapshotRoot::FunctionArgs(args) => Some(self.values(args.slots)),
            SnapshotRoot::Value(_) => None,
        }
    }
    pub fn roots(&self) -> &[SnapshotValue] {
        match &self.root_blob().entry().root {
            SnapshotRoot::Value(value) => std::slice::from_ref(value),
            SnapshotRoot::FunctionArgs(args) => self.values(args.slots),
        }
    }
    pub fn object(&self, id: ObjectId) -> &SnapshotObject {
        &self.0.graph.objects[id.0 as usize]
    }
    pub fn values(&self, range: Range<SnapshotValue>) -> &[SnapshotValue] {
        &self.0.graph.values[range.indexes()]
    }
    pub fn entries(&self, range: Range<MapEntry>) -> &[MapEntry] {
        &self.0.graph.entries[range.indexes()]
    }
    pub fn string(&self, id: StringId) -> &BexStr {
        &self.0.graph.strings[id.0 as usize]
    }
    pub fn bigint(&self, id: BigintId) -> &Arc<BigInt> {
        &self.0.graph.bigints[id.0 as usize].value
    }
    pub fn bytes(&self, data: Uint8ArrayData) -> &[u8] {
        &self.0.graph.bytes[data.range.indexes()]
    }
    /// A declaration's name.
    pub fn name(&self, id: NameId) -> &DeclarationName {
        &self.0.graph.names[id.0 as usize]
    }
    pub fn ty(&self, id: TypeId) -> &OwnedType {
        &self.0.graph.types[id.0 as usize].ty
    }
    pub fn type_arguments(
        &self,
        range: Range<OwnedType>,
    ) -> impl ExactSizeIterator<Item = &OwnedType> + '_ {
        self.0.graph.types[range.indexes()].iter().map(|ty| &ty.ty)
    }
    pub fn object_count(&self) -> usize {
        self.0.graph.objects.len()
    }
    /// What the capture's content says about how it was captured.
    #[cfg(any(test, feature = "stats"))]
    pub fn stats(&self) -> CaptureStats {
        let graph = &self.0.graph;
        let mut stats = CaptureStats {
            copied_bytes: graph.bytes.len(),
            shared_bytes: 0,
            limited: false,
        };
        for text in graph.strings.iter() {
            match text {
                BexStr::Inline { .. } => stats.copied_bytes += text.len(),
                BexStr::Flat(_) | BexStr::Slice { .. } | BexStr::Concat(_) => {
                    stats.shared_bytes += text.len();
                }
            }
        }
        for bigint in graph.bigints.iter() {
            stats.shared_bytes +=
                usize::try_from(bigint.value.bits().div_ceil(8)).unwrap_or(usize::MAX);
        }
        let cut = |value: &SnapshotValue| matches!(value, SnapshotValue::Truncated(_));
        stats.limited = self.roots().iter().any(cut)
            || graph.values.iter().any(cut)
            || graph.entries.iter().any(|entry| cut(&entry.value))
            || graph.objects.iter().any(|object| {
                object.is_cut() || matches!(object, SnapshotObject::Cell(value) if cut(value))
            });
        stats
    }
}
