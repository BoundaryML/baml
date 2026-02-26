function greet(name string, age int) -> string {
  name
}

function foo() -> string {
  greet("Alice", 30)
}

function bar() -> string {
  let g = greet;

  g("Bob", 20)
}

function baz() -> string {
  // lose parameter names
  let g: (string, int) -> string = greet;

  g("Charlie", 25)
}

function qux() -> string {
  // partially lose  and rename parameter
  let g: (c: string, int) -> string = greet;

  g("Delta", 30)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- inlay_hints
// call_arg_names.baml:6:9 (Parameter) "name:"
// call_arg_names.baml:6:18 (Parameter) "age:"
// call_arg_names.baml:10:8 (Type) ": (name: string, age: int) -> string"
//   edit@10:8 ": (name: string, age: int) -> string"
// call_arg_names.baml:12:5 (Parameter) "name:"
// call_arg_names.baml:12:12 (Parameter) "age:"
// call_arg_names.baml:26:5 (Parameter) "c:"
