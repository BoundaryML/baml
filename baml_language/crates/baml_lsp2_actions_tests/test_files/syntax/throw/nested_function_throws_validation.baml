function BadNestedBuiltin(cb: () -> int throws $rust_type) -> int {
  cb()
}

function BadNestedUnknownAttr(cb: () -> int throws string @unknown) -> int {
  cb()
}

//----
//- diagnostics
// Error: Builtin-only syntax `$rust_type` is only allowed in builtin stdlib files
//    ╭─[ throw_nested_function_throws_validation.baml:1:30 ]
//    │
//  1 │ function BadNestedBuiltin(cb: () -> int throws $rust_type) -> int {
//    │                              ──────────────┬─────────────
//    │                                            ╰─────────────── builtin-only syntax
//    │
//    │ Note: Error code: E0016
// ───╯
// Error: Unknown attribute `@unknown`
//    ╭─[ throw_nested_function_throws_validation.baml:5:59 ]
//    │
//  5 │ function BadNestedUnknownAttr(cb: () -> int throws string @unknown) -> int {
//    │                                                           ────┬───
//    │                                                               ╰───── unknown attribute
//    │
//    │ Note: Error code: E0015
// ───╯
