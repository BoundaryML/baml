// Test: field access on captured variables inside lambdas
// Exercises path_member_resolutions save/restore across lambda boundaries (Finding 1)
// and captured-variable path resolution in nested lambdas (Finding 3).

class Inner {
  value int
}

class Outer {
  inner Inner
  name string
}

// Case 1: Lambda captures a class variable and accesses its field.
// Tests that `obj.name` resolves correctly inside a lambda where `obj` is captured.
function test_captured_field_access(obj: Outer) -> string {
  let get_name = () -> string { obj.name }
  get_name()
}

// Case 2: Lambda captures a class variable and accesses a nested field chain.
// Tests that `obj.inner.value` (multi-segment path on captured var) resolves correctly.
function test_captured_nested_field(obj: Outer) -> int {
  let get_value = () -> int { obj.inner.value }
  get_value()
}

// Case 3: Field access AFTER a lambda that also does field access.
// Tests that path_member_resolutions from the lambda don't leak into the parent scope.
// If they leaked, `result.name` might get wrong resolutions from the lambda's `obj.inner.value`.
function test_no_leak_after_lambda(obj: Outer) -> string {
  let get_value = () -> int { obj.inner.value }
  let v = get_value()
  // This field access is in the parent scope — must have its own clean resolutions
  let result = obj
  result.name
}

// Case 4: Nested lambdas — inner lambda captures from grandparent through intermediate.
// Tests that `captured_obj.name` resolves to Local even though capture analysis
// for the intermediate lambda hasn't run when inner lambda's path resolution executes.
function test_nested_lambda_capture(obj: Outer) -> string {
  let outer_fn = () -> {
    // obj is captured from grandparent (test_nested_lambda_capture)
    let inner_fn = () -> string {
      // obj is captured from intermediate lambda which itself captures from grandparent
      obj.name
    }
    inner_fn()
  }
  outer_fn()
}

// Case 5: Two lambdas in sequence, each accessing different fields.
// Tests that path_member_resolutions from the first lambda don't affect the second.
function test_sequential_lambdas(obj: Outer) -> int {
  let get_name = () -> string { obj.name }
  let get_value = () -> int { obj.inner.value }
  let n = get_name()
  get_value()
}

// Case 6: Lambda with its own parameter that shadows nothing, plus captured field access.
// The lambda parameter `x` is local; `obj` is captured. Both used in the body.
function test_lambda_param_and_capture(obj: Outer) -> int {
  let add_to_value = (x: int) -> int { x + obj.inner.value }
  add_to_value(10)
}
