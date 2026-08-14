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

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// deep_method_call.baml:1:1 (comment) len=68 "// Test: deep local-rooted method calls — throw target resolution."
// deep_method_call.baml:2:1 (comment) len=79 "// Issue #5: For `a.b.method()`, call_target_name reads segments[0]/segments[1]"
// deep_method_call.baml:3:1 (comment) len=78 "// (\"a\"/\"b\") instead of using the receiver type and last segment (\"B.method\")."
// deep_method_call.baml:5:1 (keyword) len=5 "class"
// deep_method_call.baml:5:7 (class) [declaration] len=1 "B"
// deep_method_call.baml:6:3 (property) [declaration] len=5 "label"
// deep_method_call.baml:6:9 (type) [defaultLibrary] len=6 "string"
// deep_method_call.baml:8:3 (keyword) len=8 "function"
// deep_method_call.baml:8:12 (method) [declaration] len=5 "greet"
// deep_method_call.baml:8:18 (parameter) [declaration] len=4 "self"
// deep_method_call.baml:8:27 (type) [defaultLibrary] len=6 "string"
// deep_method_call.baml:9:5 (parameter) len=4 "self"
// deep_method_call.baml:9:10 (property) len=5 "label"
// deep_method_call.baml:13:1 (keyword) len=5 "class"
// deep_method_call.baml:13:7 (class) [declaration] len=1 "A"
// deep_method_call.baml:14:3 (property) [declaration] len=1 "b"
// deep_method_call.baml:14:5 (class) len=1 "B"
// deep_method_call.baml:17:1 (comment) len=69 "// Two-segment method call: a.b is the receiver, greet is the method."
// deep_method_call.baml:18:1 (comment) len=56 "// call_target_name should produce \"B.greet\", not \"A.b\"."
// deep_method_call.baml:19:1 (keyword) len=8 "function"
// deep_method_call.baml:19:10 (function) [declaration] len=16 "test_deep_method"
// deep_method_call.baml:19:27 (parameter) [declaration] len=1 "a"
// deep_method_call.baml:19:30 (class) len=1 "A"
// deep_method_call.baml:19:36 (type) [defaultLibrary] len=6 "string"
// deep_method_call.baml:20:3 (parameter) len=1 "a"
// deep_method_call.baml:20:5 (property) len=1 "b"
// deep_method_call.baml:20:7 (method) len=5 "greet"
// deep_method_call.baml:23:1 (comment) len=71 "// Three-segment chain: a.b.label is a field chain (not a method call)."
// deep_method_call.baml:24:1 (comment) len=44 "// Should resolve correctly as field access."
// deep_method_call.baml:25:1 (keyword) len=8 "function"
// deep_method_call.baml:25:10 (function) [declaration] len=15 "test_deep_field"
// deep_method_call.baml:25:26 (parameter) [declaration] len=1 "a"
// deep_method_call.baml:25:29 (class) len=1 "A"
// deep_method_call.baml:25:35 (type) [defaultLibrary] len=6 "string"
// deep_method_call.baml:26:3 (parameter) len=1 "a"
// deep_method_call.baml:26:5 (property) len=1 "b"
// deep_method_call.baml:26:7 (property) len=5 "label"
// deep_method_call.baml:29:1 (comment) len=81 "// Method call on direct local — this is the 2-segment case that already works."
// deep_method_call.baml:30:1 (keyword) len=8 "function"
// deep_method_call.baml:30:10 (function) [declaration] len=18 "test_direct_method"
// deep_method_call.baml:30:29 (parameter) [declaration] len=1 "b"
// deep_method_call.baml:30:32 (class) len=1 "B"
// deep_method_call.baml:30:38 (type) [defaultLibrary] len=6 "string"
// deep_method_call.baml:31:3 (parameter) len=1 "b"
// deep_method_call.baml:31:5 (method) len=5 "greet"
