//! Global handle table for opaque `BexExternalValue` variants crossing the FFI boundary.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_project::{BexExternalAdt, BexExternalValue, Handle, MediaKind};
use bex_resource_types::{ResourceHandle, ResourceType};

use crate::baml::cffi::BamlHandleType;

/// Subset of `BexExternalValue` that can be held as a handle.
/// Enforces at the type level that primitives/containers never enter the table.
#[derive(Clone, Debug)]
pub enum HandleTableValue {
    Handle(Handle),
    Resource(ResourceHandle),
    FunctionRef { global_index: usize },
    Adt(BexExternalAdt),
}

pub struct HandleTableOptions<'a> {
    pub(crate) table: &'a HandleTable,
    pub(crate) serialize_media: bool,
    pub(crate) serialize_prompt_ast: bool,
}

impl HandleTableOptions<'_> {
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

impl HandleTableValue {
    /// Map this value to its proto `BamlHandleType` tag.
    pub fn handle_type(&self) -> BamlHandleType {
        match self {
            Self::Handle(_) => BamlHandleType::HandleUnknown,
            Self::Resource(r) => match r.kind() {
                ResourceType::File => BamlHandleType::ResourceFile,
                ResourceType::Socket => BamlHandleType::ResourceSocket,
                ResourceType::Response => BamlHandleType::ResourceHttpResponse,
            },
            Self::FunctionRef { .. } => BamlHandleType::FunctionRef,
            Self::Adt(adt) => match adt {
                BexExternalAdt::Media(m) => match m.kind {
                    MediaKind::Image => BamlHandleType::AdtMediaImage,
                    MediaKind::Audio => BamlHandleType::AdtMediaAudio,
                    MediaKind::Video => BamlHandleType::AdtMediaVideo,
                    MediaKind::Pdf => BamlHandleType::AdtMediaPdf,
                    MediaKind::Generic => BamlHandleType::AdtMediaGeneric,
                },
                BexExternalAdt::PromptAst(_) => BamlHandleType::AdtPromptAst,
                BexExternalAdt::Collector(_) => BamlHandleType::AdtCollector,
                BexExternalAdt::Type(_) => BamlHandleType::AdtType,
            },
        }
    }
}

impl TryFrom<BexExternalValue> for HandleTableValue {
    type Error = &'static str;

    fn try_from(value: BexExternalValue) -> Result<Self, Self::Error> {
        match value {
            BexExternalValue::Handle(h) => Ok(Self::Handle(h)),
            BexExternalValue::Resource(r) => Ok(Self::Resource(r)),
            BexExternalValue::FunctionRef { global_index } => {
                Ok(Self::FunctionRef { global_index })
            }
            BexExternalValue::Adt(a) => Ok(Self::Adt(a)),
            BexExternalValue::Null
            | BexExternalValue::Int(_)
            | BexExternalValue::Float(_)
            | BexExternalValue::Bool(_)
            | BexExternalValue::String(_)
            | BexExternalValue::Array { .. }
            | BexExternalValue::Map { .. }
            | BexExternalValue::Instance { .. }
            | BexExternalValue::Variant { .. }
            | BexExternalValue::Union { .. } => {
                Err("only opaque BexExternalValue variants can be held as handles")
            }
        }
    }
}

impl From<HandleTableValue> for BexExternalValue {
    fn from(value: HandleTableValue) -> Self {
        match value {
            HandleTableValue::Handle(h) => BexExternalValue::Handle(h),
            HandleTableValue::Resource(r) => BexExternalValue::Resource(r),
            HandleTableValue::FunctionRef { global_index } => {
                BexExternalValue::FunctionRef { global_index }
            }
            HandleTableValue::Adt(a) => BexExternalValue::Adt(a),
        }
    }
}

/// Global handle table mapping opaque u64 keys to `Arc<HandleTableValue>`.
/// Single instance shared by all bridges.
pub struct HandleTable {
    next_key: AtomicU64,
    entries: RwLock<HashMap<u64, Arc<HandleTableValue>>>,
}

impl HandleTable {
    pub fn new() -> Self {
        Self {
            next_key: AtomicU64::new(1), // start at 1; 0 = invalid
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a value and return its unique key.
    pub fn insert(&self, value: HandleTableValue) -> u64 {
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
    pub fn resolve(&self, key: u64) -> Option<Arc<HandleTableValue>> {
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
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global static handle table instance.
pub static HANDLE_TABLE: LazyLock<HandleTable> = LazyLock::new(HandleTable::new);

#[cfg(test)]
mod tests {
    use bex_project::BexExternalValue;

    use super::*;

    fn make_function_ref() -> HandleTableValue {
        HandleTableValue::FunctionRef { global_index: 42 }
    }

    #[test]
    fn insert_and_resolve() {
        let table = HandleTable::new();
        let key = table.insert(make_function_ref());
        let resolved = table.resolve(key).unwrap();
        assert!(matches!(
            &*resolved,
            HandleTableValue::FunctionRef { global_index: 42 }
        ));
    }

    #[test]
    fn resolve_missing_returns_none() {
        let table = HandleTable::new();
        assert!(table.resolve(9999).is_none());
    }

    #[test]
    fn clone_handle_produces_new_key() {
        let table = HandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        assert_ne!(key1, key2);
        // Both resolve to something
        assert!(table.resolve(key1).is_some());
        assert!(table.resolve(key2).is_some());
    }

    #[test]
    fn clone_handle_shares_same_arc() {
        let table = HandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        let arc1 = table.resolve(key1).unwrap();
        let arc2 = table.resolve(key2).unwrap();
        // Same underlying allocation
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }

    #[test]
    fn release_original_clone_still_resolves() {
        let table = HandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        assert!(table.release(key1));
        assert!(table.resolve(key1).is_none());
        assert!(table.resolve(key2).is_some());
    }

    #[test]
    fn release_both_clones() {
        let table = HandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.clone_handle(key1).unwrap();
        assert!(table.release(key1));
        assert!(table.release(key2));
        assert!(table.resolve(key1).is_none());
        assert!(table.resolve(key2).is_none());
    }

    #[test]
    fn double_release_returns_false() {
        let table = HandleTable::new();
        let key = table.insert(make_function_ref());
        assert!(table.release(key));
        assert!(!table.release(key)); // second release returns false
    }

    #[test]
    fn try_from_rejects_primitives() {
        assert!(HandleTableValue::try_from(BexExternalValue::Null).is_err());
        assert!(HandleTableValue::try_from(BexExternalValue::Int(1)).is_err());
        assert!(HandleTableValue::try_from(BexExternalValue::String("hi".into())).is_err());
        assert!(HandleTableValue::try_from(BexExternalValue::Bool(true)).is_err());
        assert!(HandleTableValue::try_from(BexExternalValue::Float(1.0)).is_err());
    }

    #[test]
    fn try_from_accepts_function_ref() {
        let val = BexExternalValue::FunctionRef { global_index: 7 };
        let htv = HandleTableValue::try_from(val).unwrap();
        assert!(matches!(
            htv,
            HandleTableValue::FunctionRef { global_index: 7 }
        ));
    }

    #[test]
    fn handle_type_function_ref() {
        let htv = HandleTableValue::FunctionRef { global_index: 0 };
        assert_eq!(htv.handle_type() as i32, BamlHandleType::FunctionRef as i32);
    }

    #[test]
    fn roundtrip_to_bex_external_value() {
        let original = HandleTableValue::FunctionRef { global_index: 99 };
        let bex: BexExternalValue = original.into();
        let back = HandleTableValue::try_from(bex).unwrap();
        assert!(matches!(
            back,
            HandleTableValue::FunctionRef { global_index: 99 }
        ));
    }

    #[test]
    fn key_starts_at_one() {
        let table = HandleTable::new();
        let key = table.insert(make_function_ref());
        assert_eq!(key, 1, "first key should be 1 (0 is reserved as invalid)");
    }

    #[test]
    fn keys_are_monotonically_increasing() {
        let table = HandleTable::new();
        let key1 = table.insert(make_function_ref());
        let key2 = table.insert(make_function_ref());
        let key3 = table.insert(make_function_ref());
        assert!(key1 < key2);
        assert!(key2 < key3);
    }
}
