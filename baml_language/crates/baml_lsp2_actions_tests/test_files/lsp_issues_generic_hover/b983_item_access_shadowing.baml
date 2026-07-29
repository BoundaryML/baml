class Lorem<T> {
  a: T
}

function use(a: Lorem<int>) -> void {
  let b: Lorem<int> = a
  b.a<[CURSOR]
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-983: hovering b.a should show the instantiated field type `a: int`, not the same-named parameter's type.)
// hover at b983_item_access_shadowing.baml:7:6
// ```baml
// a: int
// ```
