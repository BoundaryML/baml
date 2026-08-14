//! Fire-and-forget error observation and reporting.

mod common;

use std::sync::{Arc, Mutex};

use bex_engine::{
    BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder, is_cancelled_engine_error,
};
use bex_heap::CollectionLevel;
use common::compile_for_engine;
use sys_native::SysOpsExt;

async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let engine = make_engine(source);
    call_main(&engine, true).await
}

fn make_engine(source: &str) -> Arc<BexEngine> {
    let snapshot = compile_for_engine(source);
    Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    )
}

async fn call_main(
    engine: &Arc<BexEngine>,
    copy_objects: bool,
) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            copy_objects,
        )
        .await
}

async fn wait_for_spawn_completion(engine: &Arc<BexEngine>) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while engine.active_future_count().await != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("spawn did not settle");
}

/// The canonical bug-2 repro: `g` awaits `f` and HANDLES its error, so
/// `main` must observe `g`'s result — not `f`'s error resurfacing
/// fire-and-forget at `main`'s `await g`.
#[tokio::test]
async fn handled_child_error_does_not_resurface_at_spawner() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let f = spawn { bad() };
            let g = spawn { (await f) catch (e) { let e => 7 } };
            (await g) catch (e) { let e => -1 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// Re-throw variant: `g` awaits `f` and re-throws. `main`'s `catch` must
/// observe the error exactly once, through `g` — not pre-empted by `f`'s
/// fire-and-forget entry bypassing the `catch`.
#[tokio::test]
async fn rethrown_child_error_arrives_via_consumer() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let f = spawn { bad() };
            let g = spawn { (await f) catch (e) { let e => throw e } };
            (await g) catch (e) { let e => 9 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(9));
}

/// Slow variant: `f` errors while `g` is already parked awaiting it, so the
/// deferred error's wake signal (rather than an already-settled read) drives
/// the observation.
#[tokio::test]
async fn handled_slow_child_error_does_not_resurface() {
    let source = r#"
        function bad_slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            throw "boom"
        }
        function main() -> int {
            let f = spawn { bad_slow() };
            let g = spawn { (await f) catch (e) { let e => 7 } };
            (await g) catch (e) { let e => -1 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// A genuinely fire-and-forget error surfaces when GC proves its future is
/// unreachable, not at an unrelated await.
#[tokio::test]
async fn unobserved_child_error_surfaces_after_major_gc() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function other() -> int { baml.sys.sleep(baml.time.Duration.from_milliseconds(50n)); 1 }
        function main() -> int {
            spawn { bad() };
            let g = spawn { other() };
            await g
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;
    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
    assert!(!errors[0].cancelled);
}

#[tokio::test]
async fn unobserved_child_error_surfaces_after_minor_gc() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            spawn { bad() };
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    wait_for_spawn_completion(&engine).await;
    engine.collect_garbage(CollectionLevel::Minor).await;
    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
    assert!(!errors[0].cancelled);

    engine.collect_garbage(CollectionLevel::Major).await;
    assert!(engine.take_unhandled_spawn_errors().is_empty());
}

#[tokio::test]
async fn observed_child_error_does_not_surface_after_gc() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let failing = spawn { bad() };
            (await failing) catch (e) { let e => 99 }
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(99)
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    assert!(engine.take_unhandled_spawn_errors().is_empty());
}

#[tokio::test]
async fn host_handle_keeps_errored_future_observable() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> baml.future.Future<int, string> {
            spawn { bad() }
        }
    "#;
    let engine = make_engine(source);
    let handle = call_main(&engine, false).await.unwrap();
    assert!(matches!(handle, BexExternalValue::Handle(_)));
    wait_for_spawn_completion(&engine).await;

    engine.collect_garbage(CollectionLevel::Major).await;
    assert!(engine.take_unhandled_spawn_errors().is_empty());

    drop(handle);
    engine.collect_garbage(CollectionLevel::Major).await;
    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
}

#[tokio::test]
async fn error_then_cancel_reports_nonfatal_cancellation_flag() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let failing = spawn { bad() };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            let _ = failing.cancel();
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;
    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
    assert!(errors[0].cancelled);
}

#[tokio::test]
async fn handler_receives_unhandled_spawn_error() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            spawn { bad() };
            1
        }
    "#;
    let engine = make_engine(source);
    let handled = Arc::new(Mutex::new(Vec::new()));
    let handled_for_callback = Arc::clone(&handled);
    engine.set_unhandled_spawn_error_handler(Some(Arc::new(move |error| {
        handled_for_callback.lock().unwrap().push(error);
    })));

    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;

    let handled = handled.lock().unwrap();
    assert_eq!(handled.len(), 1);
    assert_eq!(handled[0].value, BexExternalValue::String("boom".into()));
    assert!(!handled[0].trace.is_empty());
    assert!(!handled[0].cancelled);
    assert!(engine.take_unhandled_spawn_errors().is_empty());
}

#[tokio::test]
async fn installing_handler_drains_already_queued_errors() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            spawn { bad() };
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;

    let handled = Arc::new(Mutex::new(Vec::new()));
    let handled_for_callback = Arc::clone(&handled);
    engine.set_unhandled_spawn_error_handler(Some(Arc::new(move |error| {
        handled_for_callback.lock().unwrap().push(error);
    })));

    let handled = handled.lock().unwrap();
    assert_eq!(handled.len(), 1);
    assert_eq!(handled[0].value, BexExternalValue::String("boom".into()));
    assert!(engine.take_unhandled_spawn_errors().is_empty());
}

#[tokio::test]
async fn panicking_handler_does_not_unwind_gc_and_preserves_the_current_report() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            spawn { bad() };
            1
        }
    "#;
    let engine = make_engine(source);
    engine.set_unhandled_spawn_error_handler(Some(Arc::new(|_| {
        panic!("handler failed");
    })));

    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;

    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
}

#[tokio::test]
async fn multiple_unhandled_spawn_errors_are_reported() {
    let source = r#"
        function fail(message: string) -> int throws string { throw message }
        function main() -> int {
            spawn { fail("one") };
            spawn { fail("two") };
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;

    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 2);
    assert_ne!(errors[0].report_id, errors[1].report_id);
    let mut values: Vec<_> = errors.into_iter().map(|error| error.value).collect();
    values.sort_by_key(|value| format!("{value:?}"));
    assert_eq!(
        values,
        vec![
            BexExternalValue::String("one".into()),
            BexExternalValue::String("two".into())
        ]
    );
}

#[tokio::test]
async fn shutdown_waits_for_active_calls_and_rejects_new_calls() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
            spawn { bad() };
            1
        }
    "#;
    let engine = make_engine(source);
    let call_engine = Arc::clone(&engine);
    let call = tokio::spawn(async move { call_main(&call_engine, true).await });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    let shutdown_engine = Arc::clone(&engine);
    let shutdown = tokio::spawn(async move { shutdown_engine.shutdown().await });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    assert_eq!(
        call_main(&engine, true).await,
        Err(EngineError::ShuttingDown)
    );
    assert_eq!(call.await.unwrap().unwrap(), BexExternalValue::Int(1));
    shutdown.await.unwrap();

    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
}

#[tokio::test]
async fn active_call_can_be_cancelled_while_shutdown_waits() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(500n));
            1
        }
    "#;
    let engine = make_engine(source);
    let call_id = sys_types::CallId::next();
    let call_engine = Arc::clone(&engine);
    let call = tokio::spawn(async move {
        call_engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(call_id).build(),
                true,
            )
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    let shutdown_engine = Arc::clone(&engine);
    let shutdown = tokio::spawn(async move { shutdown_engine.shutdown().await });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    engine.cancel_function_call(call_id).unwrap();
    let error = call.await.unwrap().unwrap_err();
    assert!(is_cancelled_engine_error(&error));
    shutdown.await.unwrap();
}

#[tokio::test]
async fn cancelled_shutdown_restores_the_running_state() {
    let source = r#"
        function main() -> int {
            spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
                42
            };
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );

    let shutdown_engine = Arc::clone(&engine);
    let shutdown = tokio::spawn(async move { shutdown_engine.shutdown().await });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());

    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn unhandled_grandchild_error_is_not_attached_to_parent_spawn() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function worker() -> int {
            spawn { bad() };
            42
        }
        function main() -> int {
            await spawn { worker() }
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(42)
    );
    engine.shutdown().await;

    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
}

#[tokio::test]
async fn detached_spawn_error_uses_the_same_gc_reporting_path() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            spawn with baml.spawn.options(detach = true) { bad() };
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;

    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
}

#[tokio::test]
async fn cancelled_spawn_does_not_report_an_unhandled_error() {
    let source = r#"
        function main() -> int {
            let pending = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
                42
            };
            let _ = pending.cancel();
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    engine.shutdown().await;
    assert!(engine.take_unhandled_spawn_errors().is_empty());
}

#[tokio::test]
async fn call_completion_does_not_join_but_shutdown_does() {
    let source = r#"
        function main() -> int {
            spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
                throw "boom"
            };
            1
        }
    "#;
    let engine = make_engine(source);
    assert_eq!(
        call_main(&engine, true).await.unwrap(),
        BexExternalValue::Int(1)
    );
    assert_eq!(engine.active_future_count().await, 1);

    engine.shutdown().await;
    let errors = engine.take_unhandled_spawn_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].value, BexExternalValue::String("boom".into()));
}

/// B-405: awaiting an unrelated future must not surface an error from a
/// reachable future that this task observes later.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn caught_error_is_not_preempted_by_unrelated_await() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            7
        }
        function main() -> int {
            let failing = spawn { bad() };
            let unrelated = spawn { slow() };
            let _ = await unrelated;
            (await failing) catch (e) { let e => 99 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// B-405: returning a future across a function boundary must not make its
/// error fire-and-forget while the caller still holds and later awaits it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn returned_future_error_is_not_preempted_by_unrelated_await() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function start() -> baml.future.Future<int, string> {
            spawn { bad() }
        }
        function slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            7
        }
        function main() -> int {
            let failing = start();
            let unrelated = spawn { slow() };
            let _ = await unrelated;
            (await failing) catch (e) { let e => 99 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// B-405: a future reachable through a container remains observable and must
/// not be treated as fire-and-forget at an unrelated await.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn contained_future_error_is_not_preempted_by_unrelated_await() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            7
        }
        function main() -> int {
            let futures = [spawn { bad() }];
            let unrelated = spawn { slow() };
            let _ = await unrelated;
            (await futures[0]) catch (e) { let e => 99 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// B-405: delivering an error from `await` counts as observation even when it
/// is rethrown and handled by the caller.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rethrown_error_is_not_preempted_by_unrelated_await() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            7
        }
        function consume() -> int throws string {
            let failing = spawn { bad() };
            let unrelated = spawn { slow() };
            let _ = (await unrelated) catch (e) { baml.errors.Io => 0 };
            await failing
        }
        function main() -> int {
            consume() catch (e) { let e: string => 99 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// B-405: a combinator is an observer of its input futures. An unrelated
/// earlier await must not steal an input error before the combinator runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn combinator_observation_is_not_preempted_by_unrelated_await() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function good() -> int throws string { 42 }
        function slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            7
        }
        function main() -> int {
            let futures = [spawn { bad() }, spawn { good() }];
            let unrelated = spawn { slow() };
            let _ = await unrelated;
            await baml.future.any(futures) catch (e) { let e => -1 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(42));
}
