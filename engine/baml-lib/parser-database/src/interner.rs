//! An extremely simple, non-threadsafe string interner to reduce the memory usage and the
//! allocation pressure of ParserDatabase.
//!
//! The StringIds returned by `intern` are only valid for this specific instance of the interner
//! they were interned with.

use indexmap::IndexSet;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct StringId(usize);

#[derive(Default)]
pub(crate) struct StringInterner {
    map: IndexSet<String>,
}

impl StringInterner {
    pub(crate) fn get(&self, id: StringId) -> Option<&str> {
        self.map.get_index(id.0).map(|s| s.as_str())
    }

    /// Get an already-interned string.
    pub(crate) fn lookup(&self, s: &str) -> Option<StringId> {
        log::info!("Looking up: {}", s);
        log::info!("Current contents of self.map: {:?}", self.map);
        let result = self.map.get_index_of(s).map(StringId);
        log::info!("Result: {:?}", result);
        result
    }

    pub(crate) fn intern(&mut self, s: &str) -> StringId {
        if let Some(id) = self.lookup(s) {
            id
        } else {
            let (idx, is_new) = self.map.insert_full(s.to_owned());
            debug_assert!(is_new);
            StringId(idx)
        }
    }
}
