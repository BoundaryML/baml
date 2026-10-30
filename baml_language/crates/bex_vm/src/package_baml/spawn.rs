//! Native implementations for `namespace baml.spawn`: `Limit` and
//! `CancelToken`, and [`spawn_launch`], which reads what a `spawn` starts out
//! of its `Plan`.
//!
//! `Limit` and `CancelToken` are `@@rust_type` classes whose state is their
//! payload.

use std::{num::NonZeroUsize, sync::Arc};

use bex_vm_types::{
    HeapPtr, LimitInner, LimitSet, ObjectType, RealizedTy, RustPayload,
    cancellation::{CancelTokenState, CancellationSource},
    types::{CancellationToken, Value},
};

use super::{
    BamlClassSpawnCancelToken, BamlClassSpawnLimit, BamlNamespaceSpawn, PackageBamlImpl, copy, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// The tokens a `baml.spawn.CancelToken` value fires on (see
/// [`CancelTokenState`]), or `None` if the value is not a `CancelToken`
/// (including the `OmittedArg` sentinel for an omitted optional argument).
pub fn cancel_token_members(vm: &BexVm, value: Value) -> Option<Vec<CancellationSource>> {
    match &vm.as_rust_instance(&value).ok()?.payload {
        RustPayload::CancelToken(state) => Some(state.sources().collect()),
        _ => None,
    }
}

/// A concrete generated token observing the effective task, including its
/// deadline. Its own authority cancels that task, never its input sources.
pub fn alloc_projected_cancel_token(
    vm: &mut BexVm,
    own: CancellationToken,
    inputs: Vec<CancellationSource>,
) -> Value {
    alloc_cancel_token(vm, CancelTokenState::new(own, inputs))
}

/// Allocate a `baml.spawn.CancelToken` instance holding `state`.
fn alloc_cancel_token(vm: &mut BexVm, state: CancelTokenState) -> Value {
    copy::spawn::CancelToken {
        payload: Arc::new(state),
    }
    .to_value(vm)
}

/// Convert a `usize` count/limit to `i64`, saturating at `i64::MAX`. Group
/// counts and limits are always small in practice; this just makes the cast
/// explicitly lossless-or-saturating for clippy.
fn clamp_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

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
            .collect::<Vec<_>>();
        alloc_cancel_token(vm, CancelTokenState::new(CancellationToken::new(), inputs))
    }

    fn cancel(_vm: &BexVm, canceltoken: &view::spawn::CancelToken<'_>) -> i64 {
        // Best-effort under concurrent cancels (see `CancelTokenState::cancel`).
        i64::from(canceltoken.payload().cancel())
    }

    fn is_cancelled(_vm: &BexVm, canceltoken: &view::spawn::CancelToken<'_>) -> bool {
        canceltoken.payload().is_cancelled()
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
    pub cancel: Vec<CancellationSource>,
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
    copy::spawn::Limit { payload: inner }.to_value(vm)
}

/// The shared admission state behind a `baml.spawn.Limit` value.
fn limit_inner(vm: &BexVm, limit: Value) -> Result<Arc<LimitInner>, VmInternalError> {
    let instance = vm.as_rust_instance(&limit)?;
    Ok(Arc::clone(view::spawn::Limit { instance }.payload()))
}

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

    fn capacity(_vm: &BexVm, limit: &view::spawn::Limit<'_>) -> i64 {
        clamp_to_i64(limit.payload().capacity().get())
    }

    fn active_count(_vm: &BexVm, limit: &view::spawn::Limit<'_>) -> i64 {
        clamp_to_i64(limit.payload().active_count())
    }

    fn queued_count(_vm: &BexVm, limit: &view::spawn::Limit<'_>) -> i64 {
        clamp_to_i64(limit.payload().queued_count())
    }
}

// The namespace declares no free functions of its own beyond `__spawn`,
// which the compiler lowers at the call site.
impl BamlNamespaceSpawn for PackageBamlImpl {}
