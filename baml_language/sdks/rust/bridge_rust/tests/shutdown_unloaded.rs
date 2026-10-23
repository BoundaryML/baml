//! `runtime::shutdown` without a loaded engine. A test binary of its own:
//! whether the engine is loaded is process-wide state.

use baml_bridge::{loader, runtime};

#[test]
fn shutdown_without_a_loaded_engine_is_a_no_op() {
    // SAFETY: the only test in this binary; nothing else reads the environment.
    #[expect(unsafe_code)]
    unsafe {
        // Loading from this path fails, and never downloads.
        std::env::set_var("BAML_BRIDGE_PATH", "/nonexistent/bridge_cffi");
        std::env::set_var("BAML_BRIDGE_DISABLE_DOWNLOAD", "true");
    }
    // Never loaded: had shutdown tried to load the engine, it would fail.
    runtime::shutdown().expect("nothing to shut down");

    // Failed to load: there is no runtime to shut down either.
    assert!(loader::preload().is_err(), "the engine cannot load here");
    runtime::shutdown().expect("nothing to shut down after a failed load");
}
