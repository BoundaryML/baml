//! Global handle table for opaque `BexExternalValue` variants crossing the FFI boundary.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_project::{BexExternalAdt, BexExternalValue, Handle, MediaKind};

use crate::baml_bridge::cffi::BamlHandleType;

/// Newtype wrapper around opaque `$rust_type` objects
/// (`Arc<dyn Any + Send + Sync>`) stored as a handle.
#[derive(Clone)]
pub struct BexRustData(pub Arc<dyn std::any::Any + Send + Sync>);

impl std::fmt::Debug for BexRustData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BexRustData").finish()
    }
}

/// Subset of `BexExternalValue` that can be held as a handle.
/// Enforces at the type level that primitives/containers never enter the table.
///
/// Note: `HOST_VALUE_CALLABLE` keys do NOT live in this table. They are
/// bridge-side identifiers backed by a per-bridge `HostValueRegistry`.
/// See `bex_external_types::host_value` for the inverted-direction
/// lifetime model (Rust holds keys + Arc-with-Drop; bridge holds the
/// underlying host object).
#[derive(Clone, Debug)]
pub enum CffiHandleTableEntry {
    BexHeapHandle(Handle),
    FunctionRef { global_index: usize },
    Adt(BexExternalAdt),
    RustData(BexRustData),
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
    /// Map this value to its proto `BamlHandleType` tag.
    pub fn handle_type(&self) -> BamlHandleType {
        match self {
            Self::BexHeapHandle(_) => BamlHandleType::UntaggedBexHeap,
            Self::FunctionRef { .. } => BamlHandleType::FunctionRef,
            Self::RustData(_) => BamlHandleType::UntaggedRustData,
            Self::Adt(adt) => match adt {
                BexExternalAdt::Collector(_) => BamlHandleType::AdtCollector,
                BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_) => BamlHandleType::AdtType,
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
            BexExternalValue::FunctionRef { global_index } => {
                Ok(Self::FunctionRef { global_index })
            }
            BexExternalValue::Adt(a) => Ok(Self::Adt(a)),
            BexExternalValue::RustData(arc) => Ok(Self::RustData(BexRustData(arc))),
            // HostValue uses a separate per-bridge registry, not HANDLE_TABLE.
            BexExternalValue::HostValue(_)
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
            CffiHandleTableEntry::FunctionRef { global_index } => {
                BexExternalValue::FunctionRef { global_index }
            }
            CffiHandleTableEntry::Adt(a) => BexExternalValue::Adt(a),
            CffiHandleTableEntry::RustData(BexRustData(arc)) => BexExternalValue::RustData(arc),
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
struct CffiHandleTableRow {
    value: Arc<CffiHandleTableEntry>,
    refcount: u64,
}

/// Global handle table mapping opaque u64 keys to `Arc<CffiHandleTableEntry>`.
/// Single instance shared by all bridges.
pub struct CffiHandleTable {
    next_key: AtomicU64,
    /// Lock order: `entries` before `heap_keys`, always.
    entries: RwLock<HashMap<u64, CffiHandleTableRow>>,
    /// Dedup index for the identity-bearing arm: one live cffi key per heap
    /// [`Handle`] (`Eq` = slab key + issuing heap), so a host-side key compare
    /// answers object identity. `RustData`/`Adt` entries carry no identity and
    /// never dedup.
    heap_keys: RwLock<HashMap<Handle, u64>>,
}

impl CffiHandleTable {
    pub fn new() -> Self {
        Self {
            next_key: AtomicU64::new(1), // start at 1; 0 = invalid
            entries: RwLock::new(HashMap::new()),
            heap_keys: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a value and return its key, adding one ownership.
    ///
    /// A `BexHeapHandle` naming an object that already has a live key returns
    /// that key (host-side `==` on keys is an identity compare); every other
    /// value gets a fresh key. Either way the caller now owes one release.
    pub fn insert(&self, value: CffiHandleTableEntry) -> u64 {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let CffiHandleTableEntry::BexHeapHandle(handle) = &value {
            let mut heap_keys = self
                .heap_keys
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(&key) = heap_keys.get(handle) {
                entries
                    .get_mut(&key)
                    .unwrap_or_else(|| unreachable!("heap_keys always names a live entry"))
                    .refcount += 1;
                return key;
            }
            let key = self.next_key.fetch_add(1, Ordering::Relaxed);
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
        let key = self.next_key.fetch_add(1, Ordering::Relaxed);
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
    /// The identity-bearing arm keeps its key (a copy is another owner of the
    /// same identity); identity-free values get a fresh key sharing the Arc,
    /// preserving the historical "each wrapper holds its own key" shape.
    pub fn clone_handle(&self, key: u64) -> Option<u64> {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let row = entries.get_mut(&key)?;
        if matches!(&*row.value, CffiHandleTableEntry::BexHeapHandle(_)) {
            row.refcount += 1;
            return Some(key);
        }
        let value = row.value.clone();
        let new_key = self.next_key.fetch_add(1, Ordering::Relaxed);
        entries.insert(new_key, CffiHandleTableRow { value, refcount: 1 });
        Some(new_key)
    }

    /// Resolve a key to its value (cheap Arc clone). Not an ownership change.
    pub fn resolve(&self, key: u64) -> Option<Arc<CffiHandleTableEntry>> {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .map(|row| row.value.clone())
    }

    /// Release one ownership of a key. Returns true if the key was live.
    /// The row (and its dedup index entry) is removed when the last
    /// ownership is released.
    pub fn release(&self, key: u64) -> bool {
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
            if let CffiHandleTableEntry::BexHeapHandle(handle) = &*row.value {
                self.heap_keys
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(handle);
            }
        }
        true
    }

    /// Return the number of currently live handle-table keys.
    pub fn len(&self) -> usize {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Return whether the handle table currently owns no keys.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Atomically resolve and release one ownership — the inbound
    /// ownership-transfer lane (the host clones a key for the wire; decoding
    /// the wire consumes that clone). Returns None if the key was absent.
    pub fn drain(&self, key: u64) -> Option<Arc<CffiHandleTableEntry>> {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let row = entries.get_mut(&key)?;
        let value = row.value.clone();
        row.refcount -= 1;
        if row.refcount == 0 {
            entries.remove(&key);
            if let CffiHandleTableEntry::BexHeapHandle(handle) = &*value {
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
    use bex_project::{BexExternalValue, HostValueArc, HostValueKind};

    use super::*;
    use crate::{baml_bridge::cffi::baml_outbound_value::Value as BamlValueVariant, value_encode};

    fn make_function_ref() -> CffiHandleTableEntry {
        CffiHandleTableEntry::FunctionRef { global_index: 42 }
    }

    #[test]
    fn insert_and_resolve() {
        let table = CffiHandleTable::new();
        let key = table.insert(make_function_ref());
        let resolved = table.resolve(key).unwrap();
        assert!(matches!(
            &*resolved,
            CffiHandleTableEntry::FunctionRef { global_index: 42 }
        ));
    }

    #[test]
    fn resolve_missing_returns_none() {
        let table = CffiHandleTable::new();
        assert!(table.resolve(9999).is_none());
    }

    #[test]
    fn clone_handle_produces_new_key() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        assert_ne!(key1, key2);
        // Both resolve to something
        assert!(table.resolve(key1).is_some());
        assert!(table.resolve(key2).is_some());
    }

    #[test]
    fn clone_handle_shares_same_arc() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        let arc1 = table.resolve(key1).unwrap();
        let arc2 = table.resolve(key2).unwrap();
        // Same underlying allocation
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }

    #[test]
    fn release_original_clone_still_resolves() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_none());
        assert!(table.resolve(key2).is_some());
    }

    #[test]
    fn release_both_clones() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        assert!(table.release(key1));
        assert!(table.release(key2));
        assert!(table.resolve(key1).is_none());
        assert!(table.resolve(key2).is_none());
    }

    #[test]
    fn double_release_returns_false() {
        let table = CffiHandleTable::new();
        let key = table.insert(make_function_ref());
        assert!(table.release(key));
        assert!(!table.release(key)); // second release returns false
    }

    #[test]
    fn drain_removes_entry() {
        let table = CffiHandleTable::new();
        let key = table.insert(make_function_ref());
        let drained = table.drain(key).expect("drain should return entry");
        assert!(matches!(
            &*drained,
            CffiHandleTableEntry::FunctionRef { global_index: 42 }
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
    fn try_from_accepts_function_ref() {
        let val = BexExternalValue::FunctionRef { global_index: 7 };
        let htv = CffiHandleTableEntry::try_from(val).unwrap();
        assert!(matches!(
            htv,
            CffiHandleTableEntry::FunctionRef { global_index: 7 }
        ));
    }

    #[test]
    fn handle_type_function_ref() {
        let htv = CffiHandleTableEntry::FunctionRef { global_index: 0 };
        assert_eq!(htv.handle_type() as i32, BamlHandleType::FunctionRef as i32);
    }

    #[test]
    fn roundtrip_to_bex_external_value() {
        let original = CffiHandleTableEntry::FunctionRef { global_index: 99 };
        let bex: BexExternalValue = original.into();
        let back = CffiHandleTableEntry::try_from(bex).unwrap();
        assert!(matches!(
            back,
            CffiHandleTableEntry::FunctionRef { global_index: 99 }
        ));
    }

    #[test]
    fn key_starts_at_one() {
        let table = CffiHandleTable::new();
        let key = table.insert(make_function_ref());
        assert_eq!(key, 1, "first key should be 1 (0 is reserved as invalid)");
    }

    #[test]
    fn keys_are_monotonically_increasing() {
        let table = CffiHandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.insert(make_function_ref());
        let key3 = table.insert(make_function_ref());
        assert!(key1 < key2);
        assert!(key2 < key3);
    }

    #[test]
    fn artifact_safe_encoding_does_not_insert_or_serialize_handle_table_key() {
        let value = BexExternalValue::HostValue(HostValueArc::new(42, HostValueKind::Callable));
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
        let rust_data = BexRustData(Arc::new(42_u32));
        let key1 = table.insert(CffiHandleTableEntry::RustData(rust_data.clone()));
        let key2 = table.insert(CffiHandleTableEntry::RustData(rust_data));
        assert_ne!(key1, key2, "RustData carries no identity");

        // An ADT holding a heap handle is still identity-free at this layer.
        let handle = bex_project::Handle::new(7, stub_heap());
        let adt = |h: &bex_project::Handle| {
            CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::TaggedHeapHandle {
                ty: bex_project::RuntimeTy::int(),
                heap_handle: h.clone(),
            })
        };
        let key3 = table.insert(adt(&handle));
        let key4 = table.insert(adt(&handle));
        assert_ne!(key3, key4, "Adt entries never dedup");
        for key in [key1, key2, key3, key4] {
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
