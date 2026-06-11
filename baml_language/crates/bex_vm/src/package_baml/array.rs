use std::{borrow::Cow, cmp::Ordering, collections::HashMap};

use bex_heap::TlabHolder;
use bex_vm_types::{HeapPtr, Object, ObjectType, types::Value};
use num_bigint::BigInt;

use super::{BamlClassArray, Continuation, NativeCallResult, PackageBamlImpl, make_to_json_callee};
use crate::{
    BexVm, VmPanic,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// Equality matching BAML's `==` operator: by-value for primitives, by-content
/// for strings / uint8arrays / variants, by-`HeapPtr` reference for arrays /
/// maps / class instances. Used by `includes`, `index_of`, `last_index_of`.
fn baml_eq(vm: &BexVm, a: Value, b: Value) -> bool {
    if let (Some(la), Some(rb)) = (a.as_object_ptr(), b.as_object_ptr()) {
        // Try content comparison for the types where `==` is content-based;
        // fall through to HeapPtr equality otherwise.
        let lobj = vm.get_object(la);
        let robj = vm.get_object(rb);
        match (lobj, robj) {
            (Object::String(_), Object::String(_)) => match (vm.as_string(&a), vm.as_string(&b)) {
                (Ok(ls), Ok(rs)) => ls == rs,
                _ => false,
            },
            (Object::Uint8Array(_), Object::Uint8Array(_)) => {
                match (
                    vm.as_uint8array(&a).map(|bytes| bytes.to_vec()),
                    vm.as_uint8array(&b).map(|bytes| bytes.to_vec()),
                ) {
                    (Ok(lb), Ok(rb)) => lb == rb,
                    _ => false,
                }
            }
            (Object::Variant(lv), Object::Variant(rv)) => lv.enm == rv.enm && lv.index == rv.index,
            // Heap-boxed floats compare by content (the post-tagged-pointer
            // encoding allocates a fresh `Object::Float` per float, so two
            // semantically-equal floats land at distinct `HeapPtr`s and
            // would otherwise miss the reference-equality fallback).
            (Object::Float(lf), Object::Float(rf)) => lf == rf,
            _ => la == rb,
        }
    } else {
        a == b
    }
}

// ── GC bookkeeping helpers ────────────────────────────────────────────────────
//
// All callback-driven continuations capture an `f_ptr` (the callable) plus
// some Values they're operating on. These helpers collect the heap roots and
// apply forwarding without each continuation re-implementing the same loops.

fn collect_value_roots(values: &[Value], roots: &mut Vec<HeapPtr>) {
    for v in values {
        if let Some(p) = v.as_object_ptr() {
            roots.push(p);
        }
    }
}

fn forward_values(values: &mut [Value], forwarding: &HashMap<HeapPtr, HeapPtr>) {
    for v in values {
        if let Some(ptr) = v.as_object_ptr() {
            if let Some(&new) = forwarding.get(&ptr) {
                *v = Value::object(new);
            }
        }
    }
}

fn forward_ptr(ptr: &mut HeapPtr, forwarding: &HashMap<HeapPtr, HeapPtr>) {
    if let Some(&new) = forwarding.get(ptr) {
        *ptr = new;
    }
}

/// Extract the callback `HeapPtr` from a `Value` carrying a heap
/// object, or return a type-error `NativeCallResult` for any other
/// variant.
fn extract_callable(vm: &BexVm, f: Value) -> Result<HeapPtr, NativeCallResult> {
    if let Some(p) = f.as_object_ptr() {
        Ok(p)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Object(bex_vm_types::ObjectType::Any),
            got: vm.type_of(&f),
        }))
    }
}

fn expect_bool(vm: &BexVm, value: Value) -> Result<bool, NativeCallResult> {
    if let Some(b) = value.as_bool() {
        Ok(b)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Bool,
            got: vm.type_of(&value),
        }))
    }
}

fn expect_int(vm: &BexVm, value: Value) -> Result<i64, NativeCallResult> {
    if let Some(i) = value.as_int() {
        Ok(i)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Int,
            got: vm.type_of(&value),
        }))
    }
}

// ── Natural-order sort machinery (`baml._rust_sort` fast path) ───────────────
//
// `Sortable.sort` routes homogeneous primitive arrays (int/bigint/string/
// float) here via the `_is_primitive_array` guard; everything else goes
// through `sort_by` + `_compare_shim`. The float domain orders by
// `f64::total_cmp` — a total order over all doubles including NaN — kept
// bit-exact with `_float_total_cmp` (which backs `Comparable for float`), so
// the fast path and the comparator path can never disagree on an ordering.
// The mixed-domain / null / non-primitive rejections below are defensive:
// `_rust_sort` is only reached for arrays the type system already proved
// homogeneous primitive.

#[derive(Clone, Copy)]
pub(super) enum NaturalDomain {
    Int,
    Float,
    Bigint,
    String,
}

#[derive(Clone, Copy)]
enum NaturalKind {
    Int,
    Float,
    Bigint,
    String,
}

fn value_type_name(vm: &BexVm, value: Value) -> String {
    if value.is_null() {
        return "null".to_string();
    }
    if value.as_int().is_some() {
        return "int".to_string();
    }
    if value.as_bool().is_some() {
        return "bool".to_string();
    }
    if value.is_omitted() {
        return "omitted".to_string();
    }
    if let Some(ptr) = value.as_object_ptr() {
        return ObjectType::of(vm.get_object(ptr)).to_string();
    }
    "unknown".to_string()
}

fn invalid_sort(context: &str, message: impl Into<String>) -> VmRustFnError {
    VmBamlError::InvalidArgument {
        message: format!("{context}: {}", message.into()),
    }
    .into()
}

fn natural_kind(vm: &BexVm, context: &str, value: Value) -> Result<NaturalKind, VmRustFnError> {
    if value.is_null() {
        return Err(invalid_sort(
            context,
            "natural ordering does not support null",
        ));
    }
    if value.as_int().is_some() {
        return Ok(NaturalKind::Int);
    }
    let Some(ptr) = value.as_object_ptr() else {
        return Err(invalid_sort(
            context,
            format!(
                "natural ordering does not support {}",
                value_type_name(vm, value)
            ),
        ));
    };
    match vm.get_object(ptr) {
        // NaN is *not* rejected: the float domain orders by `total_cmp`,
        // which gives NaN a defined position.
        Object::Float(_) => Ok(NaturalKind::Float),
        Object::Bigint(_) => Ok(NaturalKind::Bigint),
        Object::String(_) => Ok(NaturalKind::String),
        _ => Err(invalid_sort(
            context,
            format!(
                "natural ordering does not support {}",
                value_type_name(vm, value)
            ),
        )),
    }
}

pub(super) fn validate_natural_order_with_vm(
    vm: &BexVm,
    context: &str,
    values: &[Value],
) -> Result<NaturalDomain, VmRustFnError> {
    let mut has_int = false;
    let mut has_float = false;
    let mut has_bigint = false;
    let mut has_string = false;

    for value in values {
        match natural_kind(vm, context, *value)? {
            NaturalKind::Int => has_int = true,
            NaturalKind::Float => has_float = true,
            NaturalKind::Bigint => has_bigint = true,
            NaturalKind::String => has_string = true,
        }
    }

    let numeric_domains = usize::from(has_float) + usize::from(has_bigint);
    if has_string && (has_int || has_float || has_bigint) {
        return Err(invalid_sort(
            context,
            "natural ordering does not support mixing string with numeric values",
        ));
    }
    if has_float && has_bigint {
        return Err(invalid_sort(
            context,
            "natural ordering does not support mixing float and bigint values",
        ));
    }
    if has_string {
        Ok(NaturalDomain::String)
    } else if numeric_domains == 0 {
        Ok(NaturalDomain::Int)
    } else if has_float {
        Ok(NaturalDomain::Float)
    } else {
        Ok(NaturalDomain::Bigint)
    }
}

fn value_as_float_for_sort(vm: &BexVm, value: Value) -> f64 {
    if let Some(i) = value.as_int() {
        #[allow(clippy::cast_precision_loss)]
        return i as f64;
    }
    let ptr = value
        .as_object_ptr()
        .expect("validated float sort value should be object-backed");
    match vm.get_object(ptr) {
        Object::Float(f) => *f,
        _ => unreachable!("validated float sort value should be float or int"),
    }
}

fn value_as_bigint_cow_for_sort(vm: &BexVm, value: Value) -> Cow<'_, BigInt> {
    if let Some(i) = value.as_int() {
        return Cow::Owned(BigInt::from(i));
    }
    let ptr = value
        .as_object_ptr()
        .expect("validated bigint sort value should be object-backed");
    match vm.get_object(ptr) {
        Object::Bigint(arc) => Cow::Borrowed(arc.as_ref()),
        _ => unreachable!("validated bigint sort value should be bigint or int"),
    }
}

fn value_as_string_for_sort(vm: &BexVm, value: Value) -> &bex_str::BexStr {
    let ptr = value
        .as_object_ptr()
        .expect("validated string sort value should be object-backed");
    match vm.get_object(ptr) {
        Object::String(s) => s,
        _ => unreachable!("validated string sort value should be string"),
    }
}

pub(super) fn compare_natural_values(
    vm: &BexVm,
    domain: NaturalDomain,
    left: Value,
    right: Value,
) -> Ordering {
    match domain {
        NaturalDomain::Int => left
            .as_int()
            .expect("validated int sort value should be int")
            .cmp(
                &right
                    .as_int()
                    .expect("validated int sort value should be int"),
            ),
        NaturalDomain::Float => {
            value_as_float_for_sort(vm, left).total_cmp(&value_as_float_for_sort(vm, right))
        }
        NaturalDomain::Bigint => {
            value_as_bigint_cow_for_sort(vm, left).cmp(&value_as_bigint_cow_for_sort(vm, right))
        }
        NaturalDomain::String => {
            value_as_string_for_sort(vm, left).cmp(value_as_string_for_sort(vm, right))
        }
    }
}

/// Whether the first element's runtime type tag puts `values` in one of the
/// natural-sort primitive domains. Decides `Sortable.sort`'s dispatch (see
/// `baml._is_primitive_array` in `comparable.baml`). Empty → `true`.
pub(super) fn is_primitive_array_values(vm: &BexVm, values: &[Value]) -> bool {
    match values.first() {
        None => true,
        Some(v) => {
            if v.as_int().is_some() {
                true
            } else if let Some(ptr) = v.as_object_ptr() {
                matches!(
                    vm.get_object(ptr),
                    Object::Float(_) | Object::Bigint(_) | Object::String(_)
                )
            } else {
                false
            }
        }
    }
}

fn write_back_array(
    vm: &mut BexVm,
    receiver: Value,
    sorted: Vec<Value>,
) -> Result<Value, VmRustFnError> {
    {
        let mut array = vm.as_array_mut(&receiver)?;
        *array = sorted;
    }
    Ok(receiver)
}

fn write_back_array_result(
    vm: &mut BexVm,
    receiver: Value,
    sorted: Vec<Value>,
) -> NativeCallResult {
    match write_back_array(vm, receiver, sorted) {
        Ok(value) => NativeCallResult::Done(value),
        Err(e) => NativeCallResult::Error(e),
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
            return NativeCallResult::Done(Value::object(vm.alloc_array(self.results)));
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

// ── filter ────────────────────────────────────────────────────────────────────

/// Continuation for `Array.filter`. Accumulates the original elements whose
/// predicate result is true, preserving order.
struct FilterContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    results: Vec<Value>,
}

impl Continuation for FilterContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let keep = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if keep {
            self.results.push(self.array[self.idx]);
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(self.results)));
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
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            return NativeCallResult::Done(Value::bool(true));
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::bool(false));
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
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if !b {
            return NativeCallResult::Done(Value::bool(false));
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::bool(true));
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
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            let result = if self.return_value {
                self.array[self.idx]
            } else {
                #[allow(clippy::cast_possible_wrap)]
                Value::int(self.idx as i64)
            };
            return NativeCallResult::Done(result);
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::NULL);
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
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            let result = if self.return_value {
                self.array[self.idx]
            } else {
                #[allow(clippy::cast_possible_wrap)]
                Value::int(self.idx as i64)
            };
            return NativeCallResult::Done(result);
        }
        if self.idx == 0 {
            return NativeCallResult::Done(Value::NULL);
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
            return NativeCallResult::Done(Value::object(vm.alloc_array(self.results)));
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

// ─── Array.sort_by continuation ─────────────────────────────────────────────

/// CPS insertion sort: builds `sorted` one element at a time, yielding to the
/// BAML comparator for each `(current, sorted[insert_idx])` pair. The receiver
/// is only written back once the whole sort completes, so a throwing
/// comparator leaves the array in its pre-sort state.
struct SortByContinuation {
    receiver: Value,
    f_ptr: HeapPtr,
    items: Vec<Value>,
    next_idx: usize,
    sorted: Vec<Value>,
    insert_idx: usize,
    current: Value,
}

impl SortByContinuation {
    fn advance_or_finish(mut self: Box<Self>, vm: &mut BexVm) -> NativeCallResult {
        if self.next_idx >= self.items.len() {
            return write_back_array_result(vm, self.receiver, self.sorted);
        }
        self.current = self.items[self.next_idx];
        self.next_idx += 1;
        self.insert_idx = 0;
        self.yield_compare()
    }

    fn yield_compare(self: Box<Self>) -> NativeCallResult {
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![self.current, self.sorted[self.insert_idx]],
            type_args: vec![],
            continuation: self,
        }
    }
}

impl Continuation for SortByContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let cmp = match expect_int(vm, value) {
            Ok(i) => i,
            Err(e) => return e,
        };
        if cmp < 0 {
            self.sorted.insert(self.insert_idx, self.current);
            return self.advance_or_finish(vm);
        }
        self.insert_idx += 1;
        if self.insert_idx >= self.sorted.len() {
            self.sorted.push(self.current);
            return self.advance_or_finish(vm);
        }
        self.yield_compare()
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = vec![self.f_ptr];
        collect_value_roots(&[self.receiver, self.current], &mut roots);
        collect_value_roots(&self.items, &mut roots);
        collect_value_roots(&self.sorted, &mut roots);
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        forward_ptr(&mut self.f_ptr, forwarding);
        forward_values(std::slice::from_mut(&mut self.receiver), forwarding);
        forward_values(std::slice::from_mut(&mut self.current), forwarding);
        forward_values(&mut self.items, forwarding);
        forward_values(&mut self.sorted, forwarding);
    }
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
            return NativeCallResult::Done(Value::object(vm.alloc_array(self.results)));
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

    fn new(size: i64, default: &Value) -> Result<Vec<Value>, VmRustFnError> {
        let size = usize::try_from(size).map_err(|_| VmBamlError::InvalidArgument {
            message: format!("array constructor size ({size}) is negative"),
        })?;
        let mut array = Vec::new();
        array.try_reserve(size).map_err(|_| VmPanic::AllocFailure {
            message: format!("Allocation of {size} elements for new array failed"),
        })?;
        array.resize(size, *default);
        Ok(array)
    }

    #[allow(clippy::unused_unit)]
    fn set(array: &mut Vec<Value>, index: i64, value: &Value) -> Result<(), VmRustFnError> {
        let len = array.len();
        let Ok(index_usize) = usize::try_from(index) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.set: index ({index}) is negative"),
            }
            .into());
        };
        if index_usize >= len {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "array.set: index ({index_usize}) is outside the array length ({len})"
                ),
            }
            .into());
        }
        array[index_usize] = *value;
        Ok(())
    }

    fn concat(array: &[Value], other: &[Value]) -> Vec<Value> {
        array.iter().chain(other.iter()).copied().collect()
    }

    fn pop(array: &mut Vec<Value>) -> Option<Value> {
        array.pop()
    }

    fn remove_at(array: &mut Vec<Value>, index: i64) -> Option<Value> {
        let Ok(index) = usize::try_from(index) else {
            return None;
        };
        if index >= array.len() {
            None
        } else {
            Some(array.remove(index))
        }
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

    fn join(vm: &BexVm, array: &[Value], separator: &bex_str::BexStr) -> bex_str::BexStr {
        let sep = separator.as_str();
        let joined = array
            .iter()
            .map(|v| {
                vm.as_string(v)
                    .map(|s| s.as_str().to_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(sep);
        bex_str::BexStr::from(joined)
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

    fn sort_by(vm: &mut BexVm, array: &Value, compare: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *compare) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let items = match vm.as_array(array) {
            Ok(array) => array.to_vec(),
            Err(e) => return NativeCallResult::Error(e.into()),
        };
        if items.len() <= 1 {
            return write_back_array_result(vm, *array, items);
        }
        let first_sorted = items[0];
        let first_current = items[1];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_current, first_sorted],
            type_args: vec![],
            continuation: Box::new(SortByContinuation {
                receiver: *array,
                f_ptr,
                items,
                next_idx: 2,
                sorted: vec![first_sorted],
                insert_idx: 0,
                current: first_current,
            }),
        }
    }

    fn map(vm: &mut BexVm, array: &[Value], f: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *f) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(vec![])));
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

    fn filter(vm: &mut BexVm, array: &[Value], predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(vec![])));
        }
        let first_arg = array[0];
        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FilterContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
            }),
        }
    }

    fn to_json(vm: &mut BexVm, array: &[Value]) -> NativeCallResult {
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(vec![])));
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
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::bool(false));
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
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::bool(true));
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
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
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
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
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
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
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
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
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
        array.iter().any(|v| baml_eq(vm, *v, *item))
    }

    #[allow(clippy::cast_possible_wrap)]
    fn index_of(vm: &BexVm, array: &[Value], item: &Value) -> Option<i64> {
        array
            .iter()
            .position(|v| baml_eq(vm, *v, *item))
            .map(|i| i as i64)
    }

    #[allow(clippy::cast_possible_wrap)]
    fn last_index_of(vm: &BexVm, array: &[Value], item: &Value) -> Option<i64> {
        array
            .iter()
            .rposition(|v| baml_eq(vm, *v, *item))
            .map(|i| i as i64)
    }

    fn reduce(
        vm: &mut BexVm,
        array: &[Value],
        reducer: &Value,
        initial: &Value,
    ) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *reducer) {
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
        let f_ptr = match extract_callable(vm, *f) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(vec![])));
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

#[cfg(test)]
mod tests {
    use bex_vm_types::types::Value;

    use super::{BamlClassArray, PackageBamlImpl};
    use crate::errors::{VmBamlError, VmRustFnError};

    fn expect_invalid_argument<T>(result: Result<T, VmRustFnError>, expected: &str) {
        match result {
            Err(VmRustFnError::BamlError(VmBamlError::InvalidArgument { message })) => {
                assert!(
                    message.contains(expected),
                    "expected InvalidArgument message to contain {expected:?}, got {message:?}"
                );
            }
            Err(err) => panic!("expected InvalidArgument containing {expected:?}, got {err:?}"),
            Ok(_) => panic!("expected InvalidArgument containing {expected:?}, got Ok"),
        }
    }

    #[test]
    fn array_constructor_fills_fixed_size() {
        let default = Value::int(7);
        let array = <PackageBamlImpl as BamlClassArray>::new(3, &default).unwrap();

        assert_eq!(array, vec![default; 3]);
    }

    #[test]
    fn array_set_replaces_existing_index() {
        let mut array = vec![Value::int(1), Value::int(2), Value::int(3)];
        let replacement = Value::int(99);

        <PackageBamlImpl as BamlClassArray>::set(&mut array, 1, &replacement).unwrap();

        assert_eq!(array, vec![Value::int(1), replacement, Value::int(3)]);
    }

    #[test]
    fn array_constructor_negative_size_throws() {
        expect_invalid_argument(
            <PackageBamlImpl as BamlClassArray>::new(-1, &Value::int(0)),
            "negative",
        );
    }

    #[test]
    fn array_set_negative_index_throws() {
        let mut array = vec![Value::int(1)];

        expect_invalid_argument(
            <PackageBamlImpl as BamlClassArray>::set(&mut array, -1, &Value::int(0)),
            "negative",
        );
    }

    #[test]
    fn array_set_index_past_length_throws() {
        let mut array = vec![Value::int(1)];

        expect_invalid_argument(
            <PackageBamlImpl as BamlClassArray>::set(&mut array, 1, &Value::int(0)),
            "outside the array length",
        );
    }
}
