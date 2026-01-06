// Test case: unknown type in return position should report error
// and suppress downstream type mismatch errors
function Foo() -> blah {
  1
}

// Test case: unknown type in parameter position
function Bar(x: unknown_param_type) -> int {
  1
}

// Test case: unknown type in optional wrapper
function Baz() -> unknown_optional? {
  null
}
