// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
//       messageContains: mismatched types
// docs:endmeta

// docs:start example
function ReturnNumber(value: int) -> string {
  value
}
// docs:end example
