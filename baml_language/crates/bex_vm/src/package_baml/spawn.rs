//! Native implementations for `namespace baml.spawn`: `CancelToken`, and
//! [`spawn_launch`], which reads what a `spawn` starts out of its `Plan`
//! (including its `baml.sync.Semaphore`s).
//!
//! `CancelToken` is a `@@rust_type` class whose state is its payload.

use std::sync::Arc;

use bex_vm_types::{
    HeapPtr, LimitInner, LimitSet, ObjectType, RealizedTy, RustPayload,
    cancellation::{CancelTokenState, CancellationSource},
    types::{CancellationToken, Value},
};

use super::{BamlClassSpawnCancelToken, BamlNamespaceSpawn, PackageBamlImpl, copy, view};
use crate::{BexVm, errors::VmInternalError};

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
pub(super) fn clamp_to_i64(value: usize) -> i64 {
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
        instance: &vm.as_instance(&plan)?,
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
    let [returns, throws] = instance.type_args() else {
        return Err(VmInternalError::InvalidArgumentCount {
            expected: 2,
            got: instance.type_args().len(),
        });
    };
    let plan = view::spawn::Plan {
        instance: &instance,
    };
    let limits = plan
        .semaphores(vm)
        .iter()
        .try_fold(LimitSet::new(), |limits, &limit| {
            Ok::<_, VmInternalError>(limits.with(&limit_inner(vm, limit)?))
        })?;
    let mut cancel = Vec::new();
    for &token in plan.cancel_tokens(vm).iter() {
        cancel.extend(cancel_token_members(vm, token).ok_or_else(|| {
            VmInternalError::TypeError {
                expected: ObjectType::RustInstance.into(),
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

/// The shared admission state behind a `baml.sync.Semaphore` value.
pub(super) fn limit_inner(vm: &BexVm, limit: Value) -> Result<Arc<LimitInner>, VmInternalError> {
    let instance = vm.as_rust_instance(&limit)?;
    Ok(Arc::clone(view::sync::Semaphore { instance }.payload()))
}

// The namespace declares no free functions of its own beyond `__spawn`,
// which the compiler lowers at the call site.
impl BamlNamespaceSpawn for PackageBamlImpl {}
