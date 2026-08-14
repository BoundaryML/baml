class Lorem<T> {
  value: T
}

function use(<[CURSOR]a: Lorem<int>) -> void {
  let b: Lorem<int> = <[CURSOR]a
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-984: hovering a parameter reference should preserve the generic argument in `Lorem<int>`.)
// hover at b984_parameter_generics.baml:5:14
// ```baml
// a: Lorem<int>
// ```
// hover at b984_parameter_generics.baml:6:23
// ```baml
// a: Lorem<int>
// ```
