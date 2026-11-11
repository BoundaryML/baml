//! How the interpreter reads and writes maps in the shared object format
//! ([`MapRef`]): a snapshot of every entry for whole-map readers, and an
//! edit guard with the epoch-checked operations the multi-step lookups use.
//!
//! Replaces the `MapReadGuard`/`MapWriteGuard` over `Box<MapData>` that
//! `BexVm::as_map`/`as_map_mut` returned. The method names are `MapData`'s,
//! so the callers read the same; entries come out as values, floats boxed.

use bex_vm_types::{
    EntryId, HeapPtr, MapEntry, MapEpochChanged, MapRef, Value,
    errors::VmInternalError,
    object_format::{Access, SlotValue},
    types::{MapGuard, ObjectType},
};

use crate::BexVm;

/// Every entry of a map, read under its lock, values boxed.
pub struct MapSnapshot {
    epoch: u64,
    entries: Vec<MapEntry>,
}

impl MapSnapshot {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Keys and values, in insertion order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Value, &Value)> {
        self.entries.iter().map(|entry| (&entry.key, &entry.value))
    }

    pub fn keys(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.entries.iter().map(|entry| &entry.key)
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.entries.iter().map(|entry| &entry.value)
    }

    /// Every entry, in insertion order.
    pub fn entries(&self) -> &[MapEntry] {
        &self.entries
    }

    /// Every entry's stored hash, in insertion order.
    pub fn hashes(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.entries.iter().map(|entry| entry.hash)
    }

    /// Every key and value, in insertion order.
    pub fn snapshot_entries(&self) -> Vec<(Value, Value)> {
        self.entries
            .iter()
            .map(|entry| (entry.key, entry.value))
            .collect()
    }

    /// The value of the entry with hash `hash` whose key `matches` accepts.
    pub fn get_hashed(&self, hash: u64, mut matches: impl FnMut(&Value) -> bool) -> Option<Value> {
        self.entries
            .iter()
            .find(|entry| entry.hash == hash && matches(&entry.key))
            .map(|entry| entry.value)
    }
}

impl BexVm {
    /// The map `value` points to.
    pub fn as_map_ref(&self, value: &Value) -> Result<MapRef, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::Map)?;
        self.map_at(ptr).ok_or_else(|| VmInternalError::TypeError {
            expected: ObjectType::Map.into(),
            got: ObjectType::of(self.get_object(ptr)).into(),
        })
    }

    /// The map at `ptr`, if it is one.
    #[inline]
    pub fn map_at(&self, ptr: HeapPtr) -> Option<MapRef> {
        // SAFETY: the VM holds a heap permit, so a pointer it holds is live.
        unsafe { MapRef::of(ptr) }
    }

    /// The map at `ptr`, which the caller has seen is one.
    ///
    /// # Panics
    /// If it is not a map.
    #[track_caller]
    pub fn expect_map(&self, ptr: HeapPtr) -> MapRef {
        self.map_at(ptr)
            .unwrap_or_else(|| panic!("expected a map at {ptr:?}"))
    }

    /// Every entry of `map`, read under its lock.
    pub fn map_snapshot(&self, map: MapRef, access: Access) -> MapSnapshot {
        let guard = map.lock(access);
        let entries = (0..guard.len())
            .map(|pos| MapEntry {
                key: self.box_slot(guard.key(pos)),
                value: self.box_slot(guard.value(pos)),
                hash: guard.hash(pos),
            })
            .collect();
        MapSnapshot {
            epoch: guard.epoch(),
            entries,
        }
    }

    /// The entries of the map `value` points to (see [`MapSnapshot`]).
    pub fn as_map(&self, value: &Value) -> Result<MapSnapshot, VmInternalError> {
        let map = self.as_map_ref(value)?;
        Ok(self.map_snapshot(map, Access::Shared))
    }

    /// [`Self::as_map`] for a map only this thread can reach, without its
    /// lock.
    ///
    /// # Safety
    ///
    /// As for [`Self::as_array_local`].
    pub unsafe fn as_map_local(&self, value: &Value) -> Result<MapSnapshot, VmInternalError> {
        let map = self.as_map_ref(value)?;
        Ok(self.map_snapshot(map, Access::ThreadLocal))
    }

    /// The map `value` points to, held for writing (see [`MapEdit`]).
    pub fn as_map_mut(&mut self, value: &Value) -> Result<MapEdit<'_>, VmInternalError> {
        let map = self.as_map_ref(value)?;
        Ok(MapEdit::new(self, map, Access::Shared))
    }

    /// [`Self::as_map_mut`] for a map only this thread can reach, without its
    /// lock.
    ///
    /// # Safety
    ///
    /// As for [`Self::as_array_local`].
    pub unsafe fn as_map_mut_local(
        &mut self,
        value: &Value,
    ) -> Result<MapEdit<'_>, VmInternalError> {
        let map = self.as_map_ref(value)?;
        Ok(MapEdit::new(self, map, Access::ThreadLocal))
    }

    /// The value of the entry of `map` with hash `hash` whose key `matches`
    /// accepts, read under the map's lock.
    pub fn map_get_hashed(
        &self,
        map: MapRef,
        access: Access,
        hash: u64,
        mut matches: impl FnMut(&Value) -> bool,
    ) -> Option<Value> {
        let guard = map.lock(access);
        let pos = guard.find(hash, |key| matches(&self.box_slot(key)))?;
        Some(self.box_slot(guard.value(pos)))
    }

    /// The map's epoch and the entries whose key has hash `hash`, for a
    /// lookup that compares keys with the lock released.
    pub fn map_bucket(
        &self,
        map: MapRef,
        access: Access,
        hash: u64,
    ) -> (u64, Vec<(EntryId, Value)>) {
        let guard = map.lock(access);
        let candidates = guard
            .candidates(hash)
            .into_iter()
            .map(|pos| (pos as EntryId, self.box_slot(guard.key(pos))))
            .collect();
        (guard.epoch(), candidates)
    }
}

/// A map held under its lock for writing, with `MapData`'s epoch-checked
/// operations. Values written are unboxed into the map's slots.
pub struct MapEdit<'vm> {
    vm: &'vm mut BexVm,
    guard: MapGuard,
}

impl<'vm> MapEdit<'vm> {
    pub fn new(vm: &'vm mut BexVm, map: MapRef, access: Access) -> Self {
        Self {
            vm,
            guard: map.lock(access),
        }
    }

    pub fn len(&self) -> usize {
        self.guard.len()
    }

    pub fn is_empty(&self) -> bool {
        self.guard.is_empty()
    }

    pub fn epoch(&self) -> u64 {
        self.guard.epoch()
    }

    pub fn clear(&mut self) {
        self.guard.clear();
    }

    fn check(&self, epoch: u64) -> Result<(), MapEpochChanged> {
        if epoch == self.guard.epoch() {
            Ok(())
        } else {
            Err(MapEpochChanged)
        }
    }

    fn position(id: EntryId) -> usize {
        usize::try_from(id).expect("an entry position fits usize")
    }

    /// Insert a key the caller knows is not in the map; returns its ID.
    pub fn insert_hashed(&mut self, hash: u64, key: Value, value: Value) -> EntryId {
        let pos = self
            .guard
            .insert(
                &mut self.vm.tlab,
                hash,
                SlotValue::Value(key),
                SlotValue::Value(value),
            )
            .unwrap_or_else(|error| panic!("a map entry does not fit its slots: {error}"));
        pos as EntryId
    }

    /// The value of entry `id`, if the snapshot that found it is current.
    pub fn get_if_epoch(
        &mut self,
        epoch: u64,
        id: Option<EntryId>,
    ) -> Result<Option<Value>, MapEpochChanged> {
        self.check(epoch)?;
        let value = id.map(|id| self.guard.value(Self::position(id)));
        Ok(value.map(|value| self.vm.slot_value(value)))
    }

    /// Replace entry `id`'s value, or insert `key` when `id` is `None`; the
    /// previous value.
    pub fn set_if_epoch(
        &mut self,
        epoch: u64,
        id: Option<EntryId>,
        hash: u64,
        key: Value,
        value: Value,
    ) -> Result<Option<Value>, MapEpochChanged> {
        self.check(epoch)?;
        let Some(id) = id else {
            self.insert_hashed(hash, key, value);
            return Ok(None);
        };
        let pos = Self::position(id);
        let previous = self.guard.value(pos);
        self.guard
            .set_value(pos, SlotValue::Value(value))
            .unwrap_or_else(|error| panic!("a map value does not fit its slot: {error}"));
        Ok(Some(self.vm.slot_value(previous)))
    }

    /// Remove entry `id`; its value.
    pub fn remove_if_epoch(
        &mut self,
        epoch: u64,
        id: Option<EntryId>,
    ) -> Result<Option<Value>, MapEpochChanged> {
        self.check(epoch)?;
        let Some(id) = id else {
            return Ok(None);
        };
        let removed = self.guard.remove(Self::position(id));
        Ok(Some(self.vm.slot_value(removed)))
    }
}
