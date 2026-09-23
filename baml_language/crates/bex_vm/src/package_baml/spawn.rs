//! Native implementations for `namespace baml.spawn`: `Plan`, `Execution`,
//! `Limit`, `Root`, and `CancelToken`.
//!
//! A `Plan` instance's `_handle` holds an `Object::SpawnPlan` — the sealed,
//! traced recipe — and an `Execution` instance holds that same handle plus an
//! `ExecutionState` (`Object::RustData`). Every native that reads a handle is
//! fallible: a malformed one is an internal error, never a wrong-typed value.
//!
//! `CancelToken` and `Limit` are opaque classes whose `_handle` field holds an
//! `Object::RustData(Arc<…>)`. They are built with the standard
//! `resolve_class` + `alloc_rust_data` + `alloc_instance` pattern and read
//! back via `as_instance` + `load_field(0)` + `as_rust_data`.
#![allow(unsafe_code)]

use std::{collections::HashMap, num::NonZeroUsize, sync::Arc};

use bex_heap::TlabHolder;
use bex_vm_types::{
    ExecutionExtent, ExecutionMisuse, ExecutionState, HeapPtr, LimitInner, Object, ObjectType,
    RealizedTy, SpawnPlanData,
    types::{CancellationToken, Value},
};

use super::{
    BamlClassSpawnCancelToken, BamlClassSpawnExecution, BamlClassSpawnLimit,
    BamlClassSpawnModifier_T__E__for_CancelToken, BamlClassSpawnModifier_T__E__for_Limit,
    BamlClassSpawnModifier_T__E__for_Root, BamlClassSpawnPlan, BamlNamespaceSpawn, Continuation,
    NativeCallResult, PackageBamlImpl, view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmPanic, VmRustFnError},
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

impl BamlClassSpawnModifier_T__E__for_CancelToken for PackageBamlImpl {
    /// Links the token into the plan: once it fires, the task is cancelled.
    /// A composite links each token it fires on.
    fn apply(vm: &mut BexVm, canceltoken: &Value, plan: &Value) -> Result<Value, VmRustFnError> {
        let tokens =
            cancel_token_members(vm, *canceltoken).ok_or_else(|| VmInternalError::TypeError {
                expected: ObjectType::RustData.into(),
                got: vm.type_of(canceltoken),
            })?;
        let linked = plan_data(vm, *plan)?.with_cancel(tokens);
        Ok(alloc_plan(vm, linked))
    }
}

/// The sealed recipe behind a `baml.spawn.Plan` value.
fn plan_data(vm: &BexVm, plan: Value) -> Result<&SpawnPlanData, VmInternalError> {
    let handle = vm.as_instance(&plan)?.load_field(0);
    plan_data_of_handle(vm, handle)
}

/// The recipe a `_handle`/`_plan` field value points at.
fn plan_data_of_handle(vm: &BexVm, handle: Value) -> Result<&SpawnPlanData, VmInternalError> {
    match handle.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::SpawnPlan(data)) => Ok(data),
        _ => Err(VmInternalError::TypeError {
            expected: ObjectType::SpawnPlan.into(),
            got: vm.type_of(&handle),
        }),
    }
}

/// Allocate a `baml.spawn.Plan<T, E>` instance around `data`; `T`/`E` are the
/// types a launch of it yields.
fn alloc_plan(vm: &mut BexVm, data: SpawnPlanData) -> Value {
    let class = vm.resolve_class("baml.spawn.Plan");
    let (returns, throws) = data.future_types();
    let type_args = Box::new([returns.clone(), throws.clone()]);
    let handle = Value::object(vm.tlab.alloc(Object::SpawnPlan(Box::new(data))));
    Value::object(
        vm.tlab
            .alloc_instance_with_type_args(class, type_args, vec![handle]),
    )
}

/// Allocate the `baml.spawn.Execution<T, E>` handed to `layer` of the plan
/// behind `plan_handle`: `T`/`E` are the types of the plan beneath that layer.
/// Returns the value and the state it shares with every copy of itself.
pub fn alloc_execution(
    vm: &mut BexVm,
    plan_handle: Value,
    layer: usize,
) -> Result<(Value, Arc<ExecutionState>), VmInternalError> {
    let plan = plan_data_of_handle(vm, plan_handle)?;
    let (returns, throws) = plan.types_beneath(layer);
    let type_args = Box::new([returns.clone(), throws.clone()]);
    let state = Arc::new(ExecutionState::new(plan.id, layer));
    let class = vm.resolve_class("baml.spawn.Execution");
    let state_handle = Value::object(
        vm.alloc_rust_data(Arc::clone(&state) as Arc<dyn std::any::Any + Send + Sync>),
    );
    let execution = Value::object(vm.tlab.alloc_instance_with_type_args(
        class,
        type_args,
        vec![plan_handle, state_handle],
    ));
    Ok((execution, state))
}

/// The last two type arguments of the running call: `T`, `E` for `Plan.new`,
/// and `Output`, `Error` for `Plan.wrap` (whose receiver's come first).
fn trailing_type_arg_pair(vm: &BexVm) -> Result<(RealizedTy, RealizedTy), VmInternalError> {
    match vm.current_call_type_args() {
        [.., returns, throws] => Ok((returns.clone(), throws.clone())),
        other => Err(VmInternalError::InvalidArgumentCount {
            expected: 2,
            got: other.len(),
        }),
    }
}

/// The heap object a callable argument points at.
fn callable_ptr(vm: &BexVm, callable: Value) -> Result<HeapPtr, VmInternalError> {
    callable
        .as_object_ptr()
        .ok_or_else(|| VmInternalError::TypeError {
            expected: ObjectType::Closure.into(),
            got: vm.type_of(&callable),
        })
}

impl BamlClassSpawnPlan for PackageBamlImpl {
    // Static constructor: returns the heap `Plan` instance, not `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(
        vm: &mut BexVm,
        body: &Value,
        name: Option<&bex_str::BexStr>,
    ) -> Result<Value, VmRustFnError> {
        let body = callable_ptr(vm, *body)?;
        let (returns, throws) = trailing_type_arg_pair(vm)?;
        let data = SpawnPlanData::new(name.cloned(), body, returns, throws);
        Ok(alloc_plan(vm, data))
    }

    fn wrap(vm: &mut BexVm, plan: &Value, wrapper: &Value) -> Result<Value, VmRustFnError> {
        let wrapper = callable_ptr(vm, *wrapper)?;
        let (returns, throws) = trailing_type_arg_pair(vm)?;
        let wrapped = plan_data(vm, *plan)?.wrapped(wrapper, returns, throws);
        Ok(alloc_plan(vm, wrapped))
    }

    fn name(
        vm: &BexVm,
        plan: &view::spawn::Plan<'_>,
    ) -> Result<Option<bex_str::BexStr>, VmRustFnError> {
        let data = plan_data_of_handle(vm, plan.instance.load_field(0))?;
        Ok(data.name.clone())
    }
}

/// Closes what one `Execution.run` opened, however the call it made ends: the
/// run is finished, and the `Execution` handed to the layer beneath expires
/// with that layer's wrapper. Dropped on unwind as well as on return, so a
/// throw out of the wrapper closes the extent too.
struct RunContinuation {
    running: Arc<ExecutionState>,
    beneath: Option<ExecutionExtent>,
}

impl Drop for RunContinuation {
    fn drop(&mut self) {
        self.running.finish();
    }
}

impl Continuation for RunContinuation {
    fn call(self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        NativeCallResult::Done(value)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        Vec::new()
    }

    fn apply_forwarding(&mut self, _forwarding: &HashMap<HeapPtr, HeapPtr>) {}
}

/// The state behind an `Execution` instance's `_handle`.
fn execution_state(vm: &BexVm, handle: Value) -> Result<Arc<ExecutionState>, VmInternalError> {
    let mismatch = || VmInternalError::TypeError {
        expected: ObjectType::RustData.into(),
        got: vm.type_of(&handle),
    };
    match handle.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::RustData(data)) => Arc::clone(data)
            .downcast::<ExecutionState>()
            .map_err(|_| mismatch()),
        _ => Err(mismatch()),
    }
}

fn misuse_panic(misuse: ExecutionMisuse) -> VmPanic {
    VmPanic::UserPanic {
        message: match misuse {
            ExecutionMisuse::WrongPlan => {
                "baml.spawn.Execution: this execution belongs to a different plan"
            }
            ExecutionMisuse::RanTwice => "baml.spawn.Execution.run: already called",
            ExecutionMisuse::OutsideExtent => {
                "baml.spawn.Execution.run: the wrapper that received this execution has returned"
            }
        }
        .to_string(),
    }
}

impl BamlClassSpawnExecution for PackageBamlImpl {
    fn run(vm: &mut BexVm, execution: &Value) -> NativeCallResult {
        let prepared: Result<_, VmRustFnError> = (|| {
            let instance = vm.as_instance(execution)?;
            let plan_handle = instance.load_field(0);
            let running = execution_state(vm, instance.load_field(1))?;
            let plan = plan_data_of_handle(vm, plan_handle)?;
            running.begin(plan).map_err(misuse_panic)?;
            Ok((plan_handle, running))
        })();
        let (plan_handle, running) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return NativeCallResult::Error(error),
        };
        // From here the continuation owns the run: dropping it finishes it.
        let mut continuation = RunContinuation {
            running,
            beneath: None,
        };
        let call = (|| {
            let plan = plan_data_of_handle(vm, plan_handle)?;
            match continuation.running.layer().checked_sub(1) {
                // The innermost layer's execution runs the body itself.
                None => Ok((plan.body, Vec::new())),
                Some(beneath) => {
                    let wrapper = plan.layers[beneath].wrapper;
                    let (execution, state) = alloc_execution(vm, plan_handle, beneath)?;
                    continuation.beneath = Some(ExecutionExtent::new(state));
                    Ok((wrapper, vec![execution]))
                }
            }
        })();
        match call {
            Ok((callee, args)) => NativeCallResult::YieldToCall {
                callee,
                args,
                type_args: Vec::new(),
                continuation: Box::new(continuation),
            },
            Err(error) => NativeCallResult::Error(VmRustFnError::InternalError(error)),
        }
    }

    fn name(
        vm: &BexVm,
        execution: &view::spawn::Execution<'_>,
    ) -> Result<Option<bex_str::BexStr>, VmRustFnError> {
        let data = plan_data_of_handle(vm, execution.instance.load_field(0))?;
        Ok(data.name.clone())
    }
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

impl BamlClassSpawnModifier_T__E__for_Limit for PackageBamlImpl {
    /// Admits the plan through the limit as well; the same limit again changes
    /// nothing.
    fn apply(vm: &mut BexVm, limit: &Value, plan: &Value) -> Result<Value, VmRustFnError> {
        let limit = limit_inner(vm, *limit)?;
        let admitted = plan_data(vm, *plan)?.with_limit(&limit);
        Ok(alloc_plan(vm, admitted))
    }
}

impl BamlClassSpawnModifier_T__E__for_Root for PackageBamlImpl {
    /// Makes the runtime the plan's cancellation parent.
    fn apply(vm: &mut BexVm, _root: &Value, plan: &Value) -> Result<Value, VmRustFnError> {
        let rooted = plan_data(vm, *plan)?.rooted();
        Ok(alloc_plan(vm, rooted))
    }
}

// The namespace declares no free functions of its own beyond `__spawn`,
// which the compiler lowers at the call site.
impl BamlNamespaceSpawn for PackageBamlImpl {}
