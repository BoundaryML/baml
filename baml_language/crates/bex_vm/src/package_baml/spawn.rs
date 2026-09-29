//! Native implementations for `namespace baml.spawn`: `Limit` and
//! `CancelToken`, and [`spawn_launch`], which reads what a `spawn` starts out
//! of its `Plan`.
//!
//! `CancelToken` and `Limit` are opaque classes whose `_handle` field holds an
//! `Object::RustData(Arc<…>)`. They are built with the standard
//! `resolve_class` + `alloc_rust_data` + `alloc_instance` pattern and read
//! back via `as_instance` + `load_field(0)` + `as_rust_data`.

use std::{num::NonZeroUsize, sync::Arc};

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, LimitInner, LimitSet, Object, ObjectType, RealizedTy,
    types::{CancellationToken, Value},
};

use super::{
    BamlClassSpawnCancelToken, BamlClassSpawnLimit, BamlNamespaceSpawn, PackageBamlImpl, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// What a `baml.spawn.CancelToken` holds: it has fired once its own token or
/// any of `inputs` has. `cancel()` fires only `own`, so cancelling a
/// `CancelToken.any` composite never reaches its inputs, and linking the token
/// into a plan links every one of them.
///
/// A composite keeps its inputs rather than forwarding them into a fresh token,
/// because a forwarder has nothing to end it: no signal marks the composite's
/// last clone being dropped, so a forwarding task holds every input until one
/// fires — for a long-lived input, for the life of the program. Holding them
/// also makes `is_cancelled` exact at once, where a forwarder answered only
/// after it had been scheduled.
struct CancelTokenState {
    own: CancellationToken,
    /// Flattened: an input that is itself a composite contributes its tokens.
    inputs: Box<[CancellationToken]>,
}

impl CancelTokenState {
    fn fresh() -> Self {
        Self {
            own: CancellationToken::new(),
            inputs: Box::new([]),
        }
    }

    /// Every token whose firing fires this one.
    fn tokens(&self) -> impl Iterator<Item = &CancellationToken> {
        std::iter::once(&self.own).chain(self.inputs.iter())
    }

    fn is_cancelled(&self) -> bool {
        self.tokens().any(CancellationToken::is_cancelled)
    }
}

/// The tokens a `baml.spawn.CancelToken` value fires on (see
/// [`CancelTokenState`]). Field 0 (`_handle`) is the `Object::RustData`
/// holding the state. Returns `None` if the value is not a well-formed
/// `CancelToken` instance (including the `OmittedArg` sentinel for an omitted
/// optional argument).
fn cancel_token_members(vm: &BexVm, value: Value) -> Option<Vec<CancellationToken>> {
    let instance = vm.as_instance(&value).ok()?;
    let handle = instance.load_field(0);
    let state = vm.as_rust_data::<CancelTokenState>(&handle).ok()?;
    Some(state.tokens().cloned().collect())
}

/// Allocate a `baml.spawn.CancelToken` instance holding `state`.
fn alloc_cancel_token(vm: &mut BexVm, state: CancelTokenState) -> Value {
    let class = vm.resolve_class("baml.spawn.CancelToken");
    let handle = Value::object(vm.alloc_rust_data(Arc::new(state)));
    Value::object(vm.alloc_instance(class, vec![handle]))
}

/// Convert a `usize` count/limit to `i64`, saturating at `i64::MAX`. Group
/// counts and limits are always small in practice; this just makes the cast
/// explicitly lossless-or-saturating for clippy.
fn clamp_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSpawnCancelToken for PackageBamlImpl {
    // The generated trait fixes the return type as `Value` (the heap
    // `CancelToken` instance), so it cannot return `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm) -> Value {
        alloc_cancel_token(vm, CancelTokenState::fresh())
    }

    fn any(vm: &mut BexVm, tokens: &[Value]) -> Value {
        // Holds its inputs instead of forwarding them (see `CancelTokenState`),
        // under a fresh own token so cancelling the composite stays one-way.
        let inputs = tokens
            .iter()
            .filter_map(|&token| cancel_token_members(vm, token))
            .flatten()
            .collect();
        alloc_cancel_token(
            vm,
            CancelTokenState {
                own: CancellationToken::new(),
                inputs,
            },
        )
    }

    fn cancel(vm: &BexVm, canceltoken: &view::spawn::CancelToken<'_>) -> i64 {
        // Returns 1 if this call performed the Pending -> Cancelled transition,
        // 0 if the token was already cancelled. `CancellationToken::cancel`
        // returns (), so we snapshot `is_cancelled` first (a benign TOCTOU
        // under concurrent cancels — the count is best-effort).
        let state = canceltoken._handle::<CancelTokenState>(vm);
        let was_cancelled = state.is_cancelled();
        state.own.cancel();
        i64::from(!was_cancelled)
    }

    fn is_cancelled(vm: &BexVm, canceltoken: &view::spawn::CancelToken<'_>) -> bool {
        canceltoken._handle::<CancelTokenState>(vm).is_cancelled()
    }
}

/// What a `spawn` starts, read out of its `baml.spawn.Plan<T, E>`.
#[derive(Debug)]
pub struct SpawnLaunch {
    /// The callable the task runs: `() -> T throws E`.
    pub body: HeapPtr,
    pub name: Option<String>,
    /// The `T` of the `Future<T, E>` the spawn yields.
    pub returns: RealizedTy,
    /// That future's `E`.
    pub throws: RealizedTy,
    /// Every limit of the plan, taken together before the task starts.
    pub limits: LimitSet,
    /// Tokens linked into the task's own: any of them firing cancels it.
    pub cancel: Vec<CancellationToken>,
    /// The task's cancellation parent is the runtime, not the spawner.
    pub root: bool,
}

/// The callable a launch `plan` (a `baml.spawn.Plan<T, E>` instance) runs.
pub fn plan_body(vm: &BexVm, plan: Value) -> Result<HeapPtr, VmInternalError> {
    let body = view::spawn::Plan {
        instance: vm.as_instance(&plan)?,
    }
    .body();
    body.as_object_ptr()
        .ok_or_else(|| VmInternalError::TypeError {
            expected: ObjectType::Closure.into(),
            got: vm.type_of(&body),
        })
}

/// Read the launch `plan` (a `baml.spawn.Plan<T, E>` instance) describes.
pub fn spawn_launch(vm: &BexVm, plan: Value) -> Result<SpawnLaunch, VmInternalError> {
    let body = plan_body(vm, plan)?;
    let instance = vm.as_instance(&plan)?;
    let [returns, throws] = &*instance.class_type_args else {
        return Err(VmInternalError::InvalidArgumentCount {
            expected: 2,
            got: instance.class_type_args.len(),
        });
    };
    let plan = view::spawn::Plan { instance };
    let limits = plan
        .limits(vm)
        .iter()
        .try_fold(LimitSet::new(), |limits, &limit| {
            Ok::<_, VmInternalError>(limits.with(&limit_inner(vm, limit)?))
        })?;
    let mut cancel = Vec::new();
    for &token in plan.cancel_tokens(vm).iter() {
        cancel.extend(cancel_token_members(vm, token).ok_or_else(|| {
            VmInternalError::TypeError {
                expected: ObjectType::RustData.into(),
                got: vm.type_of(&token),
            }
        })?);
    }
    Ok(SpawnLaunch {
        body,
        name: plan.name(vm).map(str::to_owned),
        returns: returns.clone(),
        throws: throws.clone(),
        limits,
        cancel,
        root: plan.root(),
    })
}

/// Allocate a fresh `baml.spawn.Limit` instance wrapping `inner`.
fn alloc_limit(vm: &mut BexVm, inner: Arc<LimitInner>) -> Value {
    let class = vm.resolve_class("baml.spawn.Limit");
    let handle = Value::object(vm.alloc_rust_data(inner));
    Value::object(vm.alloc_instance(class, vec![handle]))
}

/// The shared admission state behind a `baml.spawn.Limit` value.
fn limit_inner(vm: &BexVm, limit: Value) -> Result<Arc<LimitInner>, VmInternalError> {
    let handle = vm.as_instance(&limit)?.load_field(0);
    let mismatch = || VmInternalError::TypeError {
        expected: ObjectType::RustData.into(),
        got: vm.type_of(&handle),
    };
    match handle.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::RustData(data)) => Arc::clone(data)
            .downcast::<LimitInner>()
            .map_err(|_| mismatch()),
        _ => Err(mismatch()),
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSpawnLimit for PackageBamlImpl {
    // Static constructor: returns the heap `Limit` instance, not `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm, capacity: i64) -> Result<Value, VmRustFnError> {
        let capacity = usize::try_from(capacity)
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or_else(|| VmBamlError::InvalidArgument {
                message: format!("baml.spawn.Limit.new: capacity must be positive, got {capacity}"),
            })?;
        Ok(alloc_limit(vm, LimitInner::new(capacity)))
    }

    fn capacity(vm: &BexVm, limit: &view::spawn::Limit<'_>) -> i64 {
        clamp_to_i64(limit._handle::<LimitInner>(vm).capacity().get())
    }

    fn active_count(vm: &BexVm, limit: &view::spawn::Limit<'_>) -> i64 {
        clamp_to_i64(limit._handle::<LimitInner>(vm).active_count())
    }

    fn queued_count(vm: &BexVm, limit: &view::spawn::Limit<'_>) -> i64 {
        clamp_to_i64(limit._handle::<LimitInner>(vm).queued_count())
    }
}

// The namespace declares no free functions of its own beyond `__spawn`,
// which the compiler lowers at the call site.
impl BamlNamespaceSpawn for PackageBamlImpl {}
