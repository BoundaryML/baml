use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm::telemetry::InvocationOutcome;
use bex_vm_types::{Future, RealizedTy};
use sys_native::SysOpsExt;

use super::*;

fn engine(source: &str) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            baml_db::testing::compile_source(source),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    )
}

async fn entry(
    engine: &Arc<BexEngine>,
    cancel: &CancellationToken,
    args: &[Value],
) -> ActiveHeapPermit<BexThread> {
    let mut thread = engine.new_root_thread(cancel.clone()).await;
    let (function, _) =
        &engine.resolved_function_names[engine.resolve_function_name("Main").unwrap()];
    thread.vm.set_entry_point(*function, args);
    thread
}

fn assert_completed(engine: &BexEngine, expected: &[(u64, InvocationOutcome)]) {
    let mut actual = engine.completed_threads.lock().unwrap().clone();
    actual.sort_by_key(|(id, _)| *id);
    assert_eq!(
        actual, expected,
        "each logical thread must finish exactly once"
    );
}

#[tokio::test]
async fn conversion_errors_and_terminal_outcomes_finalize() {
    for (source, return_type, throws_type, cancelled, outcome) in [
        (
            "function Main() -> int { 1 }",
            RuntimeTy::int(),
            None,
            false,
            InvocationOutcome::Ok,
        ),
        (
            "function Main() -> int { 1 }",
            RuntimeTy::union([RuntimeTy::string(), RuntimeTy::bool()]),
            None,
            false,
            InvocationOutcome::Errored,
        ),
        (
            "function Main() -> int throws int { throw 7; }",
            RuntimeTy::int(),
            Some(RuntimeTy::union([RuntimeTy::string(), RuntimeTy::bool()])),
            false,
            InvocationOutcome::Errored,
        ),
        (
            "function Main() -> int { 1 }",
            RuntimeTy::int(),
            None,
            true,
            InvocationOutcome::Cancelled,
        ),
        (
            "function Main() -> int { baml.sys.exit(7); 1 }",
            RuntimeTy::int(),
            None,
            false,
            InvocationOutcome::Errored,
        ),
    ] {
        let engine = engine(source);
        let cancel = CancellationToken::new();
        let thread = entry(&engine, &cancel, &[]).await;
        let id = thread.vm.thread_id;
        // Set cancellation after entry so the loop's cancel-wins path, not
        // call_function's preflight cancellation check, owns completion.
        if cancelled {
            cancel.cancel();
        }
        let result = engine
            .run_thread_event_loop(
                return_type,
                throws_type,
                thread,
                CallId::next(),
                None,
                &cancel,
                true,
            )
            .await;
        if source.contains("baml.sys.exit") {
            assert!(matches!(result, Err(EngineError::Exit { code: 7 })));
        } else {
            match outcome {
                InvocationOutcome::Ok => assert!(matches!(
                    result,
                    Ok(ThreadOutcome::RootValue(BexExternalValue::Int(1)))
                )),
                InvocationOutcome::Errored => {
                    assert!(matches!(result, Err(EngineError::TypeMismatch { .. })));
                }
                InvocationOutcome::Cancelled => {
                    assert!(is_cancelled_engine_error(result.as_ref().err().unwrap()));
                }
            }
        }
        assert_completed(&engine, &[(id, outcome)]);
    }
}

#[tokio::test]
async fn ready_and_suspended_sysop_failures_finalize() {
    for suspended in [false, true] {
        let mut ops = sys_native::SysOps::native();
        ops.baml_sys_sleep = Arc::new(move |_, _, _, _, _| {
            let failure = Err(sys_types::OpError::new(
                SysOp::BamlSysSleep,
                bex_vm_types::errors::VmInternalError::BridgeFailure {
                    message: "test sysop failure".into(),
                },
            ));
            if suspended {
                SysOpResult::Async(Box::pin(async move {
                    tokio::task::yield_now().await;
                    failure
                }))
            } else {
                SysOpResult::Ready(failure)
            }
        });
        let engine = Arc::new(BexEngine::new(baml_db::testing::compile_source("function Main() -> int { baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n)); 1 }"), Arc::new(ops), vec![]).unwrap());
        let cancel = CancellationToken::new();
        let thread = entry(&engine, &cancel, &[]).await;
        let id = thread.vm.thread_id;
        let result = engine
            .run_thread_event_loop(
                RuntimeTy::int(),
                None,
                thread,
                CallId::next(),
                None,
                &cancel,
                true,
            )
            .await;
        assert!(result.is_err());
        assert_completed(&engine, &[(id, InvocationOutcome::Errored)]);
    }
}

#[tokio::test]
async fn await_lookup_failure_finalizes() {
    for body in ["await f", "baml.future.__await_any([f])"] {
        let engine = engine(&format!(
            "function Main(f: baml.future.Future<int, never>) -> int {{ {body} }}"
        ));
        let cancel = CancellationToken::new();
        let mut thread = engine.new_root_thread(cancel.clone()).await;
        // A pending heap future with an unissued ID passes the VM's Await but
        // fails the engine registry lookup before releasing the permit.
        let future = thread.vm.tlab_mut().alloc_future(Future::pending(
            FutureId::from_usize(usize::MAX),
            RealizedTy::int(),
            RealizedTy::never(),
            cancel.clone(),
        ));
        let (function, _) =
            &engine.resolved_function_names[engine.resolve_function_name("Main").unwrap()];
        thread
            .vm
            .set_entry_point(*function, &[Value::object(future)]);
        let id = thread.vm.thread_id;
        let result = engine
            .run_thread_event_loop(
                RuntimeTy::int(),
                None,
                thread,
                CallId::next(),
                None,
                &cancel,
                true,
            )
            .await;
        assert!(matches!(result, Err(EngineError::FutureNotFound { .. })));
        assert_completed(&engine, &[(id, InvocationOutcome::Errored)]);
    }
}

#[tokio::test]
async fn suspended_await_failure_and_cancellation_finalize() {
    for body in ["await f", "baml.future.__await_any([f])"] {
        for cancelled in [false, true] {
            let engine = engine(&format!(
                "function Main(f: baml.future.Future<int, never>) -> int {{ {body} }}"
            ));
            let cancel = CancellationToken::new();
            let mut thread = engine.new_root_thread(cancel.clone()).await;
            let (future_id, future) = engine.futures.acquire(thread.proof()).await.new_future(
                RealizedTy::int(),
                RealizedTy::never(),
                CancellationToken::new(),
                "test".into(),
            );
            let (function, _) =
                &engine.resolved_function_names[engine.resolve_function_name("Main").unwrap()];
            thread
                .vm
                .set_entry_point(*function, &[Value::object(future)]);
            let id = thread.vm.thread_id;
            let running = engine.run_thread_event_loop(
                RuntimeTy::int(),
                None,
                thread,
                CallId::next(),
                None,
                &cancel,
                true,
            );
            tokio::pin!(running);
            assert!(futures::poll!(&mut running).is_pending());
            let admin = engine
                .heap_permit_manager
                .new_permit(())
                .await
                .acquire()
                .await;
            // The waiter receives this error only after reacquisition. With both
            // branches ready, cancellation must win the biased select.
            engine
                .futures
                .acquire(admin.proof())
                .await
                .internal_error_future(future_id, EngineError::Other("test await failure".into()))
                .unwrap();
            if cancelled {
                cancel.cancel();
            }
            drop(admin);
            let result = running.await;
            if cancelled {
                assert!(is_cancelled_engine_error(result.as_ref().err().unwrap()));
            } else {
                assert!(
                    matches!(result, Err(EngineError::Other(ref message)) if message == "test await failure")
                );
            }
            assert_completed(
                &engine,
                &[(
                    id,
                    if cancelled {
                        InvocationOutcome::Cancelled
                    } else {
                        InvocationOutcome::Errored
                    },
                )],
            );
        }
    }
}

#[tokio::test]
async fn child_success_and_error_settle_and_finalize_once() {
    for (body, outcome, child_outcome) in [
        ("7", InvocationOutcome::Ok, InvocationOutcome::Ok),
        (
            "throw 7;",
            InvocationOutcome::Errored,
            InvocationOutcome::Errored,
        ),
        // A child settles its escaping Exit as an error value; the root
        // recognizes that value as explicit process exit when it is awaited.
        (
            "baml.sys.exit(7); 1",
            InvocationOutcome::Errored,
            InvocationOutcome::Errored,
        ),
    ] {
        let engine = engine(&format!(
            "function Main() -> int throws int {{ let f = spawn {{ {body} }}; await f }}"
        ));
        let result = engine
            .call_function(
                "Main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next()).build(),
                true,
            )
            .await;
        if outcome == InvocationOutcome::Ok {
            assert!(result.is_ok());
        } else if body.contains("baml.sys.exit") {
            assert!(matches!(result, Err(EngineError::Exit { code: 7 })));
        } else {
            assert!(result.is_err());
        }
        assert_completed(&engine, &[(1, outcome), (2, child_outcome)]);
        assert_eq!(
            engine
                .futures
                .active_future_count(&engine.heap_permit_manager)
                .await,
            0
        );
    }
}

#[tokio::test]
async fn queued_and_running_child_cancellation_finish_once() {
    for (running_child, source) in [
        (
            false,
            r#"function Main() -> int {
            let g = baml.spawn.TaskGroup.new(1);
            let active = spawn with baml.spawn.options(group = g) { 7 };
            let queued = spawn with baml.spawn.options(group = g) { 99 };
            g.cancel(active = false);
            let a = await active;
            let b = (await queued) catch (e) { baml.panics.Cancelled => 0 };
            a + b
        }"#,
        ),
        (
            true,
            r#"function Main() -> int {
            let active = spawn { 7 };
            let cancelled = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                99
            };
            let a = await active;
            let b = (await cancelled) catch (e) { baml.panics.Cancelled => 0 };
            a + b
        }"#,
        ),
    ] {
        let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut ops = sys_native::SysOps::native();
        ops.baml_sys_sleep = Arc::new(move |_, _, _, ctx, _| {
            let started_tx = started_tx.clone();
            let cancel = ctx.cancel.clone();
            SysOpResult::Async(Box::pin(async move {
                // Report only after this child's sys-op future is polled.
                // The test cancels this child, leaving its parent uncancelled.
                started_tx.send(cancel).unwrap();
                std::future::pending().await
            }))
        });
        let engine = Arc::new(
            BexEngine::new(
                baml_db::testing::compile_source(source),
                Arc::new(ops),
                vec![],
            )
            .unwrap(),
        );
        let running = engine.call_function(
            "Main",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        );
        tokio::pin!(running);
        if running_child {
            tokio::select! {
                token = tokio::time::timeout(std::time::Duration::from_secs(10), started_rx.recv()) => {
                    token.expect("child must reach its sys-op").unwrap().cancel();
                }
                result = &mut running => panic!("call finished before child cancellation: {result:?}"),
            }
        }
        let result = running.await.unwrap();
        assert_eq!(result, BexExternalValue::Int(7));
        assert_completed(
            &engine,
            &[
                (1, InvocationOutcome::Ok),
                (2, InvocationOutcome::Ok),
                (3, InvocationOutcome::Cancelled),
            ],
        );
        assert_eq!(
            engine
                .futures
                .active_future_count(&engine.heap_permit_manager)
                .await,
            0
        );
    }
}

#[tokio::test]
async fn child_internal_error_settles_and_finishes_both_threads() {
    let engine =
        engine("function Main(f: () -> int) -> int { let child = spawn { f() }; await child }");
    let cancel = CancellationToken::new();
    // Bypass host argument validation to exercise a VM-internal callee error
    // inside a dispatched child and propagation through its parent's await.
    let thread = entry(&engine, &cancel, &[Value::int(7)]).await;
    let result = engine
        .run_thread_event_loop(
            RuntimeTy::int(),
            None,
            thread,
            CallId::next(),
            None,
            &cancel,
            true,
        )
        .await;
    assert!(result.is_err());
    assert_completed(
        &engine,
        &[
            (1, InvocationOutcome::Errored),
            (2, InvocationOutcome::Errored),
        ],
    );
}

#[tokio::test]
async fn clock_restore_invalidates_inflight_timing_without_changing_execution() {
    use btel_clock::{ClockMode, TimingStatus};

    let engine = Arc::new(
        BexEngine::new_with_telemetry_clock(
            baml_db::testing::compile_source("function Main() -> int { 7 }"),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            ClockMode::Monotonic,
        )
        .unwrap(),
    );
    let cancel = CancellationToken::new();
    let thread = entry(&engine, &cancel, &[]).await;
    let old = Arc::clone(thread.vm.telemetry_clock());
    let id = thread.vm.thread_id;
    let replacement = engine.reset_telemetry_clock_after_restore();
    assert_eq!(old.status(), TimingStatus::Restored);
    assert_ne!(replacement.metadata().epoch, old.metadata().epoch);
    let result = engine
        .run_thread_event_loop(
            RuntimeTy::int(),
            None,
            thread,
            CallId::next(),
            None,
            &cancel,
            true,
        )
        .await;
    assert!(matches!(
        result,
        Ok(ThreadOutcome::RootValue(BexExternalValue::Int(7)))
    ));
    assert_completed(&engine, &[(id, InvocationOutcome::Ok)]);
    let mut next = entry(&engine, &cancel, &[]).await;
    assert_eq!(next.vm.telemetry_clock().status(), TimingStatus::Valid);
    assert_ne!(next.vm.telemetry_clock().domain(), old.domain());
    engine.finish_thread_telemetry(&mut next, InvocationOutcome::Cancelled);
}
