// Test: function type with throws clause
// This verifies that () -> int throws string parses correctly

type ThrowingFn = () -> int throws string

function takes_throwing(f: () -> int throws string) -> int {
  f()
}
