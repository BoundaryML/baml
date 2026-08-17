function inspect(value: unknown) -> string {
  return value.<[CURSOR]get_field<string>("name")
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// SHOULD_CONTAIN: get_field
