//! Unified tests for error handling and stack traces.

// TODO: The error_stack_trace test requires direct VM access for stack trace inspection.
// Once BexEngine exposes stack trace information, this test should be updated to use
// the baml_test! macro. For now, it remains ignored.

#[tokio::test]
#[ignore = "stack trace not yet working"]
async fn error_stack_trace() {
    // Original test used assert_vm_fails_with_inspection to inspect vm.stack_trace().
    // The baml_test! macro doesn't expose the VM directly, so this test
    // needs BexEngine to support stack trace retrieval first.
    //
    // BAML source:
    //   function three() -> int { return 3 / 0; }
    //   function two() -> int { return three(); }
    //   function one() -> int { return two(); }
    //   function main() -> int { let t = one(); t }
    //
    // Expected: DivisionByZero error with stack trace:
    //   [0] main (line 14)
    //   [1] one (line 10)
    //   [2] two (line 6)
    //   [3] three (line 2)
}
