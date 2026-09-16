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
            InvocationOutcome::Exited,
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
        if outcome == InvocationOutcome::Ok {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err(), "expected failure for {source}");
        }
        if outcome == InvocationOutcome::Cancelled {
            assert!(is_cancelled_engine_error(result.as_ref().err().unwrap()));
        }
        if outcome == InvocationOutcome::Exited {
            assert!(matches!(result, Err(EngineError::Exit { code: 7 })));
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
            InvocationOutcome::Exited,
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
    for source in [
        r#"function Main() -> int {
            let g = baml.spawn.TaskGroup.new(1);
            let active = spawn with baml.spawn.options(group = g) { 7 };
            let queued = spawn with baml.spawn.options(group = g) { 99 };
            g.cancel(active = false);
            let a = await active;
            let b = (await queued) catch (e) { baml.panics.Cancelled => 0 };
            a + b
        }"#,
        r#"function Main() -> int {
            let active = spawn { 7 };
            let cancelled = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                99
            };
            cancelled.cancel();
            let a = await active;
            let b = (await cancelled) catch (e) { baml.panics.Cancelled => 0 };
            a + b
        }"#,
    ] {
        let engine = engine(source);
        let result = engine
            .call_function(
                "Main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next()).build(),
                true,
            )
            .await
            .unwrap();
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
async fn child_setup_failure_settles_and_finishes_both_threads() {
    let engine =
        engine("function Main(f: () -> int) -> int { let child = spawn { f() }; await child }");
    let cancel = CancellationToken::new();
    // Bypass host argument validation to exercise a VM-internal setup error
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
