//! BEP-034: spawn/await semantic invariants beyond the basic round-trip.
//!
//! Spawn/await lifecycle and cancellation invariants.

use std::time::{Duration, Instant};

use baml_tests::engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled};
use bex_engine::BexExternalValue;

/// BEP-034: "Parent throws still cascade-cancel children." When the
/// parent function throws an unhandled error, the parent thread's
/// cancel token fires; spawned children's tokens (which derive from
/// the parent's via `child_token()`) fire too, and the children's
/// next await checkpoint observes Cancelled.
///
/// We observe the cascade by spawning a child that does a long sleep,
/// then having the parent throw. The whole call should terminate in
/// well under the sleep duration — without the cascade the child would
/// hold the engine alive for 60s.
#[tokio::test]
async fn parent_throw_cancels_running_children() {
    // Compile OUTSIDE the timed region: compiling the (growing) BAML stdlib
    // takes seconds and is not what this test measures. Time only the engine
    // run, which is what the cascade makes prompt.
    let program = compile_source_with_opt(
        r#"
        function main() -> int throws baml.errors.Io {
            // Spawn a long-running child. The cascade should fire its
            // cancel token when main throws below.
            let _ = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
            throw baml.errors.Io { message: "parent boom" }
        }
        "#,
        OptLevel::One,
    );
    let started = Instant::now();
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let elapsed = started.elapsed();

    // Main throws → host receives unhandled Io.
    let err = output.result.expect_err("expected Io throw from parent");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");

    // Cascade test: the call must return promptly. Without
    // cascade-cancel the spawn body's 60s sleep would either keep the
    // engine alive for the full duration (if we waited for it) or
    // hang an awaiter. We don't await it here, but its tokio task is
    // alive in the background and would block clean shutdown if not
    // cancelled.
    assert!(
        elapsed < Duration::from_secs(5),
        "parent-throw call took {}ms (expected <5s — cascade missing?)",
        elapsed.as_millis(),
    );
}

/// B-405: an unobserved child error is not attached to the completing call.
#[tokio::test]
async fn never_awaited_spawn_error_does_not_replace_call_result() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { throw baml.errors.Io { message: "boom" } };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("unobserved spawn error must not replace the call result");
    assert_eq!(value, BexExternalValue::String("done".into()));
}

/// B-405: detached errors use global reporting too, not call attribution.
#[tokio::test]
async fn never_awaited_detached_spawn_error_does_not_replace_call_result() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                throw baml.errors.Io { message: "boom" }
            };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("detached spawn error must not replace the call result");
    assert_eq!(value, BexExternalValue::String("done".into()));
}

/// B-405: a racing child does not delay or replace the call result.
#[tokio::test]
async fn racing_never_awaited_spawn_error_does_not_replace_call_result() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { throw baml.errors.Io { message: "boom" } };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("racing spawn error must not replace the call result");
    assert_eq!(value, BexExternalValue::String("done".into()));
}

/// B-650 SDK-hang regression: a `detach = true` spawn that NEVER settles (an
/// infinite sleep, standing in for the SDK's detached `server.serve(...)`) must
/// NOT block the root's completion. A detached spawn is decoupled from its
/// spawner and outlives the run, so root completion does not join it. Before
/// the fix the wait treated detached spawns like any other outstanding future
/// and froze the SDK's `replay_serve_detached` server, which
/// returns immediately and is torn down only by a later, separate bridge call).
/// Wall-clock timeout-guarded because the pre-fix failure mode is a hang.
#[tokio::test]
async fn detached_infinite_spawn_does_not_block_root_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(600000n));
                "never"
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        run_compiled(program, "main", IndexMap::new(), false),
    )
    .await
    .expect("detached infinite spawn must not block root completion (B-650 sdk hang)");
    let value = output
        .result
        .expect("root should return cleanly without waiting on the detached spawn");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}

/// A racing never-awaited child that succeeds does not delay the call.
#[tokio::test]
async fn racing_never_awaited_successful_spawn_returns_cleanly() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { 42 };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("racing successful never-awaited spawn must return cleanly");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}

/// B-405: function completion does not join a delayed unobserved child.
#[tokio::test]
async fn never_awaited_delayed_throw_does_not_replace_call_result() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
                throw baml.errors.Io { message: "boom" }
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("delayed spawn error must not replace the call result");
    assert_eq!(value, BexExternalValue::String("done".into()));
}

/// A delayed detached child does not delay the call either.
#[tokio::test]
async fn detached_delayed_throw_is_not_waited_and_root_returns_cleanly() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
                throw baml.errors.Io { message: "boom" }
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        run_compiled(program, "main", IndexMap::new(), false),
    )
    .await
    .expect("detached delayed throw must not block the root (B-650 detach exemption)");
    let value = output
        .result
        .expect("root should return cleanly; the detached delayed throw is not waited-for");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}

/// A finite detached child does not delay the call.
#[tokio::test]
async fn finite_detached_spawn_does_not_block_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(150n));
                1
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("finite detached spawn must not block the root; it returns cleanly");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}

/// A never-awaited successful child does not affect the call result.
#[tokio::test]
async fn never_awaited_successful_spawn_returns_cleanly() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { 42 };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("successful never-awaited spawn must return cleanly");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}

/// Nested host callables must not join unrelated spawned work.
#[tokio::test]
async fn serve_then_fetch_then_cancel_does_not_hang() {
    // Compile OUTSIDE the timed region (stdlib compile is seconds and is not
    // what this test measures).
    let program = compile_source_with_opt(
        r#"
        function main() -> int {
            let server = baml.http.Server.bind("127.0.0.1:0");
            let task = spawn {
                server.serve((req: baml.http.Request) -> baml.http.Response {
                    baml.http.Response.new(503, { }, "down".to_utf8())
                })
            };
            let resp = baml.http.fetch("http://" + server.addr + "/");
            task.cancel();
            resp.status_code
        }
        "#,
        OptLevel::One,
    );
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        run_compiled(program, "main", IndexMap::new(), false),
    )
    .await
    .expect("serve+fetch+cancel must not hang (B-650 root misclassification)");
    let value = output
        .result
        .expect("serve+fetch+cancel run should succeed");
    let BexExternalValue::Int(status) = value else {
        panic!("expected Int status code, got {value:?}");
    };
    assert_eq!(status, 503);
}
