//! Global handle table for opaque `BexExternalValue` variants crossing the FFI boundary.

use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{
        Arc, LazyLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_project::{BexExternalAdt, BexExternalValue, Handle, MediaKind};

use crate::baml_bridge::cffi::BamlHandleType;

/// A host's reference to an engine object: a key the handle table minted.
/// Never 0, which the wire and every host reserve for "no handle".
///
/// A `TableKey` is not a host-value key (`bex_project::HostKey`) and cannot be
/// passed where one is expected; the one place a bare `u64` becomes either is
/// the wire decoder, which decides by the handle's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableKey(NonZeroU64);

impl TableKey {
    /// A key as the C ABI or the wire carries it; `None` for the reserved 0.
    pub const fn new(key: u64) -> Option<Self> {
        match NonZeroU64::new(key) {
            Some(key) => Some(Self(key)),
            None => None,
        }
    }

    /// The key as the C ABI and the wire carry it.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl std::fmt::Display for TableKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A typed, runtime-owned immutable callback/re-entry capability.
#[derive(Clone)]
pub struct InvocationStateHandle {
    pub owner: Arc<dyn bex_project::Bex>,
    pub state: bex_project::InheritedInvocationState,
}

impl std::fmt::Debug for InvocationStateHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvocationStateHandle")
            .finish_non_exhaustive()
    }
}

impl bex_project::BexRustData for InvocationStateHandle {
    fn measure(&self, _: &mut bex_project::Meter) {
        // The engine owns everything this names.
    }
}

#[derive(Clone)]
pub struct TraceReservationHandle {
    pub owner: Arc<dyn bex_project::Bex>,
    pub reservation: Arc<bex_project::ReservedSpanData>,
}

impl std::fmt::Debug for TraceReservationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraceReservationHandle")
            .field("reservation", &self.reservation)
            .finish_non_exhaustive()
    }
}

impl bex_project::BexRustData for TraceReservationHandle {
    fn measure(&self, meter: &mut bex_project::Meter) {
        meter.shared(&self.reservation, |meter| self.reservation.measure(meter));
    }
}

/// Subset of `BexExternalValue` that can be held as a handle.
/// Enforces at the type level that primitives/containers never enter the table.
///
/// Note: host-value keys (`HOST_VALUE_CALLABLE`, `HOST_VALUE_OPAQUE`) do NOT
/// live in this table: they are the engine's references to host objects,
/// typed `bex_project::HostKey` and indexed by `bex_external_types::host_value`
/// (the dual of this table: the engine holds keys + Arc-with-Drop; the bridge
/// holds the underlying host object).
#[derive(Clone, Debug)]
pub enum CffiHandleTableEntry {
    BexHeapHandle(Handle),
    Adt(BexExternalAdt),
    RustData(Arc<dyn bex_project::BexRustData>),
    InvocationState(InvocationStateHandle),
    TraceReservation(TraceReservationHandle),
}

pub struct CffiHandleTableOptions<'a> {
    pub(crate) table: &'a CffiHandleTable,
    pub(crate) serialize_media: bool,
    pub(crate) serialize_prompt_ast: bool,
}

impl CffiHandleTableOptions<'_> {
    pub fn for_wire() -> Self {
        Self {
            table: &HANDLE_TABLE,
            serialize_media: true,
            serialize_prompt_ast: true,
        }
    }

    pub fn for_in_process() -> Self {
        Self {
            table: &HANDLE_TABLE,
            serialize_media: false,
            serialize_prompt_ast: false,
        }
    }
}

impl CffiHandleTableEntry {
    /// The engine object this entry names, if it names one: a heap handle,
    /// bare or tagged with the engine's proxy kind. Such an entry carries the
    /// object's identity, so one object has one live key. A payload (media, a
    /// prompt, a type, runtime data) or an invocation capability is a value
    /// and carries none.
    fn identity(&self) -> Option<&Handle> {
        match self {
            Self::BexHeapHandle(handle)
            | Self::Adt(BexExternalAdt::TaggedHeapHandle {
                heap_handle: handle,
                ..
            }) => Some(handle),
            Self::Adt(
                BexExternalAdt::Type(_)
                | BexExternalAdt::WireType(_)
                | BexExternalAdt::PromptAst(_)
                | BexExternalAdt::Media(_),
            )
            | Self::RustData(_)
            | Self::InvocationState(_)
            | Self::TraceReservation(_) => None,
        }
    }

    /// Map this value to its proto `BamlHandleType` tag.
    pub fn handle_type(&self) -> BamlHandleType {
        match self {
            Self::BexHeapHandle(_) => BamlHandleType::UntaggedBexHeap,
            Self::RustData(_) => BamlHandleType::UntaggedRustData,
            Self::InvocationState(_) => BamlHandleType::InvocationState,
            Self::TraceReservation(_) => BamlHandleType::TraceReservation,
            Self::Adt(adt) => match adt {
                BexExternalAdt::Type(_) | BexExternalAdt::WireType(_) => BamlHandleType::AdtType,
                BexExternalAdt::PromptAst(_) => BamlHandleType::AdtPromptAst,
                BexExternalAdt::Media(media) => match media.kind {
                    MediaKind::Image => BamlHandleType::AdtMediaImage,
                    MediaKind::Audio => BamlHandleType::AdtMediaAudio,
                    MediaKind::Video => BamlHandleType::AdtMediaVideo,
                    MediaKind::Pdf => BamlHandleType::AdtMediaPdf,
                    MediaKind::Generic => BamlHandleType::AdtMediaGeneric,
                },
                BexExternalAdt::TaggedHeapHandle { kind, .. } => match kind {
                    bex_project::TaggedHeapHandleKind::Callable => BamlHandleType::FunctionRef,
                    bex_project::TaggedHeapHandleKind::Stream => {
                        BamlHandleType::AdtTaggedHeapHandle
                    }
                    bex_project::TaggedHeapHandleKind::FunctionSpec => {
                        BamlHandleType::AdtFunctionSpec
                    }
                    bex_project::TaggedHeapHandleKind::RuntimeValue => {
                        BamlHandleType::AdtRuntimeValue
                    }
                },
            },
        }
    }
}

impl TryFrom<BexExternalValue> for CffiHandleTableEntry {
    type Error = &'static str;

    fn try_from(value: BexExternalValue) -> Result<Self, Self::Error> {
        match value {
            BexExternalValue::Handle(h) => Ok(Self::BexHeapHandle(h)),
            BexExternalValue::Adt(a) => Ok(Self::Adt(a)),
            BexExternalValue::RustData(arc) => Ok(Self::RustData(arc)),
            // Host values use a separate per-bridge registry, not HANDLE_TABLE.
            BexExternalValue::HostValue(_)
            | BexExternalValue::HostCallable { .. }
            | BexExternalValue::Null
            | BexExternalValue::Int(_)
            | BexExternalValue::Bigint(_)
            | BexExternalValue::Float(_)
            | BexExternalValue::Bool(_)
            | BexExternalValue::String(_)
            | BexExternalValue::Array { .. }
            | BexExternalValue::Map { .. }
            | BexExternalValue::Instance { .. }
            | BexExternalValue::Variant { .. }
            | BexExternalValue::Union { .. }
            | BexExternalValue::Uint8Array(_) => {
                Err("only opaque BexExternalValue variants can be held as handles")
            }
        }
    }
}

impl From<CffiHandleTableEntry> for BexExternalValue {
    fn from(value: CffiHandleTableEntry) -> Self {
        match value {
            CffiHandleTableEntry::BexHeapHandle(h) => BexExternalValue::Handle(h),
            CffiHandleTableEntry::Adt(a) => BexExternalValue::Adt(a),
            CffiHandleTableEntry::RustData(arc) => BexExternalValue::RustData(arc),
            CffiHandleTableEntry::InvocationState(state) => {
                BexExternalValue::RustData(Arc::new(state))
            }
            CffiHandleTableEntry::TraceReservation(reservation) => {
                BexExternalValue::RustData(Arc::new(reservation))
            }
        }
    }
}

/// One table row: the value plus its outstanding ownership count.
///
/// Every crossing that hands the host a key — an outbound encode or an
/// explicit clone — owes exactly one release. A refcount (rather than one key
/// per crossing) is what lets the identity-bearing arm reuse a key without
/// breaking that contract: the Nth crossing bumps the count, the Nth release
/// balances it, and the row dies at zero.
///
/// LIFETIME DISCIPLINE IS THE BRIDGE'S, per language. The table sees keys,
/// not owners, so on a shared (identity-arm) key a double release is
/// indistinguishable from a co-owner's legitimate one: it silently takes that
/// owner's ownership, and the table cannot detect it. Each bridge therefore
/// guarantees exactly-once at its wrapper — trivially where release IS drop
/// (Rust), by tracking a released flag on the wrapper (never in this table)
/// where a host can reach a handle after releasing it. Tests observe the
/// balance through [`Self::refcount`].
struct CffiHandleTableRow {
    value: Arc<CffiHandleTableEntry>,
    refcount: u64,
}

/// Global handle table mapping [`TableKey`]s to `Arc<CffiHandleTableEntry>`.
/// Single instance shared by all bridges.
pub struct CffiHandleTable {
    next_key: AtomicU64,
    /// Lock order: `entries` before `heap_keys`, always.
    entries: RwLock<HashMap<TableKey, CffiHandleTableRow>>,
    /// Dedup index for the entries that name an engine object
    /// ([`CffiHandleTableEntry::identity`]): one live cffi key per heap
    /// [`Handle`] (`Eq` = slab key + issuing heap), so a host-side key compare
    /// answers object identity. Payload entries carry no identity and never
    /// dedup.
    heap_keys: RwLock<HashMap<Handle, TableKey>>,
}

impl CffiHandleTable {
    pub fn new() -> Self {
        Self {
            next_key: AtomicU64::new(1),
            entries: RwLock::new(HashMap::new()),
            heap_keys: RwLock::new(HashMap::new()),
        }
    }

    /// A fresh key. The counter starts at 1 and never hands out 0, even if it
    /// ever wrapped.
    fn mint(&self) -> TableKey {
        loop {
            if let Some(key) = TableKey::new(self.next_key.fetch_add(1, Ordering::Relaxed)) {
                return key;
            }
        }
    }

    /// Insert a value and return its key, adding one ownership.
    ///
    /// An entry naming an engine object that already has a live key returns
    /// that key (host-side `==` on keys is an identity compare); every other
    /// value gets a fresh key. Either way the caller now owes one release.
    pub fn insert(&self, value: CffiHandleTableEntry) -> TableKey {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = value.identity() {
            let mut heap_keys = self
                .heap_keys
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(&key) = heap_keys.get(handle) {
                let row = entries
                    .get_mut(&key)
                    .unwrap_or_else(|| unreachable!("heap_keys always names a live entry"));
                // An object crosses in one form, fixed by its kind and its
                // class's crossing, so the key's entry is this one.
                debug_assert_eq!(
                    row.value.handle_type(),
                    value.handle_type(),
                    "one engine object crossed in two forms"
                );
                row.refcount += 1;
                return key;
            }
            let key = self.mint();
            heap_keys.insert(handle.clone(), key);
            entries.insert(
                key,
                CffiHandleTableRow {
                    value: Arc::new(value),
                    refcount: 1,
                },
            );
            return key;
        }
        let key = self.mint();
        entries.insert(
            key,
            CffiHandleTableRow {
                value: Arc::new(value),
                refcount: 1,
            },
        );
        key
    }

    /// Clone a handle: adds one ownership and returns the key to release it
    /// through.
    ///
    /// An entry naming an engine object keeps its key (a copy is another owner
    /// of the same identity); identity-free values get a fresh key sharing the
    /// Arc, preserving the historical "each wrapper holds its own key" shape.
    pub fn clone_handle(&self, key: TableKey) -> Option<TableKey> {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let row = entries.get_mut(&key)?;
        if row.value.identity().is_some() {
            row.refcount += 1;
            return Some(key);
        }
        let value = row.value.clone();
        let new_key = self.mint();
        entries.insert(new_key, CffiHandleTableRow { value, refcount: 1 });
        Some(new_key)
    }

    /// Resolve a key to its value (cheap Arc clone). Not an ownership change.
    pub fn resolve(&self, key: TableKey) -> Option<Arc<CffiHandleTableEntry>> {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .map(|row| row.value.clone())
    }

    /// Release one ownership of a key. Returns true if the key was live.
    /// The row (and its dedup index entry) is removed when the last
    /// ownership is released.
    pub fn release(&self, key: TableKey) -> bool {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(row) = entries.get_mut(&key) else {
            return false;
        };
        row.refcount -= 1;
        if row.refcount == 0 {
            let row = entries
                .remove(&key)
                .unwrap_or_else(|| unreachable!("row was just read under the same write lock"));
            if let Some(handle) = row.value.identity() {
                self.heap_keys
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(handle);
            }
        }
        true
    }

    /// The outstanding ownership count of a live key — the number of releases
    /// it still owes. Test instrumentation for the leak audits, which cannot
    /// see an imbalance on a shared key through [`Self::len`] alone (rows,
    /// not owners). `None` for a dead or unknown key.
    pub fn refcount(&self, key: TableKey) -> Option<u64> {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .map(|row| row.refcount)
    }

    /// Return the number of live handle-table rows (a refcounted engine-heap
    /// row counts once however many owners it has).
    pub fn len(&self) -> usize {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Return whether the handle table currently has no live rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Atomically resolve and release one ownership — the inbound
    /// ownership-transfer lane (the host clones a key for the wire; decoding
    /// the wire consumes that clone). Returns None if the key was absent.
    pub fn drain(&self, key: TableKey) -> Option<Arc<CffiHandleTableEntry>> {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let row = entries.get_mut(&key)?;
        let value = row.value.clone();
        row.refcount -= 1;
        if row.refcount == 0 {
            entries.remove(&key);
            if let Some(handle) = value.identity() {
                self.heap_keys
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(handle);
            }
        }
        Some(value)
    }
}

impl Default for CffiHandleTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global static handle table instance.
pub static HANDLE_TABLE: LazyLock<CffiHandleTable> = LazyLock::new(CffiHandleTable::new);

#[cfg(test)]
mod tests {
    use bex_project::{BexExternalValue, HostValueArc, HostValueKind, MediaValue};

    use super::*;
    use crate::{baml_bridge::cffi::baml_outbound_value::Value as BamlValueVariant, value_encode};

    /// Any engine row a test can mint without a heap: a media value.
    fn media_row() -> CffiHandleTableEntry {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(MediaValue::from_url(
            MediaKind::Generic,
            "https://example.com/",
            None,
        )))
    }

    #[test]
    fn insert_and_resolve() {
        let table = CffiHandleTable::new();
        let key = table.insert(media_row());
        let resolved = table.resolve(key).unwrap();
        assert!(matches!(
            &*resolved,
            CffiHandleTableEntry::Adt(BexExternalAdt::Media(_))
        ));
    }

    #[test]
    fn resolve_missing_returns_none() {
        let table = CffiHandleTable::new();
        assert!(table.resolve(TableKey::new(9999).unwrap()).is_none());
    }

    #[test]
    fn clone_handle_produces_new_key() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(media_row());
        let key2 = table.clone_handle(key1).unwrap();
        assert_ne!(key1, key2);
        // Both resolve to something
        assert!(table.resolve(key1).is_some());
        assert!(table.resolve(key2).is_some());
    }

    #[test]
    fn clone_handle_shares_same_arc() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(media_row());
        let key2 = table.clone_handle(key1).unwrap();
        let arc1 = table.resolve(key1).unwrap();
        let arc2 = table.resolve(key2).unwrap();
        // Same underlying allocation
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }

    #[test]
    fn release_original_clone_still_resolves() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(media_row());
        let key2 = table.clone_handle(key1).unwrap();
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_none());
        assert!(table.resolve(key2).is_some());
    }

    #[test]
    fn release_both_clones() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(media_row());
        let key2 = table.clone_handle(key1).unwrap();
        assert!(table.release(key1));
        assert!(table.release(key2));
        assert!(table.resolve(key1).is_none());
        assert!(table.resolve(key2).is_none());
    }

    #[test]
    fn double_release_returns_false() {
        let table = CffiHandleTable::new();
        let key = table.insert(media_row());
        assert!(table.release(key));
        assert!(!table.release(key)); // second release returns false
    }

    #[test]
    fn drain_removes_entry() {
        let table = CffiHandleTable::new();
        let key = table.insert(media_row());
        let drained = table.drain(key).expect("drain should return entry");
        assert!(matches!(
            &*drained,
            CffiHandleTableEntry::Adt(BexExternalAdt::Media(_))
        ));
        assert!(table.resolve(key).is_none());
        assert!(table.drain(key).is_none());
    }

    #[test]
    fn try_from_rejects_primitives() {
        assert!(CffiHandleTableEntry::try_from(BexExternalValue::Null).is_err());
        assert!(CffiHandleTableEntry::try_from(BexExternalValue::Int(1)).is_err());
        assert!(CffiHandleTableEntry::try_from(BexExternalValue::String("hi".into())).is_err());
        assert!(CffiHandleTableEntry::try_from(BexExternalValue::Bool(true)).is_err());
        assert!(CffiHandleTableEntry::try_from(BexExternalValue::Float(1.0)).is_err());
    }

    #[test]
    fn roundtrip_to_bex_external_value() {
        let bex: BexExternalValue = media_row().into();
        let back = CffiHandleTableEntry::try_from(bex).unwrap();
        assert!(matches!(
            back,
            CffiHandleTableEntry::Adt(BexExternalAdt::Media(_))
        ));
    }

    #[test]
    fn key_starts_at_one() {
        let table = CffiHandleTable::new();
        let key = table.insert(media_row());
        assert_eq!(
            key.get(),
            1,
            "first key should be 1 (0 is reserved as invalid)"
        );
    }

    #[test]
    fn keys_are_monotonically_increasing() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(media_row());
        let key2 = table.insert(media_row());
        let key3 = table.insert(media_row());
        assert!(key1 < key2);
        assert!(key2 < key3);
    }

    #[test]
    fn artifact_safe_encoding_does_not_insert_or_serialize_handle_table_key() {
        let value = BexExternalValue::HostValue(HostValueArc::new(
            bex_project::HostKey::new(42).unwrap(),
            HostValueKind::Callable,
        ));
        let encoded = value_encode::artifact_safe_external_to_outbound(&value).unwrap();

        assert!(!matches!(
            encoded.value,
            Some(BamlValueVariant::HandleValue(_))
        ));
    }

    /// A no-op heap for constructing real `Handle`s in-table without a VM.
    struct StubHeap;

    impl bex_project::WeakHeapRef for StubHeap {
        fn release_handle(&self, _slab_key: usize) {}

        fn resolve_handle_ptr(&self, _slab_key: usize) -> Option<bex_project::HeapPtr> {
            None
        }
    }

    fn stub_heap() -> Arc<dyn bex_project::WeakHeapRef> {
        Arc::new(StubHeap)
    }

    #[test]
    fn same_heap_handle_dedups_to_one_refcounted_key() {
        let table = CffiHandleTable::new();
        let heap = stub_heap();
        let handle = bex_project::Handle::new(7, heap);

        let key1 = table.insert(CffiHandleTableEntry::BexHeapHandle(handle.clone()));
        let key2 = table.insert(CffiHandleTableEntry::BexHeapHandle(handle.clone()));
        assert_eq!(key1, key2, "one object, one key");
        assert_eq!(table.len(), 1);

        // Two crossings owe two releases; the row survives the first.
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_some());
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_none());

        // The dedup index is cleaned at zero: a later crossing mints fresh.
        let key3 = table.insert(CffiHandleTableEntry::BexHeapHandle(handle));
        assert_ne!(key3, key1);
        assert!(table.release(key3));
    }

    #[test]
    fn distinct_objects_and_heaps_get_distinct_keys() {
        let table = CffiHandleTable::new();
        let heap = stub_heap();
        let key_a = table.insert(CffiHandleTableEntry::BexHeapHandle(
            bex_project::Handle::new(1, heap.clone()),
        ));
        let key_b = table.insert(CffiHandleTableEntry::BexHeapHandle(
            bex_project::Handle::new(2, heap),
        ));
        // Same slab key issued by a different heap is a different object.
        let key_c = table.insert(CffiHandleTableEntry::BexHeapHandle(
            bex_project::Handle::new(1, stub_heap()),
        ));
        assert_ne!(key_a, key_b);
        assert_ne!(key_a, key_c);
        assert_eq!(table.len(), 3);
        for key in [key_a, key_b, key_c] {
            assert!(table.release(key));
        }
    }

    #[test]
    fn clone_handle_identity_arm_keeps_its_key() {
        let table = CffiHandleTable::new();
        let handle = bex_project::Handle::new(7, stub_heap());
        let key = table.insert(CffiHandleTableEntry::BexHeapHandle(handle));
        assert_eq!(table.clone_handle(key), Some(key));
        assert!(table.release(key));
        assert!(table.resolve(key).is_some(), "clone's ownership keeps it");
        assert!(table.release(key));
        assert!(table.resolve(key).is_none());
    }

    #[test]
    fn identity_free_entries_never_dedup() {
        let table = CffiHandleTable::new();
        let rust_data: Arc<dyn bex_project::BexRustData> = Arc::new(bex_project::TestRustData(42));
        let key1 = table.insert(CffiHandleTableEntry::RustData(rust_data.clone()));
        let key2 = table.insert(CffiHandleTableEntry::RustData(rust_data));
        assert_ne!(key1, key2, "RustData carries no identity");
        let key3 = table.insert(media_row());
        let key4 = table.insert(media_row());
        assert_ne!(key3, key4, "a media payload carries no identity");
        for key in [key1, key2, key3, key4] {
            assert!(table.release(key));
        }
    }

    /// A tagged heap handle names an engine object as a bare one does: one
    /// object, one refcounted key, and a clone keeps it.
    #[test]
    fn tagged_heap_handle_dedups_to_one_refcounted_key() {
        let table = CffiHandleTable::new();
        let handle = bex_project::Handle::new(7, stub_heap());
        let tagged = |h: &bex_project::Handle| {
            CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::TaggedHeapHandle {
                kind: bex_project::TaggedHeapHandleKind::RuntimeValue,
                ty: bex_project::AnnotationTy::int(),
                heap_handle: h.clone(),
            })
        };
        let key1 = table.insert(tagged(&handle));
        let key2 = table.insert(tagged(&handle));
        assert_eq!(key1, key2, "one object, one key");
        assert_eq!(table.len(), 1);
        assert_eq!(table.clone_handle(key1), Some(key1));
        let other = table.insert(tagged(&bex_project::Handle::new(8, stub_heap())));
        assert_ne!(other, key1, "another object has its own key");

        // Three owners of the shared key; the row lives until the last goes.
        assert!(table.release(key1));
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_some());
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_none());
        // The dedup index is cleaned at zero: a later crossing mints fresh.
        let key3 = table.insert(tagged(&handle));
        assert_ne!(key3, key1);
        for key in [key3, other] {
            assert!(table.release(key));
        }
    }

    #[test]
    fn drain_consumes_one_ownership() {
        let table = CffiHandleTable::new();
        let handle = bex_project::Handle::new(7, stub_heap());
        let key = table.insert(CffiHandleTableEntry::BexHeapHandle(handle.clone()));
        assert_eq!(
            table.insert(CffiHandleTableEntry::BexHeapHandle(handle)),
            key
        );

        assert!(table.drain(key).is_some());
        assert!(table.resolve(key).is_some(), "one ownership remains");
        assert!(table.drain(key).is_some());
        assert!(table.resolve(key).is_none());
        assert!(table.drain(key).is_none());
    }
}
