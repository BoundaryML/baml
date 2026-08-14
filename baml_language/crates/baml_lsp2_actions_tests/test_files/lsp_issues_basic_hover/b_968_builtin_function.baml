function CopyValue() -> int {
  return baml.deep_<[CURSOR]copy(1)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_968_builtin_function.baml:2:20
// ```baml
// function deep_copy(value: T) -> T throws never
// ```
