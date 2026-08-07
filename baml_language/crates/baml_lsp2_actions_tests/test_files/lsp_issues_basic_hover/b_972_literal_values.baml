class LiteralFields {
  mode "fa<[CURSOR]st" | "slow"
  count 4<[CURSOR]2 | 100
  enabled tr<[CURSOR]ue
  empty nu<[CURSOR]ll
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_972_literal_values.baml:2:11
// ```baml
// "fast"
// ```
// An exact `string` literal type. Its base type is `string`.
// hover at b_972_literal_values.baml:3:10
// ```baml
// 42
// ```
// An exact `int` literal type. Its base type is `int`.
// hover at b_972_literal_values.baml:4:13
// ```baml
// true
// ```
// A boolean literal type. Its base type is `bool`.
// hover at b_972_literal_values.baml:5:11
// ```baml
// null
// ```
// The `null` literal type. Its base type is `null`.
