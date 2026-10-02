//! The common commit point for named functions, methods and callable entries.
//!
//! Target resolution, argument conversion and generic narrowing finish before
//! constructing prepared state. Admission consumes that state exactly once.
//! Defaults, hooks, finalizers and target code run only in active execution.

use std::sync::Arc;

use bex_vm_types::{HeapPtr, RealizedTy, Value, types::TypeValue};
use indexmap::IndexMap;
#[cfg(not(target_arch = "wasm32"))]
use tokio::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::{
    ActiveHeapPermit, BexCallResult, BexEngine, BoundaryContext, CallId, EngineError,
    LogCaptureContext, RootCallWork, RuntimeTy, ThreadOutcome, logger::TraceLogger,
    thread::BexThread,
};

/// Absolute runtime-monotonic time. Never reconstructed from a relative
/// timeout when an invocation crosses a callback or execution boundary.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InvocationDeadline {
    at: Instant,
}

/// One clock domain per runtime, shared by all host entries and callbacks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InvocationClock {
    epoch: Instant,
}

impl InvocationClock {
    pub(crate) fn new() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }

    pub(crate) fn now_ns(self) -> Result<u64, EngineError> {
        u64::try_from(self.epoch.elapsed().as_nanos()).map_err(|_| EngineError::TypeMismatch {
            message: "runtime monotonic clock exceeds the invocation protocol's range".into(),
        })
    }

    pub(crate) fn deadline_ns(self, deadline: InvocationDeadline) -> Option<u64> {
        u64::try_from(deadline.at.saturating_duration_since(self.epoch).as_nanos()).ok()
    }

    pub(crate) fn deadline(self, deadline_ns: u64) -> Result<InvocationDeadline, EngineError> {
        self.epoch
            .checked_add(std::time::Duration::from_nanos(deadline_ns))
            .map(|at| InvocationDeadline { at })
            .ok_or_else(|| EngineError::TypeMismatch {
                message: "invocation deadline exceeds the monotonic clock's range".into(),
            })
    }
}

impl InvocationDeadline {
    pub(crate) fn after(timeout: std::time::Duration) -> Result<Self, EngineError> {
        Instant::now()
            .checked_add(timeout)
            .map(|at| Self { at })
            .ok_or_else(|| EngineError::TypeMismatch {
                message: "invocation timeout exceeds the monotonic clock's range".into(),
            })
    }

    pub(crate) fn earliest(self, other: Self) -> Self {
        Self {
            at: self.at.min(other.at),
        }
    }

    pub(crate) fn is_expired(self) -> bool {
        Instant::now() >= self.at
    }

    pub(crate) async fn elapsed(self) {
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep_until(self.at).await;
        #[cfg(target_arch = "wasm32")]
        {
            // Keep JS timers on the local executor. The outer future holds
            // only Send channel state (VmSpawner requires Send). Dropping
            // this waiter stops and releases the local timer.
            let (finished, wait) = tokio::sync::oneshot::channel();
            let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
            wasm_bindgen_futures::spawn_local(async move {
                let timer = async {
                    while !self.is_expired() {
                        let remaining = self.at.saturating_duration_since(Instant::now());
                        let millis = remaining.as_nanos().div_ceil(1_000_000);
                        // Round up and recheck the absolute clock. JS timers
                        // use signed milliseconds, so long waits need chunks.
                        let millis = u32::try_from(millis.min(i32::MAX as u128))
                            .expect("bounded timer delay");
                        gloo_timers::future::TimeoutFuture::new(millis).await;
                    }
                };
                tokio::select! {
                    () = timer => { let _ = finished.send(()); },
                    _ = stopped => {},
                }
            });
            wait.await
                .expect("deadline timer terminated before its deadline");
            drop(stop);
        }
    }
}

/// Fully resolved VM inputs, still without an active call or VM entry frame.
///
/// The private preparation path holds the heap permit until admission and
/// entry setup, without yielding between them. This pins every raw VM value;
/// no unrooted inputs cross a GC safepoint.
pub(super) struct PreparedInvocation {
    pub runtime: Arc<BexEngine>,
    pub thread: ActiveHeapPermit<BexThread>,
    pub entry_ptr: HeapPtr,
    pub vm_args: Vec<Value>,
    pub type_args: IndexMap<String, RealizedTy>,
    pub type_values: IndexMap<String, TypeValue>,
    pub return_type: RuntimeTy,
    pub throws_type: Option<RuntimeTy>,
    pub host_call_id: CallId,
    pub boundary: BoundaryContext,
    pub logger: TraceLogger,
    pub copy_objects: bool,
}

impl PreparedInvocation {
    pub(super) fn admit(self) -> Result<ActiveInvocation, EngineError> {
        let cancel = self.thread.vm_thread_cancel().clone();
        let registration = RootCallWork::register(
            Arc::clone(&self.runtime),
            self.host_call_id,
            cancel,
            self.thread.entry_reservation.as_deref(),
        )?;
        Ok(ActiveInvocation {
            prepared: self,
            registration,
        })
    }
}

/// Committed execution. Its registration lives through the actual VM result,
/// including cleanup, and is released on normal return or panic unwind.
pub(super) struct ActiveInvocation {
    // Drop VM inputs/permit before releasing execution ownership.
    prepared: PreparedInvocation,
    registration: RootCallWork,
}

impl ActiveInvocation {
    pub(super) async fn execute(self) -> Result<BexCallResult, EngineError> {
        let Self {
            prepared:
                PreparedInvocation {
                    runtime,
                    mut thread,
                    entry_ptr,
                    vm_args,
                    type_args,
                    type_values,
                    return_type,
                    throws_type,
                    host_call_id,
                    boundary,
                    logger,
                    copy_objects,
                },
            registration,
        } = self;

        let execution = async {
            if let Some(reservation) = thread.entry_reservation.take() {
                thread
                    .vm
                    .set_entry_trace(&reservation.options, registration.reserved_id);
            }
            thread
                .vm
                .set_entry_point_with_type_values(entry_ptr, &vm_args, type_args, type_values);
            runtime.record_invocation_frame(host_call_id, &thread);
            // Entry setup roots the target, arguments and type values before
            // parking. GC may execute finalizers, so it belongs after admission.
            drop(vm_args);
            let inactive = thread.release();
            Box::pin(runtime.collect_before_call(&registration.work_guard)).await;
            let thread = inactive.acquire().await;
            let log_capture = logger.is_enabled().then(|| LogCaptureContext {
                boundary_id: boundary.boundary_id,
                logger: logger.clone(),
            });
            runtime
                .run_thread_event_loop_inner(
                    return_type,
                    throws_type,
                    thread,
                    host_call_id,
                    log_capture,
                    copy_objects,
                )
                .await
        };
        #[cfg(not(target_arch = "wasm32"))]
        let result = match &runtime.telemetry {
            Some(telemetry) => telemetry.runtime.scope(execution).await,
            None => execution.await,
        };
        #[cfg(target_arch = "wasm32")]
        let result = execution.await;

        // No heap permit remains; host release callbacks may safely run.
        bex_external_types::host_value::host_release_dispatch::drain();
        drop(registration);

        match result {
            Ok(ThreadOutcome::RootValue(value)) => Ok(BexCallResult { value: Ok(value) }),
            Ok(ThreadOutcome::SettledChild(_)) => Ok(BexCallResult {
                value: Err(EngineError::Other(
                    "BEP-034: root thread terminated as SettledChild".to_string(),
                )),
            }),
            Err(err) => Ok(BexCallResult { value: Err(err) }),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use baml_db::testing::compile_source;
    use sys_native::SysOpsExt;

    use super::*;
    use crate::{CancellationToken, is_cancelled_engine_error};

    fn engine() -> Arc<BexEngine> {
        Arc::new(
            BexEngine::new(
                {
                    static PROGRAM: std::sync::OnceLock<bex_vm_types::Program> =
                        std::sync::OnceLock::new();
                    PROGRAM
                        .get_or_init(|| {
                            compile_source(
                                r#"
                    function deferred() -> int { baml.sys.panic("default executed") }
                    function main(n: int = deferred()) -> int { n }
                    function callable() -> (int) -> int throws never { (n: int) -> { n } }
                    "#,
                            )
                        })
                        .clone()
                },
                Arc::new(sys_ops::SysOps::native()),
                Vec::new(),
            )
            .unwrap(),
        )
    }

    async fn prepared(
        engine: &Arc<BexEngine>,
        input: &CancellationToken,
        value: Value,
    ) -> PreparedInvocation {
        prepared_with_context(
            engine,
            crate::FunctionCallContextBuilder::new(CallId::next())
                .with_cancel_token(input.clone())
                .build(),
            value,
        )
        .await
    }

    async fn prepared_with_context(
        engine: &Arc<BexEngine>,
        mut context: crate::FunctionCallContext,
        value: Value,
    ) -> PreparedInvocation {
        context
            .bind_timeout(engine.engine_id, engine.invocation_clock)
            .unwrap();
        let mut thread = engine
            .new_entry_thread(CancellationToken::new(), context.inherited_state.as_ref())
            .await;
        let (entry, _) = engine.lookup_function("main").unwrap();
        engine
            .prepare_invocation_state(
                &mut thread,
                context.inherited_state.as_ref(),
                context.trace_options,
                context.trace_reservation,
                context.host_environment,
            )
            .unwrap();
        engine
            .prepare_cancellation(
                &mut thread,
                Some(entry),
                context.cancel,
                &context.cancel_tokens,
                context.inherited_state,
                context.deadline,
            )
            .unwrap();
        engine
            .prepare_entry_point(
                thread,
                entry,
                vec![value],
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
                RuntimeTy::Int,
                None,
                context.host_call_id,
                context.boundary,
                context.logger,
                true,
            )
            .unwrap()
    }

    #[tokio::test]
    async fn preparation_and_admission_defer_defaults_until_execution() {
        let engine = engine();
        let call = prepared(&engine, &CancellationToken::new(), Value::OMITTED_ARG).await;
        assert_eq!(engine.active_call_count(), 0);
        let active = call.admit().unwrap();
        assert_eq!(engine.active_call_count(), 1);
        let result = active.execute().await.unwrap().value;
        assert!(result.unwrap_err().to_string().contains("default executed"));
        assert_eq!(engine.active_call_count(), 0);
    }

    #[tokio::test]
    async fn cancellation_between_preparation_and_admission_never_activates() {
        let engine = engine();
        let input = CancellationToken::new();
        let call = prepared(&engine, &input, Value::OMITTED_ARG).await;
        let id = call.host_call_id;
        engine.cancel_function_call(id).unwrap();
        let Err(error) = call.admit() else {
            panic!("pre-cancelled invocation must not admit");
        };
        assert!(is_cancelled_engine_error(&error));
        assert_eq!(engine.active_call_count(), 0);
        assert!(!engine.active_calls.lock().unwrap().contains_key(&id));
        assert!(!input.is_cancelled());
    }

    #[tokio::test]
    async fn abandoning_preparation_releases_inputs_without_starting_execution() {
        let engine = engine();
        let call = prepared(&engine, &CancellationToken::new(), Value::OMITTED_ARG).await;
        drop(call);
        assert_eq!(engine.active_call_count(), 0);
        let parked = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            engine.heap_permit_manager.request_park(),
        )
        .await
        .expect("abandoned preparation must not retain a heap permit");
        drop(parked);
    }

    fn reservation(engine: &BexEngine) -> Arc<bex_vm_types::trace::ReservedSpanData> {
        Arc::new(bex_vm_types::trace::ReservedSpanData::new(
            engine.trace_scope,
            bex_vm_types::trace::TraceOptionsData::default(),
        ))
    }

    #[tokio::test]
    async fn reservation_is_consumed_at_admission_before_execution() {
        let engine = engine();
        let reserved = reservation(&engine);
        let call = prepared_with_context(
            &engine,
            crate::FunctionCallContextBuilder::new(CallId::next())
                .with_trace_reservation(reserved.clone())
                .build(),
            Value::OMITTED_ARG,
        )
        .await;
        assert!(reserved.validate(engine.trace_scope).is_ok());
        let active = call.admit().unwrap();
        assert_eq!(
            reserved.validate(engine.trace_scope),
            Err(bex_vm_types::trace::ReservationError::AlreadyAttached)
        );
        // Defaults have not run. Their later failure cannot refund admission.
        assert!(active.execute().await.unwrap().value.is_err());
        assert_eq!(engine.active_call_count(), 0);
        assert!(reserved.validate(engine.trace_scope).is_err());
    }

    #[tokio::test]
    async fn cancellation_before_admission_leaves_reservation_available() {
        let engine = engine();
        let reserved = reservation(&engine);
        let id = CallId::next();
        let call = prepared_with_context(
            &engine,
            crate::FunctionCallContextBuilder::new(id)
                .with_trace_reservation(reserved.clone())
                .build(),
            Value::OMITTED_ARG,
        )
        .await;
        engine.cancel_function_call(id).unwrap();
        assert!(call.admit().is_err());
        assert!(reserved.validate(engine.trace_scope).is_ok());
        assert_eq!(engine.active_call_count(), 0);
    }

    #[tokio::test]
    async fn competing_preparations_cannot_both_admit_one_reservation() {
        let engine = engine();
        let reserved = reservation(&engine);
        let mut calls = Vec::new();
        for _ in 0..2 {
            calls.push(
                prepared_with_context(
                    &engine,
                    crate::FunctionCallContextBuilder::new(CallId::next())
                        .with_trace_reservation(reserved.clone())
                        .build(),
                    Value::int(42),
                )
                .await,
            );
        }
        let admitted = calls.pop().unwrap().admit().unwrap();
        assert!(calls.pop().unwrap().admit().is_err());
        assert_eq!(engine.active_call_count(), 1);
        drop(admitted);
        assert_eq!(engine.active_call_count(), 0);
        assert!(reserved.validate(engine.trace_scope).is_err());
    }

    #[tokio::test]
    async fn retained_state_owns_live_sources_without_owning_active_execution() {
        let engine = engine();
        let input = CancellationToken::new();
        let active = prepared(&engine, &input, Value::int(42))
            .await
            .admit()
            .unwrap();
        let id = active.prepared.host_call_id;
        engine.record_invocation_frame(id, &active.prepared.thread);
        let state = engine.invocation_state(id).unwrap();
        drop(active);
        assert_eq!(engine.active_call_count(), 0);
        assert!(engine.invocation_state(id).is_err());
        assert!(!state.is_cancelled());
        input.cancel();
        assert!(state.is_cancelled());
        state.cancelled().await;
        // Retention does not hold a VM heap permit or a running-work guard.
        let parked = engine.heap_permit_manager.request_park().await;
        drop(parked);
        tokio::time::timeout(std::time::Duration::from_secs(5), engine.shutdown())
            .await
            .expect("retained context must not prevent shutdown");
    }

    #[test]
    fn concurrent_reservation_admission_has_one_execution_owner() {
        let engine = engine();
        let reserved = reservation(&engine);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let admitting: Vec<_> = (0..2)
            .map(|_| {
                let engine = engine.clone();
                let reserved = reserved.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    RootCallWork::register(
                        engine,
                        CallId::next(),
                        crate::thread::TaskCancel::detached([]),
                        Some(&reserved),
                    )
                })
            })
            .collect();
        let results: Vec<_> = admitting
            .into_iter()
            .map(|task| task.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(engine.active_call_count(), 1);
        assert!(reserved.validate(engine.trace_scope).is_err());
        drop(results);
        assert_eq!(engine.active_call_count(), 0);
    }

    #[test]
    fn cancellation_racing_reservation_admission_has_one_commit_point() {
        let engine = engine();
        for _ in 0..32 {
            let reserved = reservation(&engine);
            let effective = crate::thread::TaskCancel::detached([]);
            let id = CallId::next();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let admitting = {
                let engine = engine.clone();
                let reserved = reserved.clone();
                let barrier = barrier.clone();
                let effective = effective.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    RootCallWork::register(engine, id, effective, Some(&reserved))
                })
            };
            barrier.wait();
            engine.cancel_function_call(id).unwrap();
            match admitting.join().unwrap() {
                Ok(registration) => {
                    assert!(reserved.validate(engine.trace_scope).is_err());
                    assert!(effective.is_cancelled());
                    assert_eq!(engine.active_call_count(), 1);
                    drop(registration);
                }
                Err(error) => {
                    assert!(is_cancelled_engine_error(&error));
                    assert!(reserved.validate(engine.trace_scope).is_ok());
                }
            }
            assert_eq!(engine.active_call_count(), 0);
        }
    }

    #[tokio::test]
    async fn invalid_arguments_take_precedence_over_pending_cancellation() {
        let engine = engine();
        let id = CallId::next();
        engine.cancel_function_call(id).unwrap();
        let result = engine
            .call_function(
                "main",
                vec![
                    crate::BexExternalValue::Int(1),
                    crate::BexExternalValue::Int(2),
                ],
                crate::FunctionCallContextBuilder::new(id).build(),
                true,
            )
            .await;
        assert!(matches!(result, Err(EngineError::TypeMismatch { .. })));
        assert_eq!(engine.active_call_count(), 0);
        engine.release_prepared_call(id);
        assert!(!engine.active_calls.lock().unwrap().contains_key(&id));
    }

    #[tokio::test]
    async fn admitted_cancellation_does_not_cancel_input_or_sibling() {
        let engine = engine();
        let input = CancellationToken::new();
        let first = prepared(&engine, &input, Value::int(42)).await;
        let sibling = prepared(&engine, &input, Value::int(42)).await;
        let active = first.admit().unwrap();
        engine
            .cancel_function_call(active.prepared.host_call_id)
            .unwrap();
        assert!(active.prepared.thread.vm_thread_cancel().is_cancelled());
        assert!(!input.is_cancelled());
        assert!(!sibling.thread.vm_thread_cancel().is_cancelled());
        // Release the first preparation permit before the sibling's GC point.
        drop(active);
        let result = sibling.admit().unwrap().execute().await.unwrap().value;
        assert_eq!(result.unwrap(), crate::BexExternalValue::Int(42));
        assert_eq!(engine.active_call_count(), 0);
    }

    #[test]
    fn cancellation_racing_admission_is_never_lost() {
        let engine = engine();
        for _ in 0..32 {
            let id = CallId::next();
            let input = CancellationToken::new();
            let effective = crate::thread::TaskCancel::detached([input.clone().into()]);
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let admitting = {
                let engine = Arc::clone(&engine);
                let barrier = Arc::clone(&barrier);
                let effective = effective.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    RootCallWork::register(engine, id, effective, None)
                })
            };
            barrier.wait();
            engine.cancel_function_call(id).unwrap();
            match admitting.join().unwrap() {
                Ok(registration) => {
                    assert!(effective.is_cancelled());
                    drop(registration);
                }
                Err(error) => assert!(is_cancelled_engine_error(&error)),
            }
            assert!(!input.is_cancelled());
            assert!(!engine.active_calls.lock().unwrap().contains_key(&id));
        }
    }

    #[tokio::test]
    async fn admitted_completion_is_not_replaced_by_current_cancellation_state() {
        let engine = engine();
        let input = CancellationToken::new();
        let call = prepared(&engine, &input, Value::int(42)).await;
        let active = call.admit().unwrap();
        engine
            .cancel_function_call(active.prepared.host_call_id)
            .unwrap();
        assert!(active.prepared.thread.vm_thread_cancel().is_cancelled());
        // Cancellation is a request until a runtime checkpoint delivers it.
        // A body that completes without another checkpoint owns its result;
        // adapters must not reinterpret it by checking a token afterward.
        assert_eq!(
            active.execute().await.unwrap().value.unwrap(),
            crate::BexExternalValue::Int(42)
        );
        assert!(!input.is_cancelled());
        assert_eq!(engine.active_call_count(), 0);
    }
    #[tokio::test(start_paused = true)]
    async fn preparation_consumes_timeout_budget_before_admission() {
        let engine = engine();
        let input = CancellationToken::new();
        let context = crate::FunctionCallContextBuilder::new(CallId::next())
            .with_cancel_token(input.clone())
            .with_timeout(std::time::Duration::from_secs(5))
            .build();
        let call = prepared_with_context(&engine, context, Value::OMITTED_ARG).await;
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        let Err(error) = call.admit() else {
            panic!("expired preparation must not admit")
        };
        assert!(is_cancelled_engine_error(&error));
        assert_eq!(engine.active_call_count(), 0);
        assert!(!input.is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_for_preparation_permit_consumes_timeout_budget() {
        let engine = engine();
        let parked = engine.heap_permit_manager.request_park().await;
        let call = engine.call_function_bound_args(
            "main",
            vec![crate::BexCallArg::OmittedDefault],
            crate::FunctionCallContextBuilder::new(CallId::next())
                .with_timeout(std::time::Duration::from_secs(5))
                .build(),
            true,
        );
        let mut call = std::pin::pin!(call);
        assert!(futures::poll!(&mut call).is_pending());
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        drop(parked);
        assert!(is_cancelled_engine_error(&call.await.unwrap_err()));
        assert_eq!(engine.active_call_count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_is_bound_once_even_when_context_crosses_binding_helpers() {
        let engine = engine();
        let mut context = crate::FunctionCallContextBuilder::new(CallId::next())
            .with_timeout(std::time::Duration::from_secs(5))
            .build();
        context
            .bind_timeout(engine.engine_id, engine.invocation_clock)
            .unwrap();
        tokio::time::advance(std::time::Duration::from_secs(3)).await;
        context
            .bind_timeout(engine.engine_id, engine.invocation_clock)
            .unwrap();
        tokio::time::advance(std::time::Duration::from_secs(2)).await;
        assert!(context.deadline.unwrap().is_expired());
    }
    #[tokio::test(start_paused = true)]
    async fn callback_spawner_retains_deadline_after_parent_waiter_exits() {
        use sys_types::VmSpawner as _;
        let engine = engine();
        let callable = engine
            .call_function(
                "callable",
                vec![],
                crate::FunctionCallContextBuilder::new(CallId::next()).build(),
                false,
            )
            .await
            .unwrap();
        let crate::BexExternalValue::Handle(callable) = callable else {
            panic!("expected callable")
        };
        let active = prepared_with_context(
            &engine,
            crate::FunctionCallContextBuilder::new(CallId::next())
                .with_timeout(std::time::Duration::from_secs(5))
                .build(),
            Value::int(42),
        )
        .await
        .admit()
        .unwrap();
        assert!(matches!(
            engine.invocation_state(active.prepared.host_call_id),
            Err(EngineError::FunctionCallNotFound { .. })
        ));
        // This fixture captures without driving execute(), so publish the
        // frame that execution installs before any callback can capture it.
        engine.record_invocation_frame(active.prepared.host_call_id, &active.prepared.thread);
        let spawner = Arc::new(crate::InvocationSpawner {
            runtime: Arc::clone(&engine),
            cancel: None,
            inherited: engine
                .invocation_state(active.prepared.host_call_id)
                .unwrap(),
        });
        drop(active);
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        assert!(
            Arc::clone(&spawner)
                .spawn_with_function(
                    "main".into(),
                    vec![crate::BexExternalValue::Int(42)],
                    CancellationToken::new()
                )
                .await
                .is_err()
        );
        assert!(
            spawner
                .spawn_with_callable(
                    callable,
                    vec![crate::BexExternalValue::Int(42)],
                    CancellationToken::new()
                )
                .await
                .is_err()
        );
        assert_eq!(engine.active_call_count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn callable_preparation_wait_consumes_timeout_budget() {
        let engine = engine();
        let callable = engine
            .call_function(
                "callable",
                vec![],
                crate::FunctionCallContextBuilder::new(CallId::next()).build(),
                false,
            )
            .await
            .unwrap();
        let crate::BexExternalValue::Handle(callable) = callable else {
            panic!("expected callable")
        };
        let parked = engine.heap_permit_manager.request_park().await;
        let call = engine.call_callable(
            callable,
            vec![crate::BexExternalValue::Int(42)],
            crate::FunctionCallContextBuilder::new(CallId::next())
                .with_timeout(std::time::Duration::from_secs(5))
                .build(),
            true,
        );
        let mut call = std::pin::pin!(call);
        assert!(futures::poll!(&mut call).is_pending());
        tokio::time::advance(std::time::Duration::from_secs(5)).await;
        drop(parked);
        assert!(is_cancelled_engine_error(&call.await.unwrap_err()));
        assert_eq!(engine.active_call_count(), 0);
    }
}
