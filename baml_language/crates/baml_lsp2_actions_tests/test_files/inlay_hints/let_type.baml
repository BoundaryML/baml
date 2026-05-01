enum Color {
  R
  G
  B
}

function foo() -> string {
  // primitive types
  let x = "hello";
  x
}

function bar() -> Color {
  // enum types
  let x = Color.R;
  x
}

function baz(a: Color | int | string | "hello") -> Color | int | string | "hello" {
  // union types
  let x = a;
  x
}

function qux(a: map<string, Color | int | string | "hello">) -> map<string, Color | int | string | "hello"> {
  // map types
  let x = a;
  x
}

function quux(a: map<string, Color | int | string | "hello">) -> map<string, Color | int | string | "hello"> {
  // function types
  let x = qux;

  x({"hello": 1})
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- inlay_hints
// let_type.baml:9:8 (Type) ": string"
// let_type.baml:15:8 (Type) ": user.Color.R"
// let_type.baml:21:8 (Type) ": user.Color | int | string | \"hello\""
// let_type.baml:27:8 (Type) ": map<string, user.Color | int | string | \"hello\">"
// let_type.baml:33:8 (Type) ": (a: map<string, user.Color | int | string | \"hello\">) -> map<string, user.Color | int | string | \"hello\"> throws never"
// let_type.baml:35:5 (Parameter) "a: "
