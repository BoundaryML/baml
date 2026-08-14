interface Mapper<Input, Output> {
  function map(se<[CURSOR]lf, va<[CURSOR]lue: Input) -> Output throws never
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_978_interface_method_parameters.baml:2:18
// ```baml
// self: Self
// ```
// hover at b_978_interface_method_parameters.baml:2:24
// ```baml
// value: Input
// ```
