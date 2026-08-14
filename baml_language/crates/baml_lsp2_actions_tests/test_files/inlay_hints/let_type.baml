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
// let_type.baml:15:8 (Type) ": Color.R"
// let_type.baml:21:8 (Type) ": int | string | Color"
// let_type.baml:27:8 (Type) ": map<string, int | string | Color>"
// let_type.baml:33:8 (Type) ": (a: map<string, int | string | Color>) -> map<string, int | string | Color> throws never"
// let_type.baml:35:5 (Parameter) "a: "
