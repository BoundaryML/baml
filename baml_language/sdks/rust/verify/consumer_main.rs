//! Consumer smoke for the packaged `baml_bridge` crate + prebuilt engine
//! cdylib, driven through a `baml-cli`-generated `baml_sdk`. Exercises both
//! the sync and async entry points; a successful call proves the engine
//! loaded and the version handshake passed (the loader rejects a version
//! mismatch before any call). The verify workflow asserts the version triple
//! (crate == dylib `version()` == plan canonical) separately.
//!
//! Cancellation and streaming are intentionally absent — those SDK features
//! are not implemented yet; the verifier grows to cover them as they land.

fn main() {
    // Public bridge types must come from the SDK's exact runtime dependency;
    // consumers should not need a second, manually synchronized version pin.
    let unset: baml_sdk::baml_bridge::OptionalArg<String> =
        baml_sdk::baml_bridge::OptionalArg::Unset;
    assert!(matches!(unset, baml_sdk::baml_bridge::OptionalArg::Unset));

    let sync = baml_sdk::rt_int(7).expect("sync rt_int call");
    assert_eq!(sync, 7, "sync roundtrip");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build Tokio runtime");
    let asyncd = runtime
        .block_on(baml_sdk::rt_int_async(9))
        .expect("async rt_int call");
    assert_eq!(asyncd, 9, "async roundtrip");

    let greet = baml_sdk::rt_greet("kai".to_string()).expect("rt_greet call");
    assert_eq!(greet, "hi", "string roundtrip");

    println!("OK: consumer smoke passed (sync={sync}, async={asyncd}, greet={greet:?})");
}
