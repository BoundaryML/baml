//! Telemetry-only function registration. Reference lifetime and relocation are
//! supplied by the runtime; this table never keeps executable objects alive.

use std::sync::{
    RwLock,
    atomic::{AtomicBool, Ordering},
};

use rustc_hash::FxHashMap;

use crate::{FunctionId, FunctionIdAllocator, FunctionIdExhausted};

const SHARD_BITS: u32 = 6;
const SHARD_COUNT: usize = 1 << SHARD_BITS;

// Separate independently written locks, including on machines with 128-byte
// cache lines. These are engine-owned; worker exit cannot discard registrations.
#[repr(align(128))]
struct FunctionShard<R>(RwLock<FxHashMap<FunctionId, R>>);

/// Publication state on one function object. Clone preserves it for moving GC;
/// constructing a new definition must reset it, together with its identity.
#[derive(Debug, Default)]
pub struct FunctionRegistration(AtomicBool);

impl Clone for FunctionRegistration {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.0.load(Ordering::Acquire)))
    }
}

/// Initial definitions use a dense vector. Observed dynamic definitions use a
/// sharded weak map, with references repaired or removed by the runtime's GC.
pub struct FunctionLookup<R> {
    ids: FunctionIdAllocator,
    static_functions: Vec<R>,
    dynamic: [FunctionShard<R>; SHARD_COUNT],
}

impl<R> Default for FunctionLookup<R> {
    fn default() -> Self {
        Self {
            ids: FunctionIdAllocator::default(),
            static_functions: Vec::new(),
            dynamic: std::array::from_fn(|_| FunctionShard(RwLock::new(FxHashMap::default()))),
        }
    }
}

impl<R: Copy> FunctionLookup<R> {
    #[inline]
    fn shard(&self, id: FunctionId) -> &RwLock<FxHashMap<FunctionId, R>> {
        // Take the HIGH product bits: masking the original ID (or low product
        // bits) funnels periodically observed functions into the same shard.
        let index = id.get().wrapping_mul(0x9e37_79b9_7f4a_7c15) >> (u64::BITS - SHARD_BITS);
        &self.dynamic[index as usize].0
    }

    pub fn allocate_id(&self) -> Result<FunctionId, FunctionIdExhausted> {
        self.ids.allocate()
    }

    /// Called only during initial loading, before allocating any dynamic IDs.
    pub fn register_static(
        &mut self,
        reference: R,
        registration: &FunctionRegistration,
    ) -> Result<FunctionId, FunctionIdExhausted> {
        let id = self.ids.allocate()?;
        assert_eq!(id.get(), self.static_functions.len() as u64 + 1);
        self.static_functions.push(reference);
        registration.0.store(true, Ordering::Release);
        Ok(id)
    }

    /// Ensure registration before publishing a telemetry reference. Call only
    /// when creating a definition, not on an existing call-path cache hit.
    /// The ID, reference and flag must belong to the same live function object.
    #[inline]
    pub fn register(&self, id: FunctionId, reference: R, registration: &FunctionRegistration) {
        if !registration.0.load(Ordering::Acquire) {
            self.register_slow(id, reference, registration);
        }
    }

    #[cold]
    #[inline(never)]
    fn register_slow(&self, id: FunctionId, reference: R, registration: &FunctionRegistration) {
        let mut dynamic = self.shard(id).write().expect("function lookup poisoned");
        if !registration.0.load(Ordering::Relaxed) {
            let previous = dynamic.insert(id, reference);
            assert!(previous.is_none(), "function IDs cannot be reused");
            // Another worker must not publish a definition until insertion has
            // completed. The flag travels with the function when GC moves it.
            registration.0.store(true, Ordering::Release);
        }
    }

    pub fn get(&self, id: FunctionId) -> Option<R> {
        // Dynamic IDs can exceed usize on wasm32; never truncate into the vector.
        if let Ok(index) = usize::try_from(id.get() - 1)
            && let Some(reference) = self.static_functions.get(index)
        {
            return Some(*reference);
        }
        self.shard(id)
            .read()
            .expect("function lookup poisoned")
            .get(&id)
            .copied()
    }

    /// Copy currently available references, holding only one shard lock at a
    /// time. Concurrent registration can proceed between shards, so this is
    /// not an atomic snapshot of all registrations.
    pub fn references(&self) -> Vec<R> {
        let mut references = self.static_functions.clone();
        for shard in &self.dynamic {
            let dynamic = shard.0.read().expect("function lookup poisoned");
            references.extend(dynamic.values().copied());
        }
        references
    }

    /// The runtime supplies GC semantics while excluding all mutators/readers.
    /// Preserve registered live references; removing a live registration would
    /// invalidate its flag. Collected objects' flags disappear with the objects.
    pub fn retain_dynamic(&self, mut retain: impl FnMut(&mut R) -> bool) {
        for shard in &self.dynamic {
            let mut dynamic = shard.0.write().expect("function lookup poisoned");
            dynamic.retain(|_, reference| retain(reference));
            if dynamic.is_empty() {
                *dynamic = FxHashMap::default();
            } else if dynamic.len() < dynamic.capacity() / 4 {
                let target = dynamic.len().saturating_mul(2);
                dynamic.shrink_to(target);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_ids_cover_shards_and_survive_reference_repair() {
        let mut lookup = FunctionLookup::default();
        let static_registration = FunctionRegistration::default();
        let static_id = lookup
            .register_static(usize::MAX, &static_registration)
            .unwrap();
        let mut functions = Vec::new();
        for value in 0..1024 {
            // Only every 64th definition is observed. Low-bit masking would
            // collapse these registrations into one shard.
            for _ in 0..63 {
                lookup.allocate_id().unwrap();
            }
            let id = lookup.allocate_id().unwrap();
            let registration = FunctionRegistration::default();
            lookup.register(id, value, &registration);
            functions.push((id, registration));
        }
        assert!(
            lookup
                .dynamic
                .iter()
                .all(|shard| !shard.0.read().unwrap().is_empty())
        );
        let mut references = lookup.references();
        references.sort_unstable();
        assert_eq!(
            references,
            (0..1024).chain([usize::MAX]).collect::<Vec<_>>()
        );

        lookup.retain_dynamic(|reference| {
            if *reference % 2 == 0 {
                *reference += 10_000;
                true
            } else {
                false
            }
        });
        for (value, (id, registration)) in functions.iter().enumerate() {
            if value % 2 == 0 {
                let moved_flag = registration.clone();
                lookup.register(*id, value + 10_000, &moved_flag);
                assert_eq!(lookup.get(*id), Some(value + 10_000));
            } else {
                assert_eq!(lookup.get(*id), None);
            }
        }
        assert_eq!(lookup.references().len(), 513);
        assert_eq!(lookup.get(static_id), Some(usize::MAX));
        lookup.retain_dynamic(|_| false);
        assert_eq!(lookup.references(), vec![usize::MAX]);
        assert!(
            lookup
                .dynamic
                .iter()
                .all(|shard| shard.0.read().unwrap().capacity() == 0)
        );
    }

    #[test]
    fn concurrent_first_observation_publishes_before_returning() {
        let lookup = FunctionLookup::default();
        let id = lookup.allocate_id().unwrap();
        let registration = FunctionRegistration::default();
        assert_eq!(lookup.get(id), None);
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    lookup.register(id, 42, &registration);
                    assert_eq!(lookup.get(id), Some(42));
                });
            }
        });
        assert_eq!(lookup.references(), vec![42]);
        let relocated = registration.clone();
        assert!(registration.0.load(Ordering::Acquire));
        lookup.retain_dynamic(|value| {
            *value = 99;
            true
        });
        lookup.register(id, 99, &relocated);
        assert_eq!(lookup.get(id), Some(99));
        lookup.retain_dynamic(|_| false);
        assert_eq!(lookup.get(id), None);
    }
}
