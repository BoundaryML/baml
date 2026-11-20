//! Consumer smoke for the packaged `baml_bridge` crate + prebuilt engine
//! cdylib, driven through a `baml-cli`-generated `baml_sdk`. Exercises both
//! the sync and async entry points; a successful call proves the engine
//! loaded and the version handshake passed (the loader rejects a version
//! mismatch before any call). The verify workflow asserts the version triple
//! (crate == dylib `version()` == plan canonical) separately.
//!
//! Cancellation and streaming are intentionally absent — those SDK features
//! are not implemented yet; the verifier grows to cover them as they land.

/// A consumer's own type: `arbitrary_precision` breaks numbers in untagged
/// enums, so this only parses if linking the SDK left `serde_json`'s
/// features alone.
#[derive(serde::Deserialize, Debug, PartialEq)]
#[serde(untagged)]
enum NumberOrText {
    Number(f64),
    Text(String),
}

fn main() {
    // Linking the SDK must not turn on serde_json features for the whole
    // build (#4377).
    let parsed: NumberOrText = serde_json::from_str("1.5").expect("untagged f64 parses");
    assert_eq!(parsed, NumberOrText::Number(1.5));
    let parsed: NumberOrText = serde_json::from_str(r#""x""#).expect("untagged text parses");
    assert_eq!(parsed, NumberOrText::Text("x".to_string()));
    let object: serde_json::Value = serde_json::from_str(r#"{"b":1,"a":2}"#).unwrap();
    assert_eq!(
        object.to_string(),
        r#"{"a":2,"b":1}"#,
        "preserve_order is off"
    );

    // The bridge types in the SDK's signatures are named through the SDK's
    // own re-export: the consumer pins no second `baml_bridge` (#4374).
    let sync: Result<i64, baml_sdk::baml_bridge::Error> = baml_sdk::rt_int_sync(7);
    let sync = sync.expect("sync rt_int call");
    assert_eq!(sync, 7, "sync roundtrip");
    let defaulted = baml_sdk::rt_default_sync(baml_sdk::baml_bridge::OptionalArg::Unset)
        .expect("rt_default call");
    assert_eq!(defaulted, 3, "an unset argument takes its BAML default");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build Tokio runtime");
    let asyncd = runtime
        .block_on(baml_sdk::rt_int(9))
        .expect("async rt_int call");
    assert_eq!(asyncd, 9, "async roundtrip");

    let greet = baml_sdk::rt_greet_sync("kai".to_string()).expect("rt_greet call");
    assert_eq!(greet, "hi", "string roundtrip");

    println!("OK: consumer smoke passed (sync={sync}, async={asyncd}, greet={greet:?})");
}
