// Test: generic type substitution preserved through chained field access.
// Issue #3: class_field_ty() should apply concrete type arguments, not return
// the raw unsubstituted type from the generic class definition.

class Box<T> {
  value T
}

class User {
  name string
  age int
}

// box.value should be typed as User (not Unknown/type-var),
// so box.value.name should be typed as string.
function test_generic_chain(box: Box<User>) -> string {
  box.value.name
}

// Nested generics: Box<Box<User>>.value.value.name
function test_nested_generic_chain(box: Box<Box<User>>) -> string {
  box.value.value.name
}

// Generic in lambda capture: lambda captures a Box<User> and accesses value.name
function test_generic_capture(box: Box<User>) -> string {
  let get_name = () -> string { box.value.name }
  get_name()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// generic_field_chain.baml:1:1 (comment) len=74 "// Test: generic type substitution preserved through chained field access."
// generic_field_chain.baml:2:1 (comment) len=78 "// Issue #3: class_field_ty() should apply concrete type arguments, not return"
// generic_field_chain.baml:3:1 (comment) len=64 "// the raw unsubstituted type from the generic class definition."
// generic_field_chain.baml:5:1 (keyword) len=5 "class"
// generic_field_chain.baml:5:7 (class) [declaration] len=3 "Box"
// generic_field_chain.baml:5:10 (operator) len=1 "<"
// generic_field_chain.baml:5:11 (typeParameter) [declaration] len=1 "T"
// generic_field_chain.baml:5:12 (operator) len=1 ">"
// generic_field_chain.baml:6:3 (property) [declaration] len=5 "value"
// generic_field_chain.baml:6:9 (type) len=1 "T"
// generic_field_chain.baml:9:1 (keyword) len=5 "class"
// generic_field_chain.baml:9:7 (class) [declaration] len=4 "User"
// generic_field_chain.baml:10:3 (property) [declaration] len=4 "name"
// generic_field_chain.baml:10:8 (type) [defaultLibrary] len=6 "string"
// generic_field_chain.baml:11:3 (property) [declaration] len=3 "age"
// generic_field_chain.baml:11:7 (type) [defaultLibrary] len=3 "int"
// generic_field_chain.baml:14:1 (comment) len=60 "// box.value should be typed as User (not Unknown/type-var),"
// generic_field_chain.baml:15:1 (comment) len=47 "// so box.value.name should be typed as string."
// generic_field_chain.baml:16:1 (keyword) len=8 "function"
// generic_field_chain.baml:16:10 (function) [declaration] len=18 "test_generic_chain"
// generic_field_chain.baml:16:29 (parameter) [declaration] len=3 "box"
// generic_field_chain.baml:16:34 (class) len=3 "Box"
// generic_field_chain.baml:16:37 (operator) len=1 "<"
// generic_field_chain.baml:16:38 (class) len=4 "User"
// generic_field_chain.baml:16:42 (operator) len=1 ">"
// generic_field_chain.baml:16:48 (type) [defaultLibrary] len=6 "string"
// generic_field_chain.baml:17:3 (parameter) len=3 "box"
// generic_field_chain.baml:17:7 (property) len=5 "value"
// generic_field_chain.baml:17:13 (property) len=4 "name"
// generic_field_chain.baml:20:1 (comment) len=51 "// Nested generics: Box<Box<User>>.value.value.name"
// generic_field_chain.baml:21:1 (keyword) len=8 "function"
// generic_field_chain.baml:21:10 (function) [declaration] len=25 "test_nested_generic_chain"
// generic_field_chain.baml:21:36 (parameter) [declaration] len=3 "box"
// generic_field_chain.baml:21:41 (class) len=3 "Box"
// generic_field_chain.baml:21:44 (operator) len=1 "<"
// generic_field_chain.baml:21:45 (class) len=3 "Box"
// generic_field_chain.baml:21:48 (operator) len=1 "<"
// generic_field_chain.baml:21:49 (class) len=4 "User"
// generic_field_chain.baml:21:53 (operator) len=1 ">"
// generic_field_chain.baml:21:54 (operator) len=1 ">"
// generic_field_chain.baml:21:60 (type) [defaultLibrary] len=6 "string"
// generic_field_chain.baml:22:3 (parameter) len=3 "box"
// generic_field_chain.baml:22:7 (property) len=5 "value"
// generic_field_chain.baml:22:13 (property) len=5 "value"
// generic_field_chain.baml:22:19 (property) len=4 "name"
// generic_field_chain.baml:25:1 (comment) len=81 "// Generic in lambda capture: lambda captures a Box<User> and accesses value.name"
// generic_field_chain.baml:26:1 (keyword) len=8 "function"
// generic_field_chain.baml:26:10 (function) [declaration] len=20 "test_generic_capture"
// generic_field_chain.baml:26:31 (parameter) [declaration] len=3 "box"
// generic_field_chain.baml:26:36 (class) len=3 "Box"
// generic_field_chain.baml:26:39 (operator) len=1 "<"
// generic_field_chain.baml:26:40 (class) len=4 "User"
// generic_field_chain.baml:26:44 (operator) len=1 ">"
// generic_field_chain.baml:26:50 (type) [defaultLibrary] len=6 "string"
// generic_field_chain.baml:27:3 (keyword) len=3 "let"
// generic_field_chain.baml:27:7 (variable) [declaration] len=8 "get_name"
// generic_field_chain.baml:27:16 (operator) len=1 "="
// generic_field_chain.baml:27:24 (type) [defaultLibrary] len=6 "string"
// generic_field_chain.baml:27:33 (parameter) len=3 "box"
// generic_field_chain.baml:27:37 (property) len=5 "value"
// generic_field_chain.baml:27:43 (property) len=4 "name"
// generic_field_chain.baml:28:3 (variable) len=8 "get_name"
