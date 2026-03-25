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
// <no-inlay-hints>
