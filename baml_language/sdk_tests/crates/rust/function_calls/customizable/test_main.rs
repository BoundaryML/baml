//! Smoke tests for plain (non-LLM) expression functions.

use baml_sdk::{hello_world_sync, single_required_arg_sync};

#[test]
fn test_main_hello_world_returns_literal() {
    assert_eq!(hello_world_sync().unwrap(), "hello world");
}

#[test]
fn test_main_single_required_arg_round_trips() {
    // The next step up from the nullary case: one required positional
    // argument round-trips through the engine unchanged.
    assert_eq!(single_required_arg_sync("hi".to_string()).unwrap(), "hi");
}
