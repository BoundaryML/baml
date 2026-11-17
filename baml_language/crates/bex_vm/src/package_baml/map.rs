use std::sync::Arc;

use bex_vm_types::{
    BexStr, EntryId, MapData, MapReadGuard, MapWriteGuard, Object, RootHaver, RootVisitor, Value,
};

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

/// A key whose hash and equality need no VM code: no `Hash` or `Equals`
/// implementation can run for it.
enum PlainKey<'vm> {
    Int(i64),
    Str(&'vm BexStr),
}

impl<'vm> PlainKey<'vm> {
    fn of(vm: &'vm BexVm, key: Value) -> Option<Self> {
        if let Some(int) = key.as_int() {
            return Some(Self::Int(int));
        }
        vm.as_string(&key).ok().map(Self::Str)
    }

    /// The hash the `Hash` driver would compute for this key.
    fn hash(&self) -> u64 {
        match self {
            Self::Int(int) => bex_vm_types::map_int_hash(*int),
            Self::Str(text) => bex_vm_types::map_string_hash(text.as_str()),
        }
    }

    /// Whether `candidate` equals this key, or `None` for a candidate of
    /// another kind, whose `Equals` may need VM code.
    fn matches(&self, vm: &BexVm, candidate: Value) -> Option<bool> {
        match self {
            Self::Int(int) => candidate.as_int().map(|candidate| candidate == *int),
            Self::Str(text) => vm
                .as_string(&candidate)
                .ok()
                .map(|candidate| candidate == *text),
        }
    }
}

/// The map's backing store, read without its lock when it is thread-local.
#[allow(unsafe_code)]
fn read_entries(vm: &BexVm, map: Value, thread_local: bool) -> MapReadGuard<'_> {
    let Object::Map(map) = vm.get_object(map.as_object_ptr().unwrap()) else {
        unreachable!("map receiver was validated")
    };
    if thread_local {
        // SAFETY: only the `$local` twins set `thread_local`, and the
        // native backend selects those only for a receiver escape
        // analysis proved thread-local.
        unsafe { map.lock_thread_local() }
    } else {
        map.lock()
    }
}

/// [`read_entries`] for writing.
#[allow(unsafe_code)]
fn write_entries(vm: &BexVm, map: Value, thread_local: bool) -> MapWriteGuard<'_> {
    let Object::Map(map) = vm.get_object(map.as_object_ptr().unwrap()) else {
        unreachable!("map receiver was validated")
    };
    let debt = vm.tlab.alloc_debt();
    if thread_local {
        // SAFETY: as for `read_entries`.
        unsafe { map.lock_mut_thread_local(debt) }
    } else {
        map.lock_mut(debt)
    }
}

/// The operation's result from what it found or replaced.
fn operation_result(operation: Operation, previous: Option<Value>) -> NativeCallResult {
    match operation {
        Operation::Has => NativeCallResult::Done(Value::bool(previous.is_some())),
        Operation::Index if previous.is_none() => {
            NativeCallResult::Error(VmRustFnError::Panic(VmPanic::MapKeyNotFound))
        }
        _ => NativeCallResult::Done(previous.unwrap_or(Value::NULL)),
    }
}

/// Run `operation` in one step under the map's lock when the key is a
/// [`PlainKey`] and so is every key it is compared with: no VM code can run
/// in between, so there is no snapshot to take or epoch to revalidate.
/// `None` when a candidate in the key's bucket needs its own `Equals`; the
/// caller then takes the general search.
fn run_plain(
    vm: &BexVm,
    args: &[Value],
    operation: Operation,
    thread_local: bool,
) -> Option<NativeCallResult> {
    let key = PlainKey::of(vm, args[1])?;
    let hash = key.hash();
    let matches = |candidate: &Value| key.matches(vm, *candidate);
    let previous = match operation {
        Operation::Has | Operation::Get | Operation::Index => {
            let entries = read_entries(vm, args[0], thread_local);
            let found = entries.find_hashed(hash, matches).ok()?;
            entries.get_if_epoch(entries.epoch(), found)
        }
        Operation::Set | Operation::GetOrInsert | Operation::Delete => {
            let mut entries = write_entries(vm, args[0], thread_local);
            let found = entries.find_hashed(hash, matches).ok()?;
            let epoch = entries.epoch();
            match operation {
                Operation::Set => entries.set_if_epoch(epoch, found, hash, args[1], args[2]),
                Operation::GetOrInsert if found.is_none() => entries
                    .set_if_epoch(epoch, None, hash, args[1], args[2])
                    .map(|_| Some(args[2])),
                Operation::Delete => entries.remove_if_epoch(epoch, found),
                _ => entries.get_if_epoch(epoch, found),
            }
        }
    };
    let previous = previous.expect("the search ran under the map's lock at its current epoch");
    Some(operation_result(operation, previous))
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
        if let Some(result) = run_plain(vm, args, operation, thread_local) {
            return result;
        }
        // A plain key whose bucket holds a key with its own `Equals` keeps
        // its native hash, so both searches find the same bucket.
        let native_hash = PlainKey::of(vm, args[1]).map(|key| key.hash());
        let (state, hasher) = if native_hash.is_some() {
            (Value::NULL, None)
        } else {
            let (state, hasher) = super::hasher::new_map_key_state(vm);
            (state, Some(hasher))
        };
        let mut driver = Self {
            map: args[0],
            key: args[1],
            value: args.get(2).copied().unwrap_or(Value::NULL),
            state,
            hasher,
            operation,
            hash: native_hash.unwrap_or(0),
            epoch: 0,
            candidates: Vec::new(),
            cursor: 0,
            hashing: native_hash.is_none(),
            thread_local,
        };
        if native_hash.is_some() {
            driver.snapshot(vm);
            return driver.drive(vm, None);
        }
        let result = super::hashing::hash_value(vm, driver.key, state);
        super::chain(vm, result, Box::new(driver))
    }

    fn entries<'vm>(&self, vm: &'vm BexVm) -> MapReadGuard<'vm> {
        read_entries(vm, self.map, self.thread_local)
    }

    fn entries_mut<'vm>(&self, vm: &'vm BexVm) -> MapWriteGuard<'vm> {
        write_entries(vm, self.map, self.thread_local)
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
            return operation_result(self.operation, previous);
        }
    }
}

impl RootHaver for MapDriver {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            map,
            key,
            value,
            state,
            hasher: _,
            operation: _,
            hash: _,
            epoch: _,
            candidates,
            cursor: _,
            hashing: _,
            thread_local: _,
        } = self;
        map.visit_roots(visitor);
        key.visit_roots(visitor);
        value.visit_roots(visitor);
        state.visit_roots(visitor);
        for (_, key) in candidates {
            key.visit_roots(visitor);
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
