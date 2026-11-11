use std::sync::Arc;

use bex_vm_types::{
    EntryId, MapRef, RootHaver, RootVisitor, Value,
    layout::{Elem, Fault, ListOf, Lists, Maps, ObjPtr, StrKey, Strings},
    object_format::Access,
};

use super::{BamlClassMap, Continuation, NativeCallResult, PackageBamlImpl, value_model::Locality};
use crate::{BexVm, VmPanic, array_index::int_len, errors::VmRustFnError};

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
    hasher: Arc<super::hasher::HasherState>,
    operation: Operation,
    hash: u64,
    epoch: u64,
    candidates: Vec<(EntryId, Value)>,
    cursor: usize,
    /// The first callback finishes hashing the key; later ones compare it.
    hashing: bool,
    /// The map is thread-local: escape analysis selected a `$local` twin.
    thread_local: bool,
}

impl MapDriver {
    /// Look `args[1]` up with the interpreter's hashing and equality, which
    /// may run user code. String keys never get here: the shared bodies
    /// handle them, and the generated glue defers only other keys.
    fn start(
        vm: &mut BexVm,
        args: &[Value],
        operation: Operation,
        thread_local: bool,
    ) -> NativeCallResult {
        if let Err(error) = vm.as_map(&args[0]) {
            return error.into();
        }
        let (state, hasher) = super::hasher::new_state(vm);
        let driver = Self {
            map: args[0],
            key: args[1],
            value: args.get(2).copied().unwrap_or(Value::NULL),
            state,
            hasher,
            operation,
            hash: 0,
            epoch: 0,
            candidates: Vec::new(),
            cursor: 0,
            hashing: true,
            thread_local,
        };
        let result = super::hashing::hash_value(vm, driver.key, state);
        super::chain(vm, result, Box::new(driver))
    }

    /// The map's access: without its lock when it is thread-local, which
    /// only the `$local` twins set, for a receiver escape analysis proved
    /// thread-local.
    fn access(&self) -> Access {
        if self.thread_local {
            Access::ThreadLocal
        } else {
            Access::Shared
        }
    }

    fn map(&self, vm: &BexVm) -> MapRef {
        vm.expect_map(
            self.map
                .as_object_ptr()
                .expect("map receiver was validated"),
        )
    }

    /// The map held for reading and writing.
    fn entries<'vm>(&self, vm: &'vm mut BexVm) -> crate::MapEdit<'vm> {
        let map = self.map(vm);
        crate::MapEdit::new(vm, map, self.access())
    }

    fn snapshot(&mut self, vm: &BexVm) {
        let (epoch, candidates) = vm.map_bucket(self.map(vm), self.access(), self.hash);
        self.epoch = epoch;
        self.candidates = candidates;
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
                let result = super::ops::equals_structural_default(vm, key, self.key);
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
                    .entries(vm)
                    .set_if_epoch(self.epoch, matched, self.hash, self.key, self.value),
                Operation::GetOrInsert if matched.is_none() => self
                    .entries(vm)
                    .set_if_epoch(self.epoch, None, self.hash, self.key, self.value)
                    .map(|_| Some(self.value)),
                Operation::Delete => self.entries(vm).remove_if_epoch(self.epoch, matched),
                _ => self.entries(vm).get_if_epoch(self.epoch, matched),
            };
            let Ok(previous) = result else {
                self.snapshot(vm);
                matched = None;
                continue;
            };
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
            self.hash = super::hasher::finish_state(&self.hasher);
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

/// Every `Map` builtin's glue is generated over the shared bodies below.
impl BamlClassMap for PackageBamlImpl {}

// ── Bodies shared by the interpreter and the shared-format adapters ──────
//
// One generic body per builtin over the value representation `E`, with
// string keys. The interpreter's glue runs them at `E = Value` and defers
// any other key to the `*_yielding` paths, whose hashing may run user code.
pub(crate) mod shared {
    #![allow(
        clippy::unnecessary_wraps,
        reason = "one calling convention for every body: generated glue and adapters call \
                  each `fn(&mut R, ...) -> Result<_, Fault>` alike"
    )]

    use super::{
        BexVm, Elem, Fault, ListOf, Lists, Locality, MapDriver, Maps, NativeCallResult, ObjPtr,
        Operation, StrKey, Strings, Value, int_len,
    };

    fn str_key<R: Strings + ?Sized>(rt: &R, key: ObjPtr) -> StrKey {
        StrKey {
            ptr: key,
            hash: rt.str_hash(key),
        }
    }

    pub(crate) fn baml_map_length<R: Maps<E>, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
    ) -> Result<i64, Fault> {
        Ok(int_len(rt.map_len(map)))
    }

    pub(crate) fn baml_map_has<R: Maps<E> + Strings, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
        key: ObjPtr,
    ) -> Result<bool, Fault> {
        Ok(Maps::<E>::map_get(rt, map, str_key(rt, key)).is_some())
    }

    pub(crate) fn baml_map_get<R: Maps<E> + Strings, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
        key: ObjPtr,
    ) -> Result<Option<E>, Fault> {
        Ok(rt.map_get(map, str_key(rt, key)))
    }

    pub(crate) fn baml_map_index<R: Maps<E> + Strings, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
        key: ObjPtr,
    ) -> Result<E, Fault> {
        rt.map_get(map, str_key(rt, key))
            .ok_or(Fault::MapKeyNotFound)
    }

    /// Inserts or replaces; returns the previous value.
    pub(crate) fn baml_map_set<R: Maps<E> + Strings, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
        key: ObjPtr,
        value: E,
    ) -> Result<Option<E>, Fault> {
        let key = str_key(rt, key);
        Ok(rt.map_set(map, key, value))
    }

    pub(crate) fn baml_map_delete<R: Maps<E> + Strings, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
        key: ObjPtr,
    ) -> Result<Option<E>, Fault> {
        let key = str_key(rt, key);
        Ok(rt.map_remove(map, key))
    }

    pub(crate) fn baml_map_get_or_insert<R: Maps<E> + Strings, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
        key: ObjPtr,
        default: E,
    ) -> Result<E, Fault> {
        let key = str_key(rt, key);
        if let Some(existing) = rt.map_get(map, key) {
            return Ok(existing);
        }
        rt.map_set(map, key, default);
        Ok(default)
    }

    pub(crate) fn baml_map_clear<R: Maps<E>, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
    ) -> Result<(), Fault> {
        rt.map_clear(map);
        Ok(())
    }

    pub(crate) fn baml_map_keys<R: Maps<E>, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        Ok(rt.map_key_list(map))
    }

    pub(crate) fn baml_map_values<R: Maps<E> + Lists<E>, E: Elem>(
        rt: &mut R,
        map: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        let values = rt.map_values(map);
        Ok(rt.list_alloc(ListOf::ValuesOf(map), &values))
    }

    /// The yielding paths for keys only the interpreter can hash.
    macro_rules! yielding {
        ($($name:ident => $operation:ident),* $(,)?) => {$(
            pub(crate) fn $name(vm: &mut BexVm, args: &[Value], locality: Locality) -> NativeCallResult {
                MapDriver::start(vm, args, Operation::$operation, locality == Locality::ThreadLocal)
            }
        )*};
    }

    yielding! {
        baml_map_has_yielding => Has,
        baml_map_get_yielding => Get,
        baml_map_index_yielding => Index,
        baml_map_set_yielding => Set,
        baml_map_delete_yielding => Delete,
        baml_map_get_or_insert_yielding => GetOrInsert,
    }
}
