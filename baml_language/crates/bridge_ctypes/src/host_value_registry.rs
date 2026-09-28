//! Storage shared by native adapters for host-owned values.
//!
//! Each language owns a separate registry and controls invocation, cloning and
//! release of its values. These keys must never be resolved in `HANDLE_TABLE`.
use std::{
    collections::HashMap,
    sync::{
        LockResult, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

pub struct HostValueRegistry<T> {
    next_key: AtomicU64,
    table: Mutex<HashMap<u64, T>>,
}

impl<T> Default for HostValueRegistry<T> {
    fn default() -> Self {
        Self {
            next_key: AtomicU64::new(1),
            table: Mutex::new(HashMap::new()),
        }
    }
}

impl<T> HostValueRegistry<T> {
    /// Reserve a nonzero key, including for values stored on the language side.
    pub fn mint_key(&self) -> u64 {
        loop {
            let key = self.next_key.fetch_add(1, Ordering::Relaxed);
            if key != 0 {
                return key;
            }
        }
    }

    /// Register a value under a new key. Like the native encoders, fail closed
    /// on a poisoned table; release and dispatch use `lock` to handle poison.
    pub fn insert(&self, value: T) -> u64 {
        let key = self.mint_key();
        self.table.lock().unwrap().insert(key, value);
        key
    }

    /// Let adapters clone under the appropriate runtime/GIL and remove values
    /// before dropping them outside the lock. Poison handling stays host-owned.
    pub fn lock(&self) -> LockResult<MutexGuard<'_, HashMap<u64, T>>> {
        self.table.lock()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc, thread};

    use super::*;

    #[test]
    fn stored_and_language_owned_values_share_one_key_sequence() {
        let registry = Arc::new(HostValueRegistry::default());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let registry = Arc::clone(&registry);
                thread::spawn(move || (registry.insert("callable"), registry.mint_key()))
            })
            .collect();
        let keys: HashSet<_> = threads
            .into_iter()
            .flat_map(|thread| {
                let (stored, opaque) = thread.join().unwrap();
                assert_eq!(registry.lock().unwrap().get(&stored), Some(&"callable"));
                assert!(!registry.lock().unwrap().contains_key(&opaque));
                [stored, opaque]
            })
            .collect();
        assert_eq!(keys.len(), 16);
        assert!(!keys.contains(&0));
    }
}
