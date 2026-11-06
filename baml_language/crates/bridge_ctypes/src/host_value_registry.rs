//! Storage shared by native adapters for host-owned values.
//!
//! Each language owns a separate registry and controls invocation, cloning and
//! release of its values. Its keys are the engine's references to host objects
//! ([`HostKey`]), never engine handle-table keys; the two are distinct types.
use std::{
    collections::HashMap,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_project::HostKey;

pub struct HostValueRegistry<T> {
    next_key: AtomicU64,
    table: Mutex<HashMap<HostKey, Entry<T>>>,
}

/// One registered host value: the value (or its absence, for one stored on
/// the language side under this key) and how many references to it the
/// engine and in-flight requests hold.
struct Entry<T> {
    /// The value, when this registry stores it.
    value: Option<T>,
    /// References handed out: one per send of the key to the engine, each
    /// given back by the engine's release (or by a rollback that never sent
    /// it).
    count: u64,
}

/// What [`HostValueRegistry::release`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Released<T> {
    /// References remain: the value stays registered.
    Held,
    /// The last reference was given back: the key is gone, and here is the
    /// value it stored (`None` for one stored on the language side).
    Last(Option<T>),
    /// The key is not registered. Giving back a reference twice is the
    /// caller's bug; it is reported, never applied to another key.
    Unknown,
}

/// A key that is no longer registered: its references were all given back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadHostKey(pub HostKey);

impl std::fmt::Display for DeadHostKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "host value {} is no longer registered", self.0)
    }
}

impl std::error::Error for DeadHostKey {}

impl<T> Default for HostValueRegistry<T> {
    fn default() -> Self {
        Self {
            next_key: AtomicU64::new(1),
            table: Mutex::new(HashMap::new()),
        }
    }
}

impl<T> HostValueRegistry<T> {
    fn fresh_key(&self) -> HostKey {
        loop {
            if let Some(key) = HostKey::new(self.next_key.fetch_add(1, Ordering::Relaxed)) {
                return key;
            }
        }
    }

    fn table(&self) -> MutexGuard<'_, HashMap<HostKey, Entry<T>>> {
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Register a value stored on the language side under a new key, holding
    /// the one reference the send it is minted for takes.
    pub fn mint_key(&self) -> HostKey {
        let key = self.fresh_key();
        self.table().insert(
            key,
            Entry {
                value: None,
                count: 1,
            },
        );
        key
    }

    /// Register `value` under a new key, holding the one reference the send
    /// it is registered for takes.
    pub fn insert(&self, value: T) -> HostKey {
        let key = self.fresh_key();
        self.table().insert(
            key,
            Entry {
                value: Some(value),
                count: 1,
            },
        );
        key
    }

    /// Take one more reference to `key` for sending it again (a key the host
    /// received back, or one it registered earlier and still holds). Under the
    /// registry lock, so a release racing this send cannot remove the key
    /// between the check and the send.
    pub fn send_again(&self, key: HostKey) -> Result<(), DeadHostKey> {
        match self.table().get_mut(&key) {
            Some(entry) => {
                entry.count += 1;
                Ok(())
            }
            None => Err(DeadHostKey(key)),
        }
    }

    /// Give back `count` references to `key` (the engine's release, or a
    /// rollback of sends that never happened). At zero the key is removed and
    /// its value returned, for the caller to drop outside any lock it needs.
    pub fn release(&self, key: HostKey, count: u64) -> Released<T> {
        let mut table = self.table();
        let Some(entry) = table.get_mut(&key) else {
            return Released::Unknown;
        };
        entry.count = entry.count.saturating_sub(count);
        if entry.count > 0 {
            return Released::Held;
        }
        let entry = table
            .remove(&key)
            .unwrap_or_else(|| unreachable!("the entry was just read under this lock"));
        Released::Last(entry.value)
    }

    /// Read the value stored under `key`, if it is registered and stored here.
    pub fn with<R>(&self, key: HostKey, read: impl FnOnce(&T) -> R) -> Option<R> {
        self.table().get(&key)?.value.as_ref().map(read)
    }

    /// Whether `key` is registered.
    pub fn contains(&self, key: HostKey) -> bool {
        self.table().contains_key(&key)
    }

    /// How many keys are registered.
    pub fn len(&self) -> usize {
        self.table().len()
    }

    /// Whether no key is registered.
    pub fn is_empty(&self) -> bool {
        self.table().is_empty()
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
                assert_eq!(registry.with(stored, |value| *value), Some("callable"));
                assert!(registry.contains(opaque));
                assert_eq!(registry.with(opaque, |value| *value), None);
                [stored, opaque]
            })
            .collect();
        assert_eq!(keys.len(), 16);
    }

    /// A key lives while any reference to it is out: each send takes one,
    /// each release gives some back, and the last one removes it.
    #[test]
    fn a_key_lives_until_every_reference_is_given_back() {
        let registry = HostValueRegistry::default();
        let key = registry.insert("value");
        registry.send_again(key).unwrap();
        registry.send_again(key).unwrap();
        assert_eq!(registry.release(key, 2), Released::Held);
        assert!(registry.contains(key));
        assert_eq!(registry.release(key, 1), Released::Last(Some("value")));
        assert!(!registry.contains(key));
        assert_eq!(registry.release(key, 1), Released::Unknown);
        assert_eq!(registry.send_again(key), Err(DeadHostKey(key)));
    }

    /// A send racing the engine's release either takes its reference first,
    /// so the key survives the release, or finds the key gone and refuses.
    #[test]
    fn sending_again_is_atomic_with_release() {
        let registry = Arc::new(HostValueRegistry::default());
        for _ in 0..200 {
            let key = registry.insert(());
            let sender = {
                let registry = Arc::clone(&registry);
                thread::spawn(move || registry.send_again(key))
            };
            let released = registry.release(key, 1);
            match (sender.join().unwrap(), released) {
                (Ok(()), Released::Held) => {
                    assert_eq!(registry.release(key, 1), Released::Last(Some(())));
                }
                (Ok(()), Released::Last(_)) => unreachable!("a taken reference keeps the key"),
                (Err(_), Released::Last(_)) => {}
                (_, released) => panic!("unexpected interleaving: {released:?}"),
            }
        }
    }
}
