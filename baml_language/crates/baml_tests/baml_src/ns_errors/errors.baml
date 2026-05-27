// Tests for error handling and stack traces.
//
// Converted from crates/baml_tests/tests/errors.rs.
//
// SKIPPED:
//   error_stack_trace — asserts on the *text* of the error message
//     (insta::assert_snapshot!(err, ...)), which includes exact file/line
//     numbers in a traceback. BAML has no way to inspect or match against
//     the error message string from a `catch` arm, so this test cannot be
//     expressed as a BAML test block.
