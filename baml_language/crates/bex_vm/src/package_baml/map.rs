use std::{collections::HashMap, sync::Arc};

use bex_vm_types::{EntryId, HeapPtr, MapData, MapReadGuard, MapWriteGuard, Object, Value};

use super::{BamlClassMap, Continuation, MapView, NativeCallResult, PackageBamlImpl};
use crate::{BexVm, VmPanic, errors::VmRustFnError};

#[derive(Clone, Copy)]
enum Operation {
    Has,
    Get,
    Index,
    Set,
    Delete,
    GetOrInsert,
}

struct MapDriver {
    map: Value,
    key: Value,
    value: Value,
    state: Value,
    hasher: Option<Arc<super::hasher::HasherState>>,
    operation: Operation,
    hash: u64,
    epoch: u64,
    candidates: Vec<(EntryId, Value)>,
    cursor: usize,
    hashing: bool,
    /// The map is thread-local: escape analysis selected a `$local` twin.
    thread_local: bool,
}

impl MapDriver {
    fn start(
        vm: &mut BexVm,
        args: &[Value],
        operation: Operation,
        thread_local: bool,
    ) -> NativeCallResult {
        if let Err(error) = vm.as_map(&args[0]) {
            return error.into();
        }
        let string_hash = vm
            .as_string(&args[1])
            .ok()
            .map(|key| bex_vm_types::map_string_hash(key.as_str()));
        let (state, hasher) = if string_hash.is_some() {
            (Value::NULL, None)
        } else {
            let (state, hasher) = super::hasher::new_state(vm);
            (state, Some(hasher))
        };
        let mut driver = Self {
            map: args[0],
            key: args[1],
            value: args.get(2).copied().unwrap_or(Value::NULL),
            state,
            hasher,
            operation,
            hash: string_hash.unwrap_or(0),
            epoch: 0,
            candidates: Vec::new(),
            cursor: 0,
            hashing: string_hash.is_none(),
            thread_local,
        };
        if string_hash.is_some() {
            driver.snapshot(vm);
            return driver.drive(vm, None);
        }
        let result = super::hashing::hash_value(vm, driver.key, state);
        super::chain(vm, result, Box::new(driver))
    }

    /// The map's backing store, read without its lock when it is thread-local.
    #[allow(unsafe_code)]
    fn entries<'vm>(&self, vm: &'vm BexVm) -> MapReadGuard<'vm> {
        let Object::Map(map) = vm.get_object(self.map.as_object_ptr().unwrap()) else {
            unreachable!("map receiver was validated")
        };
        if self.thread_local {
            // SAFETY: only the `$local` twins set `thread_local`, and the
            // native backend selects those only for a receiver escape
            // analysis proved thread-local.
            unsafe { map.lock_thread_local() }
        } else {
            map.lock()
        }
    }

    /// [`Self::entries`] for writing.
    #[allow(unsafe_code)]
    fn entries_mut<'vm>(&self, vm: &'vm BexVm) -> MapWriteGuard<'vm> {
        let Object::Map(map) = vm.get_object(self.map.as_object_ptr().unwrap()) else {
            unreachable!("map receiver was validated")
        };
        let debt = vm.tlab.alloc_debt();
        if self.thread_local {
            // SAFETY: as for `entries`.
            unsafe { map.lock_mut_thread_local(debt) }
        } else {
            map.lock_mut(debt)
        }
    }

    fn snapshot(&mut self, vm: &BexVm) {
        let (epoch, entries) = self.entries(vm).snapshot_bucket(self.hash);
        self.epoch = epoch;
        self.candidates = entries
            .into_iter()
            .map(|(id, entry)| (id, entry.key))
            .collect();
        self.cursor = 0;
    }

    fn drive(mut self, vm: &mut BexVm, mut matched: Option<EntryId>) -> NativeCallResult {
        loop {
            if self.entries(vm).get_if_epoch(self.epoch, None).is_err() {
                self.snapshot(vm);
                matched = None;
                continue;
            }
            if matched.is_none() && self.cursor < self.candidates.len() {
                let key = self.candidates[self.cursor].1;
                let result = match (vm.as_string(&key), vm.as_string(&self.key)) {
                    (Ok(left), Ok(right)) => NativeCallResult::Done(Value::bool(left == right)),
                    _ => super::ops::equals_structural_default(vm, key, self.key),
                };
                match result {
                    NativeCallResult::Done(value) => {
                        if value.as_bool() == Some(true) {
                            matched = Some(self.candidates[self.cursor].0);
                        }
                        self.cursor += 1;
                        continue;
                    }
                    other => return super::chain(vm, other, Box::new(self)),
                }
            }
            let result = match self.operation {
                Operation::Set => self
                    .entries_mut(vm)
                    .set_if_epoch(self.epoch, matched, self.hash, self.key, self.value),
                Operation::GetOrInsert if matched.is_none() => self
                    .entries_mut(vm)
                    .set_if_epoch(self.epoch, None, self.hash, self.key, self.value)
                    .map(|_| Some(self.value)),
                Operation::Delete => self.entries_mut(vm).remove_if_epoch(self.epoch, matched),
                _ => self.entries(vm).get_if_epoch(self.epoch, matched),
            };
            let Ok(previous) = result else {
                self.snapshot(vm);
                matched = None;
                continue;
            };
            if matches!(self.operation, Operation::Set | Operation::GetOrInsert) {
                let ptr = self.map.as_object_ptr().unwrap();
                vm.heap.write_barrier(ptr, self.key);
                vm.heap.write_barrier(ptr, self.value);
            }
            return match self.operation {
                Operation::Has => NativeCallResult::Done(Value::bool(previous.is_some())),
                Operation::Index if previous.is_none() => {
                    NativeCallResult::Error(VmRustFnError::Panic(VmPanic::MapKeyNotFound))
                }
                _ => NativeCallResult::Done(previous.unwrap_or(Value::NULL)),
            };
        }
    }
}

impl Continuation for MapDriver {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let matched = if self.hashing {
            self.hashing = false;
            self.hash = super::hasher::finish_state(
                self.hasher.as_ref().expect("hash callback has a state"),
            );
            self.snapshot(vm);
            None
        } else {
            let matched = (value.as_bool() == Some(true)).then(|| self.candidates[self.cursor].0);
            self.cursor += 1;
            matched
        };
        (*self).drive(vm, matched)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        [self.map, self.key, self.value, self.state]
            .into_iter()
            .chain(self.candidates.iter().map(|(_, key)| *key))
            .filter_map(|value| value.as_object_ptr())
            .collect()
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for value in [
            &mut self.map,
            &mut self.key,
            &mut self.value,
            &mut self.state,
        ]
        .into_iter()
        .chain(self.candidates.iter_mut().map(|(_, key)| key))
        {
            if let Some(ptr) = value.as_object_ptr() {
                if let Some(new) = forwarding.get(&ptr) {
                    *value = Value::object(*new);
                }
            }
        }
    }
}

impl BamlClassMap for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(map: MapView<'_>) -> i64 {
        map.len() as i64
    }

    fn keys(_vm: &mut BexVm, map: MapView<'_>) -> Vec<Value> {
        map.keys().copied().collect()
    }

    fn values(map: MapView<'_>) -> Vec<Value> {
        map.values().copied().collect()
    }

    fn __glue_has(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Has, false)
    }

    fn __glue_has__local(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Has, true)
    }

    fn has(_vm: &mut BexVm, _map: MapView<'_>, _key: &Value) -> NativeCallResult {
        unreachable!("Map.has uses custom glue")
    }

    fn __glue_get(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Get, false)
    }

    fn __glue_get__local(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Get, true)
    }

    fn get(_vm: &mut BexVm, _map: MapView<'_>, _key: &Value) -> NativeCallResult {
        unreachable!("Map.get uses custom glue")
    }

    fn __glue_index(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Index, false)
    }

    fn __glue_index__local(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Index, true)
    }

    fn index(_vm: &mut BexVm, _map: MapView<'_>, _key: &Value) -> NativeCallResult {
        unreachable!("Map.index uses custom glue")
    }

    fn __glue_set(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Set, false)
    }

    fn __glue_set__local(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Set, true)
    }

    fn set(_vm: &mut BexVm, _map: &Value, _key: &Value, _value: &Value) -> NativeCallResult {
        unreachable!("Map.set uses custom glue")
    }

    fn __glue_delete(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Delete, false)
    }

    fn __glue_delete__local(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::Delete, true)
    }

    fn delete(_vm: &mut BexVm, _map: &Value, _key: &Value) -> NativeCallResult {
        unreachable!("Map.delete uses custom glue")
    }

    fn __glue_get_or_insert(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::GetOrInsert, false)
    }

    fn __glue_get_or_insert__local(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        MapDriver::start(vm, args, Operation::GetOrInsert, true)
    }

    fn get_or_insert(
        _vm: &mut BexVm,
        _map: &Value,
        _key: &Value,
        _default: &Value,
    ) -> NativeCallResult {
        unreachable!("Map.get_or_insert uses custom glue")
    }

    #[allow(clippy::unused_unit)]
    fn clear(map: &mut MapData) -> () {
        map.clear();
    }
}
