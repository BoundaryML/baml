//! CFFI tests
//!
//! Tests for CFFI functionality (encoding/decoding via the runtime)

use rust::baml_client::sync_client::B;

/// Test CFFI encoding and decoding via function calls
#[test]
fn test_cffi_function_call() {
    // Test that we can make a function call (which exercises CFFI)
    let result = B.FnOutputClass.call("create test output");
    assert!(
        result.is_ok(),
        "Expected successful CFFI call, got {:?}",
        result
    );

    let output = result.unwrap();
    assert!(!output.prop1.is_empty(), "Expected non-empty prop1");
}

/// Test CFFI with complex types
#[test]
fn test_cffi_complex_types() {
    // Test with nested class output
    let result = B.FnOutputClassNested.call("create nested output");
    assert!(
        result.is_ok(),
        "Expected successful CFFI call with complex types, got {:?}",
        result
    );

    let output = result.unwrap();
    assert!(!output.prop1.is_empty(), "Expected non-empty prop1");
}

/// Test CFFI with list output
#[test]
fn test_cffi_list_output() {
    let result = B.FnOutputClassList.call("create list output");
    assert!(
        result.is_ok(),
        "Expected successful CFFI call with list, got {:?}",
        result
    );

    let output = result.unwrap();
    assert!(!output.is_empty(), "Expected non-empty list");
}

/// Test CFFI error handling
#[test]
fn test_cffi_error_handling() {
    // Test with a function that should fail
    let result = B.FnAlwaysFails.call("trigger error");
    assert!(result.is_err(), "Expected CFFI to propagate error");
}
