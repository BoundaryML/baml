class PrimitiveFields {
  name str<[CURSOR]ing
  count i<[CURSOR]nt
  active bo<[CURSOR]ol
  score flo<[CURSOR]at
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_970_primitive_types.baml:2:11
// ```baml
// string
// ```
// A UTF-8 encoded string.
// hover at b_970_primitive_types.baml:3:10
// ```baml
// int
// ```
// A 63-bit signed integer. Range: -2^62 to 2^62-1 (`-4_611_686_018_427_387_904` to `4_611_686_018_427_387_903`).
// hover at b_970_primitive_types.baml:4:12
// ```baml
// bool
// ```
// A boolean value — either `true` or `false`.
// hover at b_970_primitive_types.baml:5:12
// ```baml
// float
// ```
// A 64-bit IEEE 754 floating-point number (f64).
