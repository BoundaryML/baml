//! Native implementations of the `baml.ops` comparison interfaces
//! (`Equals` / `Compare`) for primitives and containers, declared in
//! `baml_std/baml/ns_ops/comparison.baml`.
//!
//! These mirror BAML's `==` / `<` / `>` / `<=` / `>=` operators (which the
//! compiler usually special-cases to direct comparison bytecode). They exist
//! so primitives and containers satisfy interface bounds (`T extends Compare`)
//! and so a comparison reached via dynamic dispatch produces the *same* result
//! the specialized bytecode would.
//!
//! Floats compare by IEEE rules (so `NaN != NaN`), matching the `==` operator
//! and deliberately *unlike* `baml.deep_equals`, whose NaN-equal convention is
//! a test-helper nicety rather than the language's equality.

use std::{collections::HashSet, sync::Arc};

use bex_str::BexStr;
use bex_vm_types::{
    HeapPtr, ValueKind,
    types::{LockedContainer, LockedReadGuard, Object, Value},
};
use num_bigint::BigInt;

use super::{
    BamlClassOpsCompare_for_bigint, BamlClassOpsCompare_for_float, BamlClassOpsCompare_for_int,
    BamlClassOpsCompare_for_string, BamlClassOpsEquals_for_bigint, BamlClassOpsEquals_for_bool,
    BamlClassOpsEquals_for_float, BamlClassOpsEquals_for_int, BamlClassOpsEquals_for_string,
    BamlClassOpsEquals_for_uint8array, BamlNamespaceOps, NativeCallResult, PackageBamlImpl,
};
use crate::BexVm;

// ── int ───────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_int for PackageBamlImpl {
    fn eq(int: i64, other: i64) -> bool {
        int == other
    }
}

impl BamlClassOpsCompare_for_int for PackageBamlImpl {
    fn lt(int: i64, other: i64) -> bool {
        int < other
    }
}

// ── bigint ─────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_bigint for PackageBamlImpl {
    fn eq(bigint: Arc<BigInt>, other: Arc<BigInt>) -> bool {
        // `Arc<T>: PartialEq` compares the pointed-to values, so two distinct
        // allocations holding the same integer compare equal.
        bigint == other
    }
}

impl BamlClassOpsCompare_for_bigint for PackageBamlImpl {
    fn lt(bigint: Arc<BigInt>, other: Arc<BigInt>) -> bool {
        bigint < other
    }
}

// ── float ────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_float for PackageBamlImpl {
    // IEEE equality on purpose (`NaN != NaN`); see module docs. No
    // `clippy::float_cmp` attribute needed — it is exempt in `eq`-named fns.
    fn eq(float: f64, other: f64) -> bool {
        float == other
    }
}

impl BamlClassOpsCompare_for_float for PackageBamlImpl {
    // All four are direct IEEE comparisons rather than the interface's
    // boolean-derived defaults (`gt = !le`, etc.): with NaN those defaults
    // would wrongly report `gt`/`ge` as `true`, whereas IEEE `>`/`>=` are
    // `false` for any NaN operand, matching the `==`/`<` operators.
    fn lt(float: f64, other: f64) -> bool {
        float < other
    }

    fn gt(float: f64, other: f64) -> bool {
        float > other
    }

    fn ge(float: f64, other: f64) -> bool {
        float >= other
    }

    fn le(float: f64, other: f64) -> bool {
        float <= other
    }
}

// ── bool ─────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_bool for PackageBamlImpl {
    // The receiver arrives as a raw `&Value` (the codegen has no dedicated
    // `bool` receiver shape); `self: bool` guarantees it is a Bool, so a
    // non-bool falls through to `false`.
    fn eq(bool: &Value, other: bool) -> bool {
        bool.as_bool() == Some(other)
    }
}

// ── string ─────────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_string for PackageBamlImpl {
    fn eq(string: &BexStr, other: &BexStr) -> bool {
        string == other
    }
}

impl BamlClassOpsCompare_for_string for PackageBamlImpl {
    // Lexicographic order (Unicode code unit order), as documented in
    // `comparison.baml`.
    fn lt(string: &BexStr, other: &BexStr) -> bool {
        string < other
    }

    fn gt(string: &BexStr, other: &BexStr) -> bool {
        string > other
    }

    fn ge(string: &BexStr, other: &BexStr) -> bool {
        string >= other
    }

    fn le(string: &BexStr, other: &BexStr) -> bool {
        string <= other
    }
}

// ── uint8array ─────────────────────────────────────────────────────────────

impl BamlClassOpsEquals_for_uint8array for PackageBamlImpl {
    fn eq(uint8array: &[u8], other: &[u8]) -> bool {
        uint8array == other
    }
}

// ── containers ─────────────────────────────────────────────────────────────
//
// `T[]` and `map<K, V>` implement `Equals` via BAML bodies that call
// `baml.ops.equals` (see `comparison.baml`); there are no native `eq` impls to
// write here — the `EqualsDriver` below handles arrays/maps structurally.

impl BamlNamespaceOps for PackageBamlImpl {
    // The broad `==` operator (`baml.ops.equals`, may-yield). See `EqualsDriver`.
    fn equals(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        EqualsDriver::new(*a, *b).drive(vm)
    }
}

/// Outcome of comparing one popped pair in the [`EqualsDriver`] worklist.
enum Cmp {
    /// Equal so far — either a leaf matched, or structural children were pushed.
    Continue,
    /// Operands are unequal; short-circuit the whole comparison to `false`.
    NotEqual,
}

const fn step(equal: bool) -> Cmp {
    if equal { Cmp::Continue } else { Cmp::NotEqual }
}

/// Lock two distinct heap containers, acquiring in a fixed heap-address order
/// and returning the guards mapped back to `(a, b)`.
///
/// The container lock is a non-reentrant exclusive spin-lock, so two fibers that
/// compare the same pair of containers in opposite operand order would deadlock
/// if each acquired in source order. Acquiring by address makes the order
/// canonical, so no such cycle can form. Callers must ensure `pa != pb` (the
/// same-pointer case is handled separately, and would self-deadlock here).
fn lock_pair_ordered<'g, T>(
    pa: HeapPtr,
    a: &'g LockedContainer<T>,
    pb: HeapPtr,
    b: &'g LockedContainer<T>,
) -> (LockedReadGuard<'g, T>, LockedReadGuard<'g, T>) {
    debug_assert_ne!(pa, pb, "lock_pair_ordered requires distinct containers");
    if pa < pb {
        let ga = a.lock();
        let gb = b.lock();
        (ga, gb)
    } else {
        let gb = b.lock();
        let ga = a.lock();
        (ga, gb)
    }
}

/// Worklist driver for the broad `==` operator (`baml.ops.equals`).
///
/// Holds an explicit stack of pending pairs instead of recursing in Rust, so
/// that dispatching to a user class's bytecode `Equals.eq` (a follow-up) can
/// suspend and resume the walk across the VM's `YieldToCall` trampoline.
///
/// Equality is **not reflexive** — `NaN != NaN`, so a value containing a `NaN`
/// (or any non-reflexive element) is not equal to itself. There is therefore no
/// same-pointer fast path; we always descend into structure. The `visited` set
/// only handles cycles: a re-encountered *in-progress* object pair (a back-edge)
/// is assumed equal so traversal terminates, while every reachable element is
/// still compared on its first visit (so a `NaN` anywhere still forces `false`).
///
/// Semantics (the broad `==`): operands of different concrete runtime kinds are
/// never equal; primitives/strings/bigints/uint8arrays compare by value (floats
/// by IEEE, so `NaN != NaN`); enums by identity; arrays/maps/class-instances
/// recurse structurally.
///
/// NOTE: a class operand is compared structurally here even if it implements a
/// custom `Equals`; honoring a user `Equals.eq` for class operands (via
/// `YieldToCall`) is a follow-up increment that adds a `Cmp::Yield` arm.
struct EqualsDriver {
    stack: Vec<(Value, Value)>,
    visited: HashSet<(HeapPtr, HeapPtr)>,
}

impl EqualsDriver {
    fn new(a: Value, b: Value) -> Self {
        Self {
            stack: vec![(a, b)],
            visited: HashSet::new(),
        }
    }

    /// Drain the worklist to a boolean result. (A follow-up adds a `YieldToCall`
    /// arm that boxes `self` into a `Continuation` to call a user `Equals.eq`.)
    fn drive(mut self, vm: &BexVm) -> NativeCallResult {
        while let Some((a, b)) = self.stack.pop() {
            if let Cmp::NotEqual = self.compare_one(vm, a, b) {
                return NativeCallResult::Done(Value::bool(false));
            }
        }
        NativeCallResult::Done(Value::bool(true))
    }

    /// Compare one popped pair: a leaf decides equality directly; a structural
    /// value pushes its children onto the stack. Operands of different concrete
    /// kinds are never equal.
    fn compare_one(&mut self, vm: &BexVm, a: Value, b: Value) -> Cmp {
        match (a.kind(), b.kind()) {
            (ValueKind::Null, ValueKind::Null) | (ValueKind::OmittedArg, ValueKind::OmittedArg) => {
                Cmp::Continue
            }
            (ValueKind::Int(x), ValueKind::Int(y)) => step(x == y),
            (ValueKind::Bool(x), ValueKind::Bool(y)) => step(x == y),
            (ValueKind::Object(pa), ValueKind::Object(pb)) => {
                // No same-pointer (`pa == pb`) shortcut: equality is not
                // reflexive (a `NaN` inside `a` makes `a != a`), so we must
                // still compare the contents. `visited` only breaks cycles.
                let key = if pa < pb { (pa, pb) } else { (pb, pa) };
                if !self.visited.insert(key) {
                    // Already comparing this exact pair higher on the walk (a
                    // cyclic back-edge): assume equal so traversal terminates.
                    // Non-back-edge content was/will be checked on first visit.
                    return Cmp::Continue;
                }
                self.compare_objects(vm, pa, pb)
            }
            (ValueKind::Object(a), _) => {
                let Object::Cell(a) = vm.get_object(a) else {
                    return Cmp::NotEqual;
                };
                self.compare_one(vm, a.load(), b)
            }
            (_, ValueKind::Object(pb)) => {
                let Object::Cell(b) = vm.get_object(pb) else {
                    return Cmp::NotEqual;
                };
                self.compare_one(vm, a, b.load())
            }
            _ => Cmp::NotEqual,
        }
    }

    /// Compare two heap objects. Leaves decide directly; arrays/maps/instances
    /// push their children. Containers are snapshotted under their lock before
    /// touching the stack (the per-container lock is a non-reentrant spin-lock).
    #[expect(clippy::float_cmp)] // IEEE float equality on purpose (matches `float.eq`).
    fn compare_objects(&mut self, vm: &BexVm, pa: HeapPtr, pb: HeapPtr) -> Cmp {
        match (vm.get_object(pa), vm.get_object(pb)) {
            (Object::Cell(x), Object::Cell(y)) => {
                self.stack.push((x.value.load(), y.value.load()));
                Cmp::Continue
            }
            (Object::Cell(x), _) => {
                let Some(x) = x.value.load().as_object_ptr() else {
                    // rhs is a non-cell object so if lhs is not then they are not equal.
                    return Cmp::NotEqual;
                };
                self.compare_objects(vm, x, pb)
            }
            (_, Object::Cell(y)) => {
                let Some(y) = y.value.load().as_object_ptr() else {
                    // lhs is a non-cell object so if rhs is not then they are not equal.
                    return Cmp::NotEqual;
                };
                self.compare_objects(vm, pa, y)
            }

            (Object::Float(x), Object::Float(y)) => step(x == y),
            (Object::Float(_), _) => Cmp::NotEqual,
            (Object::String(x), Object::String(y)) => step(x == y),
            (Object::String(_), _) => Cmp::NotEqual,
            // Different `Arc`s with the same numeric value compare equal.
            (Object::Bigint(x), Object::Bigint(y)) => step(x == y),
            (Object::Bigint(_), _) => Cmp::NotEqual,
            // Same byte array: trivially equal (bytes are reflexive — no NaN).
            (Object::Uint8Array(_), Object::Uint8Array(_)) if pa == pb => Cmp::Continue,
            (Object::Uint8Array(x), Object::Uint8Array(y)) => {
                step(x.lock().as_slice() == y.lock().as_slice())
            }
            (Object::Uint8Array(_), _) => Cmp::NotEqual,

            (Object::Array(x), Object::Array(_)) if pa == pb => {
                let xs = x.lock();
                self.stack.extend(xs.iter().copied().map(|v| (v, v)));
                Cmp::Continue
            }
            (Object::Array(x), Object::Array(y)) => {
                let (xs, ys) = lock_pair_ordered(pa, x, pb, y);
                if xs.len() != ys.len() {
                    return Cmp::NotEqual;
                }
                self.stack
                    .extend(xs.iter().copied().zip(ys.iter().copied()));
                Cmp::Continue
            }
            (Object::Array(_), _) => Cmp::NotEqual,

            (Object::Map(x), Object::Map(_)) if pa == pb => {
                let xs = x.lock();
                self.stack.extend(xs.values().copied().map(|v| (v, v)));
                Cmp::Continue
            }
            (Object::Map(x), Object::Map(y)) => {
                let (xs, ys) = lock_pair_ordered(pa, x, pb, y);
                if xs.len() != ys.len() {
                    return Cmp::NotEqual;
                }
                // Order-insensitive: same keys, equal values.
                for (k, xv) in &**xs {
                    match ys.get(k) {
                        Some(yv) => self.stack.push((*xv, *yv)),
                        None => return Cmp::NotEqual,
                    }
                }
                Cmp::Continue
            }
            (Object::Map(_), _) => Cmp::NotEqual,

            (Object::Variant(x), Object::Variant(y)) => step(x.enm == y.enm && x.index == y.index),
            (Object::Variant(_), _) => Cmp::NotEqual,
            (Object::Instance(x), Object::Instance(y)) => {
                if x.class != y.class || x.fields.len() != y.fields.len() {
                    return Cmp::NotEqual;
                }
                for (fx, fy) in x.fields.iter().zip(y.fields.iter()) {
                    self.stack.push((fx.load(), fy.load()));
                }
                Cmp::Continue
            }
            (Object::Instance(_), _) => Cmp::NotEqual,

            (Object::BoundMethod(x), Object::BoundMethod(y)) => {
                // must be the same *value* not just equivalent.
                step(x.function == y.function && x.receiver == y.receiver)
            }
            (Object::BoundMethod(_), _) => Cmp::NotEqual,
            (Object::GenericFunction(x), Object::GenericFunction(y)) => {
                step(x.function == y.function && x.type_args == y.type_args)
            }
            (Object::GenericFunction(_), _) => Cmp::NotEqual,
            (Object::HostClosure(x), Object::HostClosure(y)) => {
                step(Arc::ptr_eq(&x.handle, &y.handle))
            }
            (Object::HostClosure(_), _) => Cmp::NotEqual,
            (Object::Future(x), Object::Future(y)) => step(x.id() == y.id()),
            (Object::Future(_), _) => Cmp::NotEqual,
            (Object::Collector(x), Object::Collector(y)) => step(Arc::ptr_eq(&x.0, &y.0)),
            (Object::Collector(_), _) => Cmp::NotEqual,

            (Object::Type(x), Object::Type(y)) => step(x == y),
            (Object::Type(_), _) => Cmp::NotEqual,

            // `Sentinel` (heap_debug builds only) is an internal freed/uninit
            // heap marker that should never reach a value comparison; treat it
            // as unequal rather than panicking.
            #[cfg(feature = "heap_debug")]
            (Object::Sentinel(_), _) => Cmp::NotEqual,

            // By reference:
            (Object::Function(_), Object::Function(_))
            | (Object::Closure(_), Object::Closure(_))
            | (Object::Class(_), Object::Class(_))
            | (Object::Enum(_), Object::Enum(_))
            | (Object::UnscheduledFuture(_), Object::UnscheduledFuture(_))
            | (Object::RustData(_), Object::RustData(_)) => step(pa == pb),
            (
                Object::Function(_)
                | Object::Closure(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::UnscheduledFuture(_)
                | Object::RustData(_),
                _,
            ) => Cmp::NotEqual,
        }
    }
}
