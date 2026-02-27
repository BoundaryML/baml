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

// Hint suppressed when arg name matches param name.
function same_name(name string) -> string {
  let name = "Alice";
  greet(name, 30)
}

// Hint suppressed when arg's final path segment contains param name.
function contains_name(my_name string) -> string {
  let my_name = "Bob";
  greet(my_name, 30)
}

// Hint NOT suppressed when arg name doesn't match.
function different_name(x string) -> string {
  let x = "Charlie";
  greet(x, 30)
}

class Info {
  name string
  age int
}

// Hint suppressed when final segment of field access matches param name.
function field_access(info Info) -> string {
  greet(info.name, info.age)
}

// Multi-line call: hints appear on the correct line for each argument.
function multiline() -> string {
  greet(
    "Eve",
    42
  )
}

class Info2 {
    n string
    a int
}

// Multi-line call with field access args.
function multiline_field(info Info2) -> string {
  greet(
    info.n,
    info.a
  )
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
// call_arg_names.baml:31:11 (Type) ": string"
//   edit@31:11 ": string"
// call_arg_names.baml:32:15 (Parameter) "age:"
// call_arg_names.baml:37:14 (Type) ": string"
//   edit@37:14 ": string"
// call_arg_names.baml:38:18 (Parameter) "age:"
// call_arg_names.baml:43:8 (Type) ": string"
//   edit@43:8 ": string"
// call_arg_names.baml:44:9 (Parameter) "name:"
// call_arg_names.baml:44:12 (Parameter) "age:"
// call_arg_names.baml:60:5 (Parameter) "name:"
// call_arg_names.baml:61:5 (Parameter) "age:"
// call_arg_names.baml:73:5 (Parameter) "name:"
// call_arg_names.baml:74:5 (Parameter) "age:"
