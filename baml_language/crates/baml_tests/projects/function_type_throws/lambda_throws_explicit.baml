// === Lambda explicit throws annotations ===

// Lambda with explicit throws string - body matches
function test_explicit_throws_match() -> int {
  let f = () -> int throws string { throw "boom" }
  f()
}

// Lambda with explicit throws never - body is pure (should be fine)
function test_explicit_throws_never_pure() -> int {
  let f = () -> int throws never { 42 }
  f()
}

// Lambda with explicit throws string - body is pure (should be fine, never <: string)
function test_explicit_throws_wider_than_body() -> int {
  let f = () -> int throws string { 42 }
  f()
}
