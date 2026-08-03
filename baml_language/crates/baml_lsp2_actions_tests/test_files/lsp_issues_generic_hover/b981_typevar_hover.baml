interface Foo<T> {
  function value(self) -> T throws never
}

class Lorem<<[CURSOR]T extends Foo<int>> {
  value: <[CURSOR]T
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-981: hovering T should show that it is Lorem's type parameter and is bounded by Foo<int>.)
// hover at b981_typevar_hover.baml:5:13
// ```baml
// type parameter T extends Foo<int>
// ```
// Declared by `class Lorem`.
// hover at b981_typevar_hover.baml:6:10
// ```baml
// type parameter T extends Foo<int>
// ```
// Declared by `class Lorem`.
