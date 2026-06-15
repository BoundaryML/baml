use std::{collections::HashMap, sync::Arc};

use bex_vm_types::{
    FutureRead, HeapPtr, ValueKind,
    types::{Instance, Object, Value},
};
use indexmap::IndexMap;

use super::{
    BamlPackageBaml, Continuation, NativeCallResult, PackageBamlImpl,
    array::{
        NaturalDomain, compare_natural_values, is_primitive_array_values,
        validate_natural_order_with_vm,
    },
    make_compare_callee,
};
use crate::BexVm;

impl BamlPackageBaml for PackageBamlImpl {
    fn deep_copy(vm: &mut BexVm, value: &Value) -> Value {
        let mut copied_objects = HashMap::new();
        deep_copy_value_recursive(vm, *value, &mut copied_objects)
    }

    fn deep_equals(vm: &BexVm, a: &Value, b: &Value) -> bool {
        let mut visited = HashMap::new();
        deep_equals_recursive(vm, *a, *b, &mut visited)
    }

    /// `baml._float_total_cmp(a, b)` — bit-exact `f64::total_cmp` three-way
    /// comparison backing `Comparable for float`. Kept in lockstep with the
    /// float domain of `compare_natural_values` (the `_rust_sort` fast path)
    /// so the two sort paths can never disagree on a float ordering.
    fn _float_total_cmp(a: f64, b: f64) -> i64 {
        match a.total_cmp(&b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }

    /// `baml._is_primitive_array(arr)` — `Sortable.sort`'s dispatch guard:
    /// whether the first element's runtime tag is a natural-sort primitive
    /// (int/bigint/string/float). Empty arrays report `true`.
    fn _is_primitive_array(vm: &BexVm, arr: &[Value]) -> bool {
        is_primitive_array_values(vm, arr)
    }

    /// `baml._rust_sort(arr)` — the primitive fast path of `Sortable.sort`.
    /// Stable natural-order sort of a homogeneous primitive array, in place
    /// (the receiver's backing `Vec` is sorted or replaced; the returned value
    /// IS the receiver). The comparator is pure Rust — no per-pair yield to
    /// BAML — and the float domain uses `f64::total_cmp`, so no domain throws.
    /// The validation rejections are defensive only: the `_is_primitive_array`
    /// guard plus `T[]` homogeneity make them unreachable from `Sortable.sort`.
    fn _rust_sort(vm: &mut BexVm, arr: &Value) -> NativeCallResult {
        let domain = {
            let values = match vm.as_array(arr) {
                Ok(guard) => guard,
                Err(e) => return NativeCallResult::Error(e.into()),
            };
            if values.len() <= 1 {
                return NativeCallResult::Done(*arr);
            }
            match validate_natural_order_with_vm(vm, "sort", &values) {
                Ok(domain) => domain,
                Err(e) => return NativeCallResult::Error(e),
            }
        };
        if matches!(domain, NaturalDomain::Int) {
            // Int values are tag-only (no heap reads), so the comparator
            // needs no `&BexVm` and the backing Vec sorts truly in place.
            let mut values = match vm.as_array_mut(arr) {
                Ok(guard) => guard,
                Err(e) => return NativeCallResult::Error(e.into()),
            };
            values.sort_by(|left, right| {
                left.as_int()
                    .expect("validated int sort value should be int")
                    .cmp(
                        &right
                            .as_int()
                            .expect("validated int sort value should be int"),
                    )
            });
            return NativeCallResult::Done(*arr);
        }
        // Heap-read domains (float/bigint/string): the comparator needs
        // `&BexVm`, which conflicts with holding the array's mutable guard —
        // sort a snapshot of the backing Vec, then swap it back in. One Vec
        // (re)allocation; still no per-element BAML round trips or re-push.
        let mut values = match vm.as_array(arr) {
            Ok(guard) => guard.to_vec(),
            Err(e) => return NativeCallResult::Error(e.into()),
        };
        values.sort_by(|left, right| compare_natural_values(vm, domain, *left, *right));
        match vm.as_array_mut(arr) {
            Ok(mut guard) => *guard = values,
            Err(e) => return NativeCallResult::Error(e.into()),
        }
        NativeCallResult::Done(*arr)
    }

    /// `baml._compare_shim(a, b)` — the dispatch shim for the `Sortable`
    /// blanket `sort`'s comparator path. Resolves `Comparable.compare` on
    /// `a`'s runtime class and yields to it with `b`; the comparison's `int`
    /// result (or thrown error) is returned straight through. See
    /// `make_compare_callee` for why the sort cannot dispatch `compare`
    /// itself.
    fn _compare_shim(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        let callee = match make_compare_callee(vm, *a) {
            Ok(ptr) => ptr,
            Err(e) => return NativeCallResult::Error(e),
        };
        NativeCallResult::YieldToCall {
            callee,
            args: vec![*b],
            type_args: vec![],
            continuation: Box::new(PassThroughContinuation),
        }
    }
}

/// Returns the callee's result unchanged. Used by `_compare_shim`, whose only
/// job is to dispatch one `compare` call and surface its value.
struct PassThroughContinuation;

impl Continuation for PassThroughContinuation {
    fn call(self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        NativeCallResult::Done(value)
    }
    fn gc_roots(&self) -> Vec<HeapPtr> {
        Vec::new()
    }
    fn apply_forwarding(&mut self, _forwarding: &HashMap<HeapPtr, HeapPtr>) {}
}

fn deep_copy_value_recursive(
    vm: &mut BexVm,
    value: Value,
    copied_objects: &mut HashMap<HeapPtr, HeapPtr>,
) -> Value {
    match value.kind() {
        ValueKind::OmittedArg | ValueKind::Null | ValueKind::Int(_) | ValueKind::Bool(_) => value,

        ValueKind::Object(ptr) => {
            if let Some(&new_ptr) = copied_objects.get(&ptr) {
                return Value::object(new_ptr);
            }

            // Futures are *handles*, not values: a `Future` is the user-
            // visible name for an entry the engine's `FutureManager` writes
            // terminal state into. Even after the future completes, its
            // on-heap representation remains shared mutable state from the
            // runtime's point of view — there is no notion of "the same
            // future, but a copy". Short-circuit before cloning the Object
            // (which would otherwise clone the `Future` struct uselessly).
            if matches!(vm.get_object(ptr), Object::Future(_)) {
                copied_objects.insert(ptr, ptr);
                return Value::object(ptr);
            }

            let object = vm.get_object(ptr).clone();

            let new_ptr = match object {
                Object::Float(f) => vm.tlab.alloc(Object::Float(f)),
                Object::String(s) => vm.tlab.alloc(Object::String(s)),
                Object::Uint8Array(bytes) => vm.tlab.alloc(Object::Uint8Array(bytes)),

                Object::Array(values) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Array(Vec::new().into()));
                    copied_objects.insert(ptr, placeholder_ptr);

                    // Snapshot under the source's lock; the recursive call
                    // re-enters the VM and may take other container locks.
                    let snapshot = values.to_vec();
                    let mut new_values = Vec::with_capacity(snapshot.len());
                    for value in snapshot {
                        new_values.push(deep_copy_value_recursive(vm, value, copied_objects));
                    }

                    // no GC write barrier because it is all in gen0
                    *vm.get_object_mut(placeholder_ptr) = Object::Array(new_values.into());
                    placeholder_ptr
                }

                Object::Map(map) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Map(IndexMap::new().into()));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let snapshot = map.to_index_map();
                    let mut new_map = IndexMap::new();
                    for (key, value) in &snapshot {
                        let new_value = deep_copy_value_recursive(vm, *value, copied_objects);
                        new_map.insert(key.clone(), new_value);
                    }

                    // no GC write barrier because it is all in gen0
                    *vm.get_object_mut(placeholder_ptr) = Object::Map(new_map.into());
                    placeholder_ptr
                }

                Object::Instance(instance) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Instance(Instance::new(
                        instance.class,
                        instance.class_type_args.clone(),
                        Vec::new(),
                    )));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let mut new_fields = Vec::with_capacity(instance.fields.len());
                    for field in instance.field_values() {
                        new_fields.push(deep_copy_value_recursive(vm, field, copied_objects));
                    }

                    let new_instance =
                        Instance::new(instance.class, instance.class_type_args, new_fields);
                    // no GC write barrier because it is all in gen0
                    *vm.get_object_mut(placeholder_ptr) = Object::Instance(new_instance);
                    placeholder_ptr
                }

                // Bigint is behind Arc — clone() is cheap (increments refcount).
                Object::Bigint(arc) => vm.tlab.alloc(Object::Bigint(std::sync::Arc::clone(&arc))),
                Object::Function(f) => vm.tlab.alloc(Object::Function(f)),
                Object::Class(c) => vm.tlab.alloc(Object::Class(c)),
                Object::Enum(e) => vm.tlab.alloc(Object::Enum(e)),
                Object::Variant(v) => vm.tlab.alloc(Object::Variant(v)),
                Object::RustData(arc) => vm.tlab.alloc(Object::RustData(Arc::clone(&arc))),
                // `Object::Future(_)` is short-circuited above; it can't
                // reach this match arm.
                Object::Future(_) => unreachable!("Future short-circuited above"),
                Object::UnscheduledFuture(f) => vm.tlab.alloc(Object::UnscheduledFuture(f)),
                Object::Collector(c) => vm.tlab.alloc(Object::Collector(c)),
                Object::Type(ty) => vm.tlab.alloc(Object::Type(ty)),
                // Closures, bound methods, and cells are shallow-copied: the captured
                // state is shared by design (mutation semantics).
                Object::Closure(c) => vm.tlab.alloc(Object::Closure(c)),
                Object::BoundMethod(bm) => vm.tlab.alloc(Object::BoundMethod(bm)),
                Object::GenericFunction(gf) => vm.tlab.alloc(Object::GenericFunction(gf)),
                // `HostClosure` is a value-type wrapper around an
                // `Arc<HostValueArc>`; cloning the Arc is cheap and matches
                // the closure semantics (shared handle).
                Object::HostClosure(hc) => vm.tlab.alloc(Object::HostClosure(hc)),
                Object::Cell(cell) => vm.tlab.alloc(Object::Cell(cell)),
                // A mock is a runtime testing handle. `m` here is an owned clone
                // (see the `.clone()` at the top of this match), and `Mock`'s
                // Clone copies the atomics, so the copy is an independent Mock
                // with its own replacement slot and counter — not shared state.
                Object::Mock(m) => vm.tlab.alloc(Object::Mock(m)),
                Object::InterfaceMethodRef(s) => vm.tlab.alloc(Object::InterfaceMethodRef(s)),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(kind) => vm.tlab.alloc(Object::Sentinel(kind)),
            };

            copied_objects.entry(ptr).or_insert(new_ptr);

            Value::object(new_ptr)
        }
    }
}

#[allow(clippy::float_cmp)]
fn deep_equals_recursive(
    vm: &BexVm,
    a: Value,
    b: Value,
    visited: &mut HashMap<(HeapPtr, HeapPtr), bool>,
) -> bool {
    match (a.kind(), b.kind()) {
        (ValueKind::OmittedArg, ValueKind::OmittedArg) => true,
        (ValueKind::Null, ValueKind::Null) => true,
        (ValueKind::Int(a), ValueKind::Int(b)) => a == b,
        (ValueKind::Bool(a), ValueKind::Bool(b)) => a == b,

        (ValueKind::Object(a_ptr), ValueKind::Object(b_ptr)) => {
            if a_ptr == b_ptr {
                return true;
            }

            let key = if a_ptr < b_ptr {
                (a_ptr, b_ptr)
            } else {
                (b_ptr, a_ptr)
            };

            if let Some(&result) = visited.get(&key) {
                return result;
            }

            visited.insert(key, true);

            let result = match (vm.get_object(a_ptr), vm.get_object(b_ptr)) {
                (Object::Float(a), Object::Float(b)) => (a.is_nan() && b.is_nan()) || a == b,
                (Object::String(a), Object::String(b)) => a == b,
                (Object::Uint8Array(a), Object::Uint8Array(b)) => {
                    let a_snap = a.to_vec();
                    let b_snap = b.to_vec();
                    a_snap == b_snap
                }

                // Different `Arc`s with the same numeric value must compare equal.
                (Object::Bigint(a), Object::Bigint(b)) => a == b,

                (Object::Array(a_values), Object::Array(b_values)) => {
                    // Snapshot under each lock before recursing; deep_equals
                    // is mutator code so we cannot hold the lock across
                    // recursive lookups that may also lock containers.
                    let a_snap = a_values.to_vec();
                    let b_snap = b_values.to_vec();
                    a_snap.len() == b_snap.len()
                        && a_snap
                            .iter()
                            .zip(b_snap.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                }

                (Object::Map(a_map), Object::Map(b_map)) => {
                    let a_snap = a_map.to_index_map();
                    let b_snap = b_map.to_index_map();
                    a_snap.len() == b_snap.len()
                        && a_snap.iter().all(|(key, a_val)| {
                            b_snap.get(key).is_some_and(|b_val| {
                                deep_equals_recursive(vm, *a_val, *b_val, visited)
                            })
                        })
                }

                (Object::Instance(a_inst), Object::Instance(b_inst)) => {
                    a_inst.class == b_inst.class
                        && a_inst.fields.len() == b_inst.fields.len()
                        && a_inst
                            .fields
                            .iter()
                            .zip(b_inst.fields.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, a.load(), b.load(), visited))
                }

                (Object::Variant(a_var), Object::Variant(b_var)) => {
                    a_var.enm == b_var.enm && a_var.index == b_var.index
                }

                (Object::Type(a_ty), Object::Type(b_ty)) => a_ty == b_ty,

                (Object::Enum(a_enum), Object::Enum(b_enum)) => {
                    a_enum.name == b_enum.name
                        && a_enum.variants.len() == b_enum.variants.len()
                        && a_enum
                            .variants
                            .iter()
                            .zip(b_enum.variants.iter())
                            .all(|(a, b)| a.name == b.name)
                }

                (Object::Class(a_class), Object::Class(b_class)) => {
                    a_class.name == b_class.name
                        && a_class.fields.len() == b_class.fields.len()
                        && a_class
                            .fields
                            .iter()
                            .zip(b_class.fields.iter())
                            .all(|(a, b)| a.name == b.name)
                }

                (Object::Function(_), Object::Function(_)) => a_ptr == b_ptr,

                // GenericFunction values compare structurally (same base
                // function + same type args). The interned/pooled case already
                // short-circuits via the `a_ptr == b_ptr` fast path above; this
                // arm covers non-pooled copies (e.g. from `baml.deep_copy`).
                (Object::GenericFunction(a_gf), Object::GenericFunction(b_gf)) => {
                    a_gf.function == b_gf.function && a_gf.type_args == b_gf.type_args
                }

                (Object::Future(a_fut), Object::Future(b_fut)) => {
                    match (a_fut.read(), b_fut.read()) {
                        (FutureRead::Ready(a_val), FutureRead::Ready(b_val)) => {
                            deep_equals_recursive(vm, a_val, b_val, visited)
                        }
                        (FutureRead::Pending(a_id), FutureRead::Pending(b_id)) => a_id == b_id,
                        _ => false,
                    }
                }

                (Object::UnscheduledFuture(a_fut), Object::UnscheduledFuture(b_fut)) => {
                    a_fut.closure == b_fut.closure
                        && a_fut.name == b_fut.name
                        && a_fut.config == b_fut.config
                }

                _ => false,
            };

            visited.insert(key, result);
            result
        }

        _ => false,
    }
}
