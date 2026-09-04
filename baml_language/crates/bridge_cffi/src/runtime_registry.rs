//! Process-wide runtime registrations. Keys are never reused after removal.
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock, RwLock, Weak},
};

use bex_project::Bex;

use crate::BridgeError;

struct Entry {
    runtime: Arc<dyn Bex>,
    program: Option<Vec<u8>>,
}
#[derive(Default)]
struct Registry {
    entries: HashMap<u64, Entry>,
    retired: HashSet<u64>,
    identities: HashMap<u64, Weak<dyn Bex>>,
    next_dynamic: u64,
}
static REGISTRY: LazyLock<RwLock<Registry>> = LazyLock::new(|| RwLock::new(Registry::default()));

/// Generated identities occupy the upper half; dynamic identities the lower half.
pub const GENERATED_KEY_BIT: u64 = 1u64 << 63;

pub fn get_runtime_by_key(key: u64) -> Result<Arc<dyn Bex>, BridgeError> {
    REGISTRY
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .entries
        .get(&key)
        .map(|entry| entry.runtime.clone())
        .ok_or_else(|| BridgeError::Startup(format!("Unknown BAML runtime key {key}")))
}

/// Compatibility accessor: ambiguity is an error, never a last-import-wins choice.
pub fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    let registry = REGISTRY.read().map_err(|_| BridgeError::LockPoisoned)?;
    if registry.entries.len() > 1 {
        return Err(BridgeError::Startup(
            "Multiple BAML runtimes are registered; supply the originating runtime key".into(),
        ));
    }
    registry
        .entries
        .values()
        .next()
        .map(|entry| entry.runtime.clone())
        .ok_or(BridgeError::NotInitialized)
}

pub fn runtime_key(runtime: &Arc<dyn Bex>) -> Result<u64, BridgeError> {
    REGISTRY
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .identities
        .iter()
        .find(|(_, entry)| {
            entry
                .upgrade()
                .is_some_and(|entry| Arc::ptr_eq(&entry, runtime))
        })
        .map(|(key, _)| *key)
        .ok_or(BridgeError::NotInitialized)
}

pub(crate) fn insert_dynamic(runtime: Arc<dyn Bex>) -> Result<u64, BridgeError> {
    let mut registry = REGISTRY.write().map_err(|_| BridgeError::LockPoisoned)?;
    let key = registry
        .next_dynamic
        .checked_add(1)
        .filter(|key| *key < GENERATED_KEY_BIT)
        .ok_or_else(|| BridgeError::Startup("BAML runtime key space exhausted".into()))?;
    registry.next_dynamic = key;
    registry
        .identities
        .retain(|_, runtime| runtime.strong_count() != 0);
    registry.identities.insert(key, Arc::downgrade(&runtime));
    registry.entries.insert(
        key,
        Entry {
            runtime,
            program: None,
        },
    );
    Ok(key)
}

/// Serialize compilation and insertion so concurrent identical imports construct one engine.
/// Compare full canonical program bytes, not just the truncated identity hash.
pub(crate) fn register_generated(
    key: u64,
    program: Vec<u8>,
    build: impl FnOnce() -> Result<Arc<dyn Bex>, BridgeError>,
) -> Result<Arc<dyn Bex>, BridgeError> {
    if key & GENERATED_KEY_BIT == 0 {
        return Err(BridgeError::Startup(
            "Generated BAML runtime keys must have bit 63 set".into(),
        ));
    }
    let mut registry = REGISTRY.write().map_err(|_| BridgeError::LockPoisoned)?;
    if let Some(entry) = registry.entries.get(&key) {
        if entry.program.as_ref() == Some(&program) {
            return Ok(entry.runtime.clone());
        }
        return Err(BridgeError::Startup(format!(
            "Conflicting BAML program registration for uint64 key {key}"
        )));
    }
    if registry.retired.contains(&key) {
        return Err(BridgeError::Startup(format!(
            "BAML runtime key {key} has been retired"
        )));
    }
    let runtime = std::panic::catch_unwind(std::panic::AssertUnwindSafe(build))
        .map_err(|_| BridgeError::Startup("Panic while constructing BAML runtime".into()))??;
    registry
        .identities
        .retain(|_, runtime| runtime.strong_count() != 0);
    registry.identities.insert(key, Arc::downgrade(&runtime));
    registry.entries.insert(
        key,
        Entry {
            runtime: runtime.clone(),
            program: Some(program),
        },
    );
    Ok(runtime)
}

/// Unregister immediately. Calls that already acquired an Arc retain their runtime.
/// Generated registrations are process-owned and cannot be removed by an SDK import.
pub fn unregister_runtime(key: u64) -> Result<Arc<dyn Bex>, BridgeError> {
    let mut registry = REGISTRY.write().map_err(|_| BridgeError::LockPoisoned)?;
    if key & GENERATED_KEY_BIT != 0 {
        return Err(BridgeError::Startup(
            "Generated BAML registrations are process-owned".into(),
        ));
    }
    let entry = registry
        .entries
        .remove(&key)
        .ok_or(BridgeError::NotInitialized)?;
    registry.retired.insert(key);
    Ok(entry.runtime)
}

pub(crate) fn take_all() -> Result<Vec<Arc<dyn Bex>>, BridgeError> {
    let mut registry = REGISTRY.write().map_err(|_| BridgeError::LockPoisoned)?;
    let entries = std::mem::take(&mut registry.entries);
    registry.retired.extend(entries.keys());
    let mut runtimes: Vec<_> = registry
        .identities
        .values()
        .filter_map(Weak::upgrade)
        .collect();
    runtimes.extend(crate::runtime_owner::retiring_runtimes());
    drop(entries);
    Ok(runtimes)
}
