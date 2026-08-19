//! Native implementations of the `baml.ops` comparison interfaces
//! (`Equals` / `Compare`) for primitives and containers, declared in
//! `baml_std/baml/ns_ops/comparison.baml`.
//!
//! These mirror BAML's `==` / `<` / `>` / `<=` / `>=` operators (which the
//! compiler usually special-cases to direct comparison bytecode). They exist
//! so primitives and containers satisfy interface bounds (`T extends Compare`)
//! and so a comparison of *those* reached via dynamic dispatch produces the
//! *same* result the specialized bytecode would. (The broad `==` driver
//! [`EqualsDriver`] dispatches a class's or enum's custom `Equals.eq` when it has
//! one, falling back to structural / variant-identity comparison otherwise.)
//!
//! Floats compare by IEEE rules (so `NaN != NaN`), matching the `==` operator.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use baml_type::{Name, TyAttr, TypeName, normalize::TypeContext};
use bex_str::BexStr;
use bex_vm_types::RealizedTy;
use bex_vm_types::{
    HeapPtr, ValueKind,
    errors::VmInternalError,
    types::{LockedContainer, LockedReadGuard, Object, Type, Value},
};
use num_bigint::BigInt;

use super::{
    BamlClassOpsCompare_for_bigint, BamlClassOpsCompare_for_float, BamlClassOpsCompare_for_int,
    BamlClassOpsCompare_for_string, BamlClassOpsEquals_for_bigint, BamlClassOpsEquals_for_bool,
    BamlClassOpsEquals_for_float, BamlClassOpsEquals_for_int, BamlClassOpsEquals_for_string,
    BamlClassOpsEquals_for_uint8array, BamlNamespaceOps, Continuation, NativeCallResult,
    PackageBamlImpl, PassThroughContinuation, resolve,
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
// `baml.ops.equals_equals` (see `comparison.baml`); there are no native `eq` impls
// to write here — the `EqualsDriver` below handles arrays/maps structurally.

impl BamlNamespaceOps for PackageBamlImpl {
    // The broad `==` operator (`baml.ops.equals_equals`, may-yield). See `EqualsDriver`.
    fn equals_equals(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        EqualsDriver::new(*a, *b).drive(vm)
    }

    // Binary-operator dispatch drivers — see [`drive_binary_op`]. The compiler
    // emits these only when a single `implement` can't be pinned at compile time
    // (an operand erased to a union / interface-existential / type variable), so
    // the impl is selected from the operands' runtime types.
    fn __union_add(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        drive_binary_op(vm, "Add", "add", *a, *b)
    }
    fn __union_sub(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        drive_binary_op(vm, "Subtract", "sub", *a, *b)
    }
    fn __union_mul(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        drive_binary_op(vm, "Multiply", "mul", *a, *b)
    }
    fn __union_div(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        drive_binary_op(vm, "Divide", "div", *a, *b)
    }
    fn __union_rem(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        drive_binary_op(vm, "Remainder", "rem", *a, *b)
    }
    fn __union_neg(vm: &mut BexVm, a: &Value) -> NativeCallResult {
        // Negation is single dispatch on `a` (`Negate` has no `Rhs`), so the
        // interface request carries no args.
        dispatch_op(vm, "Negate", "neg", vec![*a], &[])
    }
}

/// Binary operator dispatch: `a OP b` selects `<typeof a as iface<typeof b>>`
/// and tail-calls its `method(self, rhs)` with `[a, b]`. This is the double
/// dispatch a single-receiver virtual call cannot express — the impl depends on
/// *both* operand types.
fn drive_binary_op(
    vm: &mut BexVm,
    iface: &str,
    method: &str,
    a: Value,
    b: Value,
) -> NativeCallResult {
    // Operator operands always have a concrete BAML type (the type checker
    // proved they implement the interface); a value without one (a raw
    // function/future) reaching here is an engine invariant break.
    let Some(rhs_ty) = vm.value_concrete_ty(b) else {
        return NativeCallResult::from(unresolved_op(iface, method));
    };
    dispatch_op(vm, iface, method, vec![a, b], &[rhs_ty.into()])
}

/// Resolve `<typeof args[0] as baml.ops.<iface><iface_args>>::<method>` from the
/// receiver's runtime concrete type and the (runtime-derived) interface args,
/// then tail-call it with `args`. The type checker has already proved the
/// operand types implement the operator, so a missing impl is an engine
/// invariant break, surfaced as an internal error.
fn dispatch_op(
    vm: &mut BexVm,
    iface: &str,
    method: &str,
    args: Vec<Value>,
    iface_args: &[RealizedTy],
) -> NativeCallResult {
    let op_qtn = TypeName::new(Name::new("baml"), vec![Name::new("ops")], Name::new(iface));
    // A stdlib FQN constant is one of the three places a name legitimately
    // becomes a head; it resolves once, off the declaration.
    let Some(op_head) = vm.declaration_head(&op_qtn) else {
        return NativeCallResult::from(unresolved_op(iface, method));
    };
    let Some(self_ty) = vm.value_concrete_ty(args[0]) else {
        return NativeCallResult::from(unresolved_op(iface, method));
    };
    let resolver = resolve::ImplResolver::for_value(vm, args[0]);
    let Some((rule, bound_args)) =
        resolver.resolve_implements_rule(&self_ty.into(), op_head, iface_args)
    else {
        return NativeCallResult::from(unresolved_op(iface, method));
    };
    let Some(method_impl) = rule.methods.get(method) else {
        return NativeCallResult::from(unresolved_op(iface, method));
    };
    // The resolved impl's frame realizes fully against its bound args; a failure
    // is a broken compiler/VM invariant, surfaced rather than swallowed.
    let type_args = match resolver.realize_frame(&method_impl.frame, &bound_args) {
        Ok(type_args) => type_args,
        Err(e) => return NativeCallResult::from(e),
    };
    NativeCallResult::YieldToCall {
        // `fqn` is the resolved callee's heap pointer, baked at emit time.
        callee: method_impl.fqn,
        args,
        type_args,
        // The operator's value *is* the impl method's return value — forward it.
        continuation: Box::new(PassThroughContinuation),
    }
}

/// The internal error for an operator dispatch the type checker promised could
/// not miss: no concrete receiver type, no applicable impl, or a rule without
/// the method.
fn unresolved_op(iface: &str, method: &str) -> VmInternalError {
    VmInternalError::UnresolvedVirtualCall {
        method: format!("baml.ops.{iface}.{method}"),
    }
}

/// Outcome of comparing one popped pair in the [`EqualsDriver`] worklist.
enum Cmp {
    /// Equal so far — either a leaf matched, or structural children were pushed.
    Continue,
    /// Operands are unequal; short-circuit the whole comparison to `false`.
    NotEqual,
    /// The pair's class implements a custom `Equals`; suspend the walk and call its
    /// `eq(self, other)`. The bytecode result (a `bool`) decides this pair, then the
    /// walk resumes over the remaining stack.
    Yield {
        callee: HeapPtr,
        args: Vec<Value>,
        type_args: Vec<RealizedTy>,
    },
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

/// Follow a chain of `Cell` indirections to the underlying non-cell value, returning
/// `None` on a cell cycle. Operands shouldn't reach the driver as raw cells — the
/// compiler loads them before dispatch — but resolving iteratively (rather than
/// recursing once per indirection) keeps the driver total against a pathological cycle.
fn resolve_cells(vm: &BexVm, mut v: Value) -> Option<Value> {
    let mut seen: Option<HashSet<HeapPtr>> = None;
    loop {
        let ValueKind::Object(ptr) = v.kind() else {
            return Some(v);
        };
        let Object::Cell(cell) = vm.get_object(ptr) else {
            return Some(v);
        };
        if !seen.get_or_insert_with(HashSet::new).insert(ptr) {
            return None;
        }
        v = cell.load();
    }
}

/// Worklist driver for the broad `==` operator (`baml.ops.equals_equals`).
///
/// Holds an explicit stack of pending pairs instead of recursing in Rust, so that
/// dispatching to a user class's bytecode `Equals.eq` can suspend and resume the walk
/// across the VM's `YieldToCall` trampoline (`EqualsDriver` is itself the [`Continuation`]).
///
/// Equality is **not reflexive** — `NaN != NaN`, so a value containing a `NaN`
/// (or any non-reflexive element) is not equal to itself. There is therefore no
/// same-pointer fast path; we always descend into structure. The `visited` set
/// only handles cycles: a re-encountered *in-progress* object pair (a back-edge)
/// is assumed equal so traversal terminates, while every reachable element is
/// still compared on its first visit (so a `NaN` anywhere still forces `false`).
///
/// Semantics (the broad `==`): operands of different concrete runtime kinds are never
/// equal; primitives/strings/bigints/uint8arrays compare by value (floats by IEEE, so
/// `NaN != NaN`); enums by identity; two same-class instances dispatch to the class's
/// custom `Equals.eq` if it has one, else compare structurally (field by field); arrays
/// and maps recurse structurally.
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

    /// Drain the worklist: `Done(true)` if the stack empties with everything equal,
    /// `Done(false)` on the first unequal pair, or a `YieldToCall` (with `self` as the
    /// continuation) when a pair needs a user `Equals.eq` — resumed by [`Continuation::call`].
    fn drive(mut self, vm: &BexVm) -> NativeCallResult {
        while let Some((a, b)) = self.stack.pop() {
            match self.compare_one(vm, a, b) {
                Cmp::Continue => {}
                Cmp::NotEqual => return NativeCallResult::Done(Value::bool(false)),
                Cmp::Yield {
                    callee,
                    args,
                    type_args,
                } => {
                    return NativeCallResult::YieldToCall {
                        callee,
                        args,
                        type_args,
                        continuation: Box::new(self),
                    };
                }
            }
        }
        NativeCallResult::Done(Value::bool(true))
    }

    /// Compare one popped pair: a leaf decides equality directly; a structural
    /// value pushes its children onto the stack. Operands of different concrete
    /// kinds are never equal.
    fn compare_one(&mut self, vm: &BexVm, a: Value, b: Value) -> Cmp {
        // Resolve any `Cell` indirection up front (bounded against cell cycles) so the
        // arms below — and `compare_objects` — never see a raw cell. `None` is a cycle.
        let (Some(a), Some(b)) = (resolve_cells(vm, a), resolve_cells(vm, b)) else {
            return Cmp::NotEqual;
        };
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
                // Order-normalized key: a pair reached as both `(p, q)` and `(q, p)` is
                // compared once — sound because `==` / `Equals.eq` are symmetric by
                // contract (a non-symmetric user `eq` would observe only the first result).
                let key = if pa < pb { (pa, pb) } else { (pb, pa) };
                if !self.visited.insert(key) {
                    // Already comparing this exact pair higher on the walk (a
                    // cyclic back-edge): assume equal so traversal terminates.
                    // Non-back-edge content was/will be checked on first visit.
                    return Cmp::Continue;
                }
                self.compare_objects(vm, pa, pb)
            }
            // Mismatched value kinds are never equal. Every kind is listed explicitly
            // (no total `_` wildcard), so a new `ValueKind` fails to compile here and must
            // be classified rather than silently defaulting to `NotEqual` even against
            // itself.
            (
                ValueKind::Null
                | ValueKind::OmittedArg
                | ValueKind::Int(_)
                | ValueKind::Bool(_)
                | ValueKind::Object(_),
                _,
            ) => Cmp::NotEqual,
        }
    }

    /// Compare two heap objects. Leaves decide directly; arrays/maps/instances
    /// push their children. Containers are snapshotted under their lock before
    /// touching the stack (the per-container lock is a non-reentrant spin-lock).
    #[expect(clippy::float_cmp)] // IEEE float equality on purpose (matches `float.eq`).
    fn compare_objects(&mut self, vm: &BexVm, pa: HeapPtr, pb: HeapPtr) -> Cmp {
        match (vm.get_object(pa), vm.get_object(pb)) {
            // Cells are resolved to their underlying value in `compare_one` before any
            // object pair reaches here, so a cell can't occur; treat it as unequal rather
            // than recursing (which a cell cycle could otherwise do without bound).
            (Object::Cell(_), _) | (_, Object::Cell(_)) => Cmp::NotEqual,

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
                // Acquire both byte-array locks in canonical (address) order, like the
                // Array/Map arms — holding both in source order risks an AB-BA deadlock
                // with a concurrent fiber comparing the same pair in the opposite order
                // (the non-reentrant spin-lock spins forever). The same-pointer case is
                // handled above, so `lock_pair_ordered`'s `pa != pb` precondition holds.
                let (xs, ys) = lock_pair_ordered(pa, x, pb, y);
                step(xs.as_slice() == ys.as_slice())
            }
            (Object::Uint8Array(_), _) => Cmp::NotEqual,

            (Object::Array(x), Object::Array(_)) if pa == pb => {
                let xs = x.lock();
                self.stack.extend(xs.iter().copied().map(|v| (v, v)));
                Cmp::Continue
            }
            (Object::Array(x), Object::Array(y)) => {
                let (xs, ys) = lock_pair_ordered(pa, &x.data, pb, &y.data);
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
                let (xs, ys) = lock_pair_ordered(pa, &x.data, pb, &y.data);
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

            (Object::Variant(x), Object::Variant(y)) => {
                // Different enum types ⇒ never equal (`eq` needs the same `Self`).
                if x.enm != y.enm {
                    return Cmp::NotEqual;
                }
                // Dispatch to the enum's custom `Equals.eq` if it has one (`baml.ops.Equals`
                // applies to enums too), else compare by variant identity.
                if let Some((callee, type_args)) = value_concrete_ty(vm, pa)
                    .and_then(|ty| resolve_equals_eq(vm, Value::object(pa), &ty))
                {
                    return Cmp::Yield {
                        callee,
                        args: vec![Value::object(pa), Value::object(pb)],
                        type_args,
                    };
                }
                step(x.index == y.index)
            }
            (Object::Variant(_), _) => Cmp::NotEqual,
            (Object::Instance(x), Object::Instance(y)) => {
                // Different concrete type (class or generic instantiation) ⇒ never equal.
                // `Equals.eq(self, other: Self)` requires the same `Self`, so differing
                // `class_type_args` (`Box<int>` vs `Box<string>`) is unequal up front.
                // Compared with `ty_args_equivalent`, not `==`, so union args match
                // order-insensitively (`Box<int | string>` is the same `Self` as
                // `Box<string | int>`) — the same notion of "same instantiation" the
                // resolver and reflection use.
                if x.class != y.class
                    || !resolve::ImplResolver::new(vm)
                        .ty_args_equivalent(&x.class_type_args, &y.class_type_args)
                    || x.fields.len() != y.fields.len()
                {
                    return Cmp::NotEqual;
                }
                // Same concrete type: dispatch to the class's custom `Equals.eq` if it has
                // one, else compare structurally (field by field).
                if let Some((callee, type_args)) = value_concrete_ty(vm, pa)
                    .and_then(|ty| resolve_equals_eq(vm, Value::object(pa), &ty))
                {
                    return Cmp::Yield {
                        callee,
                        args: vec![Value::object(pa), Value::object(pb)],
                        type_args,
                    };
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
            (Object::GenericFunction(x), Object::GenericFunction(y)) => step(
                x.function == y.function
                    && x.type_args == y.type_args
                    && x.runtime_package == y.runtime_package,
            ),
            (Object::GenericFunction(_), _) => Cmp::NotEqual,
            (Object::HostClosure(x), Object::HostClosure(y)) => {
                step(Arc::ptr_eq(&x.handle, &y.handle))
            }
            (Object::HostClosure(_), _) => Cmp::NotEqual,
            (Object::Future(x), Object::Future(y)) => step(x.id() == y.id()),
            (Object::Future(_), _) => Cmp::NotEqual,
            (Object::Collector(x), Object::Collector(y)) => step(Arc::ptr_eq(&x.0, &y.0)),
            (Object::Collector(_), _) => Cmp::NotEqual,

            // A `type` value denotes a type and nothing more: two are equal
            // exactly when they are mutual subtypes, decided against the
            // program's facts (TYPE_SYSTEM.md, "Equivalence and canonical
            // forms") — the same relation `==` on `type` uses.
            (Object::Type(x), Object::Type(y)) => step(vm.equivalent(x.ty.as_ty(), y.ty.as_ty())),
            (Object::Type(_), _) => Cmp::NotEqual,

            // `Sentinel` (heap_debug builds only) is an internal freed/uninit
            // heap marker that should never reach a value comparison; treat it
            // as unequal rather than panicking.
            #[cfg(feature = "heap_debug")]
            (Object::Sentinel(_), _) => Cmp::NotEqual,

            // By reference:
            (Object::Function(_), Object::Function(_))
            | (Object::Interface(_), Object::Interface(_))
            | (Object::Package(_), Object::Package(_))
            | (Object::ImplRule(_), Object::ImplRule(_))
            | (Object::Closure(_), Object::Closure(_))
            | (Object::Class(_), Object::Class(_))
            | (Object::Enum(_), Object::Enum(_))
            | (Object::TypeAlias(_), Object::TypeAlias(_))
            | (Object::UnscheduledFuture(_), Object::UnscheduledFuture(_))
            | (Object::RustData(_), Object::RustData(_)) => step(pa == pb),
            (
                Object::Function(_)
                | Object::Interface(_)
                | Object::Package(_)
                | Object::ImplRule(_)
                | Object::Closure(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::TypeAlias(_)
                | Object::UnscheduledFuture(_)
                | Object::RustData(_),
                _,
            ) => Cmp::NotEqual,
        }
    }
}

impl Continuation for EqualsDriver {
    /// Resume after a user `Equals.eq` returns: `false` → the whole comparison is
    /// `false`; `true` → that pair is equal, keep draining the remaining stack.
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        match value.as_bool() {
            Some(true) => (*self).drive(vm),
            Some(false) => NativeCallResult::Done(Value::bool(false)),
            // `eq` is typed `-> bool throws never`, so a non-bool return is a compiler/VM
            // invariant break, not a possible runtime value — surface it as an internal
            // engine error rather than silently treating it as "unequal".
            None => NativeCallResult::from(VmInternalError::TypeError {
                expected: Type::Bool,
                got: vm.type_of(&value),
            }),
        }
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for (a, b) in &self.stack {
            roots.extend(a.as_object_ptr());
            roots.extend(b.as_object_ptr());
        }
        for (pa, pb) in &self.visited {
            roots.push(*pa);
            roots.push(*pb);
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        let fwd = |p: HeapPtr| forwarding.get(&p).copied().unwrap_or(p);
        for (a, b) in &mut self.stack {
            if let Some(p) = a.as_object_ptr() {
                *a = Value::object(fwd(p));
            }
            if let Some(p) = b.as_object_ptr() {
                *b = Value::object(fwd(p));
            }
        }
        // Re-key the cycle set on the forwarded pointers, preserving the `(min, max)`
        // ordering the insert side uses so a re-encountered pair still matches.
        self.visited = self
            .visited
            .iter()
            .map(|&(pa, pb)| {
                let (na, nb) = (fwd(pa), fwd(pb));
                if na < nb { (na, nb) } else { (nb, na) }
            })
            .collect();
    }
}

/// The concrete runtime `RealizedTy` of the class instance or enum value at `ptr` — `Class` with
/// its `class_type_args` (so a generic instance resolves at the right args), or `Enum`.
/// `None` for any other object kind (only instances/enums implement `Equals`).
fn value_concrete_ty(vm: &BexVm, ptr: HeapPtr) -> Option<RealizedTy> {
    match vm.get_object(ptr) {
        Object::Instance(inst) => {
            let (class_ptr, type_args) = (inst.class, inst.class_type_args.to_vec());
            match vm.get_object(class_ptr) {
                Object::Class(class) => Some(RealizedTy::Class(
                    bex_vm_types::TypeHead::new(class_ptr, class.type_tag),
                    type_args,
                    TyAttr::default(),
                )),
                _ => None,
            }
        }
        Object::Variant(v) => match vm.get_object(v.enm) {
            Object::Enum(e) => Some(RealizedTy::Enum(
                bex_vm_types::TypeHead::new(v.enm, e.type_tag),
                TyAttr::default(),
            )),
            _ => None,
        },
        _ => None,
    }
}

/// Resolve `<concrete> as Equals>::eq` to its callee plus the impl's bound type args, or
/// `None` when the type has no `Equals` impl (→ the structural/identity fallback). The
/// concrete type carries any `class_type_args`, so a generic/blanket impl
/// (`implement<T> Equals for Box<T>`) resolves at the right `T`.
fn resolve_equals_eq(
    vm: &BexVm,
    value: Value,
    concrete: &RealizedTy,
) -> Option<(HeapPtr, Vec<RealizedTy>)> {
    // `Equals` is non-generic — no interface args to select on; off the resolved
    // rule, `eq` is the concrete method (the impl's own, or the merged default),
    // invoked with its frame realized against the impl's bound type args.
    let equals_head = vm.declaration_head(&equals_qtn())?;
    let resolver = resolve::ImplResolver::for_value(vm, value);
    let (rule, bound_args) = resolver.resolve_implements_rule(concrete, equals_head, &[])?;
    let method = rule.methods.get("eq")?;
    // `fqn` is the resolved callee's heap pointer (the impl method or merged
    // default), baked at emit time — invoke it directly.
    let callee = method.fqn;
    // The resolved impl's frame realizes fully against its bound args (every
    // projection reduced through the impl registry). A failure is a broken
    // compiler/VM invariant rather than a runtime possibility, so surface it
    // instead of silently dropping the custom `eq`.
    let type_args = resolver
        .realize_frame(&method.frame, &bound_args)
        .unwrap_or_else(|e| {
            unreachable!("Equals impl frame did not realize against bound args: {e}")
        });
    Some((callee, type_args))
}

/// The `baml.ops.Equals` interface name.
fn equals_qtn() -> TypeName {
    TypeName::new(
        Name::new("baml"),
        vec![Name::new("ops")],
        Name::new("Equals"),
    )
}
