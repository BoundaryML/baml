function UseReservedAsync() -> int {
  let async = 1;
  async
}

// `async` is held back for future language surface, so binding it is an error
// here just as it was in v1 — v1 reported it as:
//
//   error: Error validating: 'async' is a reserved keyword.
//     -->  expr/var_keyword_let_async.baml:2
//      |
//    1 | function UseReservedAsync() -> int {
//    2 |   let async = 1;
//      |
//
// The trailing `async` reference resolves to the binding, so rejecting the
// name costs no follow-on "unresolved name" diagnostic.

//----
//- diagnostics
// E0158
//
//   × `async` is reserved for future use and cannot be used as a binding name
//    ╭─[expr_var_keyword_let_async.baml:2:7]
//  2 │   let async = 1;
//    ·       ──┬──
//    ·         ╰── reserved for future use
//    ╰────
