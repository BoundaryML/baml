//! The blocking `runtime::shutdown` called straight from async code. A test
//! binary of its own: shutdown ends the process-global runtime that every
//! test in a binary shares.

use std::{
    collections::HashMap,
    convert::Infallible,
    time::{Duration, Instant},
};

mod common;

use baml_bridge::{baml_value::internal::__BamlValuePrivate, encode, runtime};

const BAML_SRC: &str = r#"
function slow(started: string) -> int {
  baml.fs.write(started, "");
  baml.sys.sleep(baml.time.Duration.from_milliseconds(500));
  7
}
"#;

// One executor thread, which `shutdown()` blocks: had finishing the call in
// flight needed this runtime, or had shutdown entered a tokio runtime on this
// thread, the test would deadlock or panic.
#[tokio::test(flavor = "current_thread")]
async fn shutdown_inside_a_tokio_runtime_waits_for_the_call_in_flight() {
    common::locate_dev_engine();
    let files = HashMap::from([("main.baml".to_string(), BAML_SRC.to_string())]);
    // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    runtime::initialize_from_files(root.trim_end_matches('/'), &files)
        .expect("runtime initializes");

    let started = std::env::temp_dir().join(format!(
        "baml-bridge-shutdown-in-tokio-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&started);
    let started_arg = __BamlValuePrivate::to_baml(&started.to_string_lossy().into_owned());
    let call = tokio::spawn(runtime::invoke::<i64, Infallible>(
        "user.slow",
        encode::kwargs(vec![("started", Some(started_arg))]),
        vec![],
    ));
    // The task's first poll sends the call to the engine, which runs it on
    // its own threads.
    tokio::task::yield_now().await;
    // Shut down once the engine is running the call: shutdown rejects a call
    // the engine has not admitted yet.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !started.exists() {
        assert!(Instant::now() < deadline, "the call never started");
        std::thread::sleep(Duration::from_millis(10));
    }

    runtime::shutdown().expect("shutdown inside a tokio runtime");
    let answer = call.await.expect("the call's task completes");
    assert_eq!(answer.expect("the call in flight finishes"), 7);
    std::fs::remove_file(&started).expect("clean up the marker");
}
