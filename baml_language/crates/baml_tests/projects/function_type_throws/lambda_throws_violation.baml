// === Lambda throws contract violations (should produce errors) ===

// Lambda with explicit throws never but body throws - should error
function test_throws_never_but_throws() -> int {
  let f = () -> int throws never { throw "boom" }
  f()
}

// Lambda checked against an expected callback type should not under-declare throws.
function test_typed_lambda_throws_mismatch() -> int {
  let f: () -> int throws string = () -> int { throw 42 }
  f()
}

// Explicit lambda throws annotations must still cover the body's escaping throws.
function test_typed_lambda_annotation_underdeclares_body() -> int {
  let f: () -> int throws string = () -> int throws string { throw 42 }
  f()
}
