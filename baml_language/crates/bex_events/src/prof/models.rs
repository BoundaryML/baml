//! Per-engine LLM model interning shared by producers and storage drains.
//!
//! LLM metadata records stay compact by carrying a `u32` model id. The name
//! table is registered alongside immutable engine metadata, but its contents
//! grow on the cold LLM-completion path. Storage consumers keep their own
//! cursor and materialize newly visible entries as `model_birth` rows.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

/// One immutable model-table entry. Id zero is reserved for unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelMetadataEntry {
    pub model_id: u32,
    pub name: String,
}

#[derive(Debug, Default)]
struct Inner {
    by_name: HashMap<String, u32>,
    /// Index `id - 1`, because id zero is reserved.
    entries: Vec<ModelMetadataEntry>,
}

/// A small, thread-safe per-engine model interner.
#[derive(Debug, Default)]
pub struct ModelMetadataTable {
    inner: Mutex<Inner>,
}

impl ModelMetadataTable {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Return the existing id for `name`, or allocate the next dense id.
    ///
    /// The LLM completion path is cold and the cardinality is normally tiny,
    /// so a mutex keeps the semantics obvious without affecting call/return.
    #[must_use]
    pub fn intern(&self, name: &str) -> u32 {
        let mut inner = self.lock();
        if let Some(&id) = inner.by_name.get(name) {
            return id;
        }
        let id = u32::try_from(inner.entries.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .expect("more than u32::MAX distinct LLM models in one engine");
        let name = name.to_owned();
        inner.by_name.insert(name.clone(), id);
        inner
            .entries
            .push(ModelMetadataEntry { model_id: id, name });
        id
    }

    /// Snapshot entries whose id is greater than `after_model_id`.
    ///
    /// Consumers own their cursor; taking a snapshot never steals births from
    /// another native/wasm/live-mirror reader.
    #[must_use]
    pub fn entries_after(&self, after_model_id: u32) -> Vec<ModelMetadataEntry> {
        let inner = self.lock();
        let start = usize::try_from(after_model_id)
            .unwrap_or(usize::MAX)
            .min(inner.entries.len());
        inner.entries[start..].to_vec()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::ModelMetadataTable;

    #[test]
    fn ids_are_dense_deduplicated_and_cursor_safe() {
        let table = ModelMetadataTable::default();
        assert_eq!(table.intern("gpt-5"), 1);
        assert_eq!(table.intern("claude-4"), 2);
        assert_eq!(table.intern("gpt-5"), 1);
        assert_eq!(table.entries_after(0).len(), 2);
        let tail = table.entries_after(1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].model_id, 2);
        assert_eq!(tail[0].name, "claude-4");
        assert!(table.entries_after(2).is_empty());
    }
}
