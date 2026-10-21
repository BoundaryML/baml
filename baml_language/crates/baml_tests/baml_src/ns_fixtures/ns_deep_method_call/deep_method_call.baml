// Test: deep local-rooted method calls — throw target resolution.
// Issue #5: For `a.b.method()`, call_target_name reads segments[0]/segments[1]
// ("a"/"b") instead of using the receiver type and last segment ("B.method").

class B {
  label string

  function greet(self) -> string {
    self.label
  }
}

class A {
  b B
}

// Two-segment method call: a.b is the receiver, greet is the method.
// call_target_name should produce "B.greet", not "A.b".
function test_deep_method(a: A) -> string {
  a.b.greet()
}

// Three-segment chain: a.b.label is a field chain (not a method call).
// Should resolve correctly as field access.
function test_deep_field(a: A) -> string {
  a.b.label
}

// Method call on direct local — this is the 2-segment case that already works.
function test_direct_method(b: B) -> string {
  b.greet()
}
