// Regression: diagnostics inside expression functions should underline the
// expression that caused the error, not the start of the file.

function FunctionBodySpan() -> int {
  return missing_value
}

function ReturnTypeSpan() -> string {
  return 1
}

//----
//- diagnostics
// Error: unresolved name: missing_value
//    ╭─[ expr_diagnostic_span_regression.baml:5:10 ]
//    │
//  5 │   return missing_value
//    │          ──────┬──────  
//    │                ╰──────── unresolved name: missing_value
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: type mismatch: expected string, got 1
//    ╭─[ expr_diagnostic_span_regression.baml:9:10 ]
//    │
//  9 │   return 1
//    │          ┬  
//    │          ╰── type mismatch: expected string, got 1
//    │ 
//    │ Note: Error code: E0001
// ───╯
