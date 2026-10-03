//! Staging the runtime only stores the program. The first use compiles it,
//! builds the engine, and configures telemetry from the process environment,
//! so a host that stages the runtime as its generated SDK loads can still set
//! that environment before its first BAML call.

use std::{collections::HashMap, sync::Arc};

use bridge_cffi::BridgeError;

const TELEMETRY: &str = "BAML_TELEMETRY";

fn sources(source: &str) -> HashMap<String, String> {
    HashMap::from([("main.baml".to_string(), source.to_string())])
}

fn stage() {
    bridge_cffi::stage_runtime(".", sources("function one() -> int throws never { 1 }")).unwrap();
}

fn set_telemetry(value: &str) {
    // SAFETY: this file holds one test, which nextest runs in its own process,
    // and the engine only reads the environment inside `get_or_init_runtime`.
    unsafe { std::env::set_var(TELEMETRY, value) };
}

// One test: the runtime slot and the environment are both process-global.
#[tokio::test(flavor = "multi_thread")]
async fn engine_is_constructed_on_first_use() {
    assert!(matches!(
        bridge_cffi::get_or_init_runtime(),
        Err(BridgeError::NotInitialized)
    ));

    // Staging does not compile: a broken program fails its first use instead.
    bridge_cffi::stage_runtime(".", sources("function broken( -> int { 1 }")).unwrap();
    assert!(matches!(
        bridge_cffi::get_or_init_runtime(),
        Err(BridgeError::Startup(_))
    ));

    // The engine reads `BAML_TELEMETRY` as it is constructed. An invalid level
    // set after staging fails the first use, so nothing was constructed
    // before it. The failure is reported again on every later use.
    stage();
    set_telemetry("not-a-level");
    for _ in 0..2 {
        let Err(BridgeError::Startup(message)) = bridge_cffi::get_or_init_runtime() else {
            panic!("expected engine construction to fail on the invalid telemetry level");
        };
        assert!(message.contains(TELEMETRY), "{message}");
    }

    // A valid level set after staging is likewise the one used, and
    // concurrent first uses share one engine.
    stage();
    set_telemetry("off");
    let threads: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| bridge_cffi::get_or_init_runtime().unwrap()))
        .collect();
    let runtime = bridge_cffi::get_or_init_runtime().unwrap();
    for thread in threads {
        assert!(Arc::ptr_eq(&runtime, &thread.join().unwrap()));
    }
    drop(runtime);
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    assert!(matches!(
        bridge_cffi::get_or_init_runtime(),
        Err(BridgeError::NotInitialized)
    ));

    // A runtime that was never used has no engine to shut down.
    stage();
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    assert!(matches!(
        bridge_cffi::get_or_init_runtime(),
        Err(BridgeError::NotInitialized)
    ));
}
