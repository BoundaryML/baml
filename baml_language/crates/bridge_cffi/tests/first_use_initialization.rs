//! Initializing the runtime compiles the program but does not construct the
//! engine: that happens on the first use. Telemetry is configured from the
//! process environment during engine construction, so a host that initializes
//! the runtime as its generated SDK loads can still set that environment
//! before its first BAML call.

use std::{collections::HashMap, sync::Arc};

use bridge_cffi::BridgeError;

const TELEMETRY: &str = "BAML_TELEMETRY";

fn sources(source: &str) -> HashMap<String, String> {
    HashMap::from([("main.baml".to_string(), source.to_string())])
}

fn initialize() {
    bridge_cffi::initialize_runtime(".", sources("function one() -> int throws never { 1 }"))
        .unwrap();
}

fn set_telemetry(value: &str) {
    // SAFETY: this file holds one test, which nextest runs in its own process,
    // and the engine only reads the environment inside `get_runtime`.
    unsafe { std::env::set_var(TELEMETRY, value) };
}

// One test: the runtime slot and the environment are both process-global.
#[tokio::test(flavor = "multi_thread")]
async fn engine_is_constructed_on_first_use() {
    // A program that does not compile still fails initialization.
    assert!(
        bridge_cffi::initialize_runtime(".", sources("function broken( -> int { 1 }")).is_err()
    );
    assert!(matches!(
        bridge_cffi::get_runtime(),
        Err(BridgeError::NotInitialized)
    ));

    // The engine reads `BAML_TELEMETRY` as it is constructed. An invalid level
    // set after initialization fails the first use, so nothing was constructed
    // before it. The failure is reported again on every later use.
    initialize();
    set_telemetry("not-a-level");
    for _ in 0..2 {
        let Err(BridgeError::Startup(message)) = bridge_cffi::get_runtime() else {
            panic!("expected engine construction to fail on the invalid telemetry level");
        };
        assert!(message.contains(TELEMETRY), "{message}");
    }

    // A valid level set after initialization is likewise the one used, and
    // concurrent first uses share one engine.
    initialize();
    set_telemetry("off");
    let threads: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| bridge_cffi::get_runtime().unwrap()))
        .collect();
    let runtime = bridge_cffi::get_runtime().unwrap();
    for thread in threads {
        assert!(Arc::ptr_eq(&runtime, &thread.join().unwrap()));
    }
    drop(runtime);
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    assert!(matches!(
        bridge_cffi::get_runtime(),
        Err(BridgeError::NotInitialized)
    ));

    // A runtime that was never used has no engine to shut down.
    initialize();
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    assert!(matches!(
        bridge_cffi::get_runtime(),
        Err(BridgeError::NotInitialized)
    ));
}
