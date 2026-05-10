use std::collections::HashMap;

use bex_vm_types::{HeapPtr, Object, types::Value};

use super::{BamlClassArray, Continuation, NativeCallResult, PackageBamlImpl, make_to_json_callee};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// Equality matching BAML's `==` operator: by-value for primitives, by-content
/// for strings / uint8arrays / variants, by-`HeapPtr` reference for arrays /
/// maps / class instances. Used by `includes`, `index_of`, `last_index_of`.
fn baml_eq(vm: &BexVm, a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(la), Value::Object(rb)) => {
            // Try content comparison for the types where `==` is content-based;
            // fall through to HeapPtr equality otherwise.
            let lobj = vm.get_object(*la);
            let robj = vm.get_object(*rb);
            match (lobj, robj) {
                (Object::String(_), Object::String(_)) => {
                    match (vm.as_string(a), vm.as_string(b)) {
                        (Ok(ls), Ok(rs)) => ls == rs,
                        _ => false,
                    }
                }
                (Object::Uint8Array(_), Object::Uint8Array(_)) => {
                    match (vm.as_uint8array(a), vm.as_uint8array(b)) {
                        (Ok(lb), Ok(rb)) => lb == rb,
                        _ => false,
                    }
                }
                (Object::Variant(lv), Object::Variant(rv)) => {
                    lv.enm == rv.enm && lv.index == rv.index
                }
                _ => la == rb,
            }
        }
        _ => a == b,
    }
}

// ── GC bookkeeping helpers ────────────────────────────────────────────────────
//
// All callback-driven continuations capture an `f_ptr` (the callable) plus
// some Values they're operating on. These helpers collect the heap roots and
// apply forwarding without each continuation re-implementing the same loops.

fn collect_value_roots(values: &[Value], roots: &mut Vec<HeapPtr>) {
    for v in values {
        if let Value::Object(p) = v {
            roots.push(*p);
        }
    }
}

fn forward_values(values: &mut [Value], forwarding: &HashMap<HeapPtr, HeapPtr>) {
    for v in values {
        if let Value::Object(ptr) = v {
            if let Some(&new) = forwarding.get(ptr) {
                *ptr = new;
            }
        }
    }
}

fn forward_ptr(ptr: &mut HeapPtr, forwarding: &HashMap<HeapPtr, HeapPtr>) {
    if let Some(&new) = forwarding.get(ptr) {
        *ptr = new;
    }
}

/// Extract the callback `HeapPtr` from a `Value::Object`, or return a
/// type-error `NativeCallResult` for any other variant.
fn extract_callable(vm: &BexVm, f: &Value) -> Result<HeapPtr, NativeCallResult> {
    if let Value::Object(p) = f {
        Ok(*p)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Object(bex_vm_types::ObjectType::Any),
            got: vm.type_of(f),
        }))
    }
}

fn expect_bool(vm: &BexVm, value: &Value) -> Result<bool, NativeCallResult> {
    if let Value::Bool(b) = value {
        Ok(*b)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Bool,
            got: vm.type_of(value),
        }))
    }
}

// Boilerplate macro for the standard `Continuation` GC impls. Captures named
// fields by kind:
//   `f_ptr: <name>`         — a single `HeapPtr` field (optional).
//   `values: [<name>, ...]` — one or more `Vec<Value>` / `[Value]` fields.
macro_rules! gc_impl_array {
    (f_ptr: $f_ptr:ident, values: [$($values:ident),+ $(,)?] $(,)?) => {
        fn gc_roots(&self) -> Vec<HeapPtr> {
            let mut roots = vec![self.$f_ptr];
            $(collect_value_roots(&self.$values, &mut roots);)+
            roots
        }
        fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
            forward_ptr(&mut self.$f_ptr, forwarding);
            $(forward_values(&mut self.$values, forwarding);)+
        }
    };
    (values: [$($values:ident),+ $(,)?] $(,)?) => {
        fn gc_roots(&self) -> Vec<HeapPtr> {
            let mut roots = Vec::new();
            $(collect_value_roots(&self.$values, &mut roots);)+
            roots
        }
        fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
            $(forward_values(&mut self.$values, forwarding);)+
        }
    };
}

// ── map ───────────────────────────────────────────────────────────────────────

/// Continuation for `Array.map`. Accumulates the mapped results one element at
/// a time, yielding to the callback for each element and completing when the
/// last result has been collected.
struct MapContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    results: Vec<Value>,
}

impl Continuation for MapContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(vm.alloc_array(self.results));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array, results]);
}

// ── some / every ──────────────────────────────────────────────────────────────
//
// Both walk the array left-to-right, short-circuiting. `some` stops at the
// first true (returning true); `every` stops at the first false (returning
// false). Empty array → `some = false`, `every = true`.

struct SomeContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
}

impl Continuation for SomeContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, &value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            return NativeCallResult::Done(Value::Bool(true));
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::Bool(false));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

struct EveryContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
}

impl Continuation for EveryContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, &value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if !b {
            return NativeCallResult::Done(Value::Bool(false));
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::Bool(true));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── find / find_index ─────────────────────────────────────────────────────────

struct FindContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    /// `true` to return the matching element, `false` to return its index.
    return_value: bool,
}

impl Continuation for FindContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, &value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            let result = if self.return_value {
                self.array[self.idx]
            } else {
                #[allow(clippy::cast_possible_wrap)]
                Value::Int(self.idx as i64)
            };
            return NativeCallResult::Done(result);
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::Null);
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── find_last / find_last_index ───────────────────────────────────────────────

struct FindLastContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    /// Index of the element whose predicate result we await. We descend from
    /// `len - 1` toward 0; on reaching 0 with a `false` result, return null.
    idx: usize,
    /// `true` to return the matching element, `false` to return its index.
    return_value: bool,
}

impl Continuation for FindLastContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, &value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            let result = if self.return_value {
                self.array[self.idx]
            } else {
                #[allow(clippy::cast_possible_wrap)]
                Value::Int(self.idx as i64)
            };
            return NativeCallResult::Done(result);
        }
        if self.idx == 0 {
            return NativeCallResult::Done(Value::Null);
        }
        self.idx -= 1;
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── reduce ────────────────────────────────────────────────────────────────────

struct ReduceContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    /// Index of the element we just paired with the accumulator and yielded
    /// to the reducer. The reducer's return value will be the new accumulator.
    idx: usize,
}

impl Continuation for ReduceContinuation {
    fn call(mut self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        // `value` is the new accumulator returned by the reducer.
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(value);
        }
        let next_elem = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![value, next_elem],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── flat_map ──────────────────────────────────────────────────────────────────

struct FlatMapContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    /// Flattened results accumulated so far.
    results: Vec<Value>,
}

impl Continuation for FlatMapContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        // `value` is the inner array returned for `array[idx]`. Append its
        // contents to the accumulated results.
        let inner = match vm.as_array(&value) {
            Ok(slice) => slice.to_vec(),
            Err(e) => return NativeCallResult::Error(e.into()),
        };
        self.results.extend(inner);
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(vm.alloc_array(self.results));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array, results]);
}

// ─── Array.to_json continuation ──────────────────────────────────────────────

/// Continuation for `Array.to_json`. Dispatches `v.to_json()` for each element
/// in order, accumulating json results and finalizing into a `json[]` value.
struct ToJsonContinuation {
    /// The original array elements.
    array: Vec<Value>,
    /// Index of the element whose `to_json()` callback result we are about to receive.
    /// Starts at 0 (we yield for index 0 before constructing the continuation).
    idx: usize,
    /// Accumulated json results so far (does not include in-flight result).
    results: Vec<Value>,
}

impl Continuation for ToJsonContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        self.idx += 1;

        if self.idx >= self.array.len() {
            return NativeCallResult::Done(vm.alloc_array(self.results));
        }

        let next_val = self.array[self.idx];
        match make_to_json_callee(vm, next_val) {
            Ok(callee) => NativeCallResult::YieldToCall {
                callee,
                args: vec![],
                type_args: vec![],
                continuation: self,
            },
            Err(e) => NativeCallResult::Error(e),
        }
    }

    gc_impl_array!(values: [array, results]);
}

impl BamlClassArray for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(array: &[Value]) -> i64 {
        array.len() as i64
    }

    #[allow(clippy::cast_possible_wrap)]
    fn push(array: &mut Vec<Value>, item: &Value) -> i64 {
        array.push(*item);
        array.len() as i64
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn at(array: &[Value], index: i64) -> Option<Value> {
        array.get(index as usize).copied()
    }

    fn concat(array: &[Value], other: &[Value]) -> Vec<Value> {
        array.iter().chain(other.iter()).copied().collect()
    }

    fn pop(array: &mut Vec<Value>) -> Option<Value> {
        array.pop()
    }

    fn reverse(array: &[Value]) -> Vec<Value> {
        let mut result = array.to_vec();
        result.reverse();
        result
    }

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn slice(array: &[Value], start: i64, end: i64) -> Vec<Value> {
        let len = array.len() as i64;
        let start = start.max(0).min(len) as usize;
        let end = end.max(0).min(len) as usize;
        let end = end.max(start);
        array[start..end].to_vec()
    }

    fn join(vm: &BexVm, array: &[Value], separator: &str) -> String {
        array
            .iter()
            .map(|v| vm.as_string(v).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(separator)
    }

    #[allow(clippy::unused_unit)]
    fn clear(array: &mut Vec<Value>) -> () {
        array.clear();
    }

    fn shift(array: &mut Vec<Value>) -> Option<Value> {
        if array.is_empty() {
            None
        } else {
            Some(array.remove(0))
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    fn unshift(array: &mut Vec<Value>, item: &Value) -> i64 {
        array.insert(0, *item);
        array.len() as i64
    }

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn insert(array: &mut Vec<Value>, item: &Value, idx: i64) -> Result<i64, VmRustFnError> {
        let len = array.len();
        let Ok(idx_usize) = usize::try_from(idx) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.insert: idx ({idx}) is negative"),
            }
            .into());
        };
        if idx_usize > len {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "array.insert: idx ({idx_usize}) is beyond the array length ({len})"
                ),
            }
            .into());
        }
        array.insert(idx_usize, *item);
        Ok(array.len() as i64)
    }

    #[allow(
        clippy::unused_unit,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    fn splice(
        array: &mut Vec<Value>,
        start: i64,
        count: i64,
        replace: &[Value],
    ) -> Result<(), VmRustFnError> {
        let len = array.len();
        let Ok(start_usize) = usize::try_from(start) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.splice: start ({start}) is negative"),
            }
            .into());
        };
        if start_usize > len {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "array.splice: start ({start_usize}) is beyond the array length ({len})"
                ),
            }
            .into());
        }
        let Ok(count_usize) = usize::try_from(count) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.splice: count ({count}) is negative"),
            }
            .into());
        };
        let end = start_usize.saturating_add(count_usize).min(len);
        array.splice(start_usize..end, replace.iter().copied());
        Ok(())
    }

    fn map(vm: &mut BexVm, array: &[Value], f: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, f) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(vm.alloc_array(vec![]));
        }
        let first_arg = array[0];
        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(MapContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
            }),
        }
    }

    fn to_json(vm: &mut BexVm, array: &[Value]) -> NativeCallResult {
        if array.is_empty() {
            return NativeCallResult::Done(vm.alloc_array(vec![]));
        }

        let array = array.to_vec();
        let first_val = array[0];
        let callee = match make_to_json_callee(vm, first_val) {
            Ok(c) => c,
            Err(e) => return NativeCallResult::Error(e),
        };

        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(ToJsonContinuation {
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
            }),
        }
    }

    // ── Predicate scans ───────────────────────────────────────────────────────

    fn some(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::Bool(false));
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(SomeContinuation {
                f_ptr,
                array,
                idx: 0,
            }),
        }
    }

    fn every(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::Bool(true));
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(EveryContinuation {
                f_ptr,
                array,
                idx: 0,
            }),
        }
    }

    fn find(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::Null);
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindContinuation {
                f_ptr,
                array,
                idx: 0,
                return_value: true,
            }),
        }
    }

    fn find_index(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::Null);
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindContinuation {
                f_ptr,
                array,
                idx: 0,
                return_value: false,
            }),
        }
    }

    fn find_last(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::Null);
        }
        let last_idx = array.len() - 1;
        let first_arg = array[last_idx];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindLastContinuation {
                f_ptr,
                array,
                idx: last_idx,
                return_value: true,
            }),
        }
    }

    fn find_last_index(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::Null);
        }
        let last_idx = array.len() - 1;
        let first_arg = array[last_idx];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindLastContinuation {
                f_ptr,
                array,
                idx: last_idx,
                return_value: false,
            }),
        }
    }

    fn includes(vm: &BexVm, array: &[Value], item: &Value) -> bool {
        array.iter().any(|v| baml_eq(vm, v, item))
    }

    #[allow(clippy::cast_possible_wrap)]
    fn index_of(vm: &BexVm, array: &[Value], item: &Value) -> Option<i64> {
        array
            .iter()
            .position(|v| baml_eq(vm, v, item))
            .map(|i| i as i64)
    }

    #[allow(clippy::cast_possible_wrap)]
    fn last_index_of(vm: &BexVm, array: &[Value], item: &Value) -> Option<i64> {
        array
            .iter()
            .rposition(|v| baml_eq(vm, v, item))
            .map(|i| i as i64)
    }

    fn reduce(
        vm: &mut BexVm,
        array: &[Value],
        reducer: &Value,
        initial: &Value,
    ) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, reducer) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(*initial);
        }
        let first_elem = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![*initial, first_elem],
            type_args: vec![],
            continuation: Box::new(ReduceContinuation {
                f_ptr,
                array,
                idx: 0,
            }),
        }
    }

    fn flat_map(vm: &mut BexVm, array: &[Value], f: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, f) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(vm.alloc_array(vec![]));
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FlatMapContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::new(),
            }),
        }
    }
}
