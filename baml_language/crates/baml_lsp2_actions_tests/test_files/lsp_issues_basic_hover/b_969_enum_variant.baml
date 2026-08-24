enum Status {
  Rea<[CURSOR]dy
  Waiting
}

function GetStatus() -> Status {
  return Status.Re<[CURSOR]ady
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_969_enum_variant.baml:2:6
// ```baml
// Status.Ready: Status
// ```
// hover at b_969_enum_variant.baml:7:19
// ```baml
// Status.Ready: Status
// ```
