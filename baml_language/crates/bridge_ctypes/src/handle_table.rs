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
                BexExternalAdt::TaggedHeapHandle {
                    ty: bex_project::RuntimeTy::Function { .. },
                    ..
                } => BamlHandleType::FunctionRef,
                BexExternalAdt::TaggedHeapHandle { .. } => BamlHandleType::AdtTaggedHeapHandle,
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

/// Global handle table mapping opaque u64 keys to `Arc<CffiHandleTableEntry>`.
/// Single instance shared by all bridges.
pub struct CffiHandleTable {
    next_key: AtomicU64,
    entries: RwLock<HashMap<u64, Arc<CffiHandleTableEntry>>>,
}

impl CffiHandleTable {
    pub fn new() -> Self {
        Self {
            next_key: AtomicU64::new(1), // start at 1; 0 = invalid
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a value and return its unique key.
    pub fn insert(&self, value: CffiHandleTableEntry) -> u64 {
        let key = self.next_key.fetch_add(1, Ordering::Relaxed);
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.insert(key, Arc::new(value));
        key
    }

    /// Clone a handle: creates a new key pointing to the same Arc.
    pub fn clone_handle(&self, key: u64) -> Option<u64> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let arc = entries.get(&key)?.clone();
        drop(entries);
        let new_key = self.next_key.fetch_add(1, Ordering::Relaxed);
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(new_key, arc);
        Some(new_key)
    }

    /// Resolve a key to its value (cheap Arc clone).
    pub fn resolve(&self, key: u64) -> Option<Arc<CffiHandleTableEntry>> {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned()
    }

    /// Release a handle. Returns true if the key was present.
    pub fn release(&self, key: u64) -> bool {
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&key)
            .is_some()
    }

    /// Return the number of currently owned handle-table keys.
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

    /// Atomically resolve and remove. Returns the entry or None if the
    /// key was already absent.
    pub fn drain(&self, key: u64) -> Option<Arc<CffiHandleTableEntry>> {
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&key)
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
}
