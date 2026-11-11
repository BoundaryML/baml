function f() -> int {
  // set up cleanup
  defer { cleanup() }
  42
}
