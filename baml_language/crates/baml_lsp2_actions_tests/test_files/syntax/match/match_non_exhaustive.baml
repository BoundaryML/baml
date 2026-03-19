// Non-exhaustive match should emit diagnostic
enum Color {
  Red
  Green
  Blue
}

function MissingOneVariant(c: Color) -> string {
  match (c) {
    Color.Red => "red",
    Color.Green => "green"
  }
}

function MissingMultipleVariants(c: Color) -> string {
  match (c) {
    Color.Red => "red"
  }
}

//----
//- diagnostics
// Error: non-exhaustive match on `user.Color`; missing: Color.Blue
//     ╭─[ match_match_non_exhaustive.baml:8:49 ]
//     │
//   8 │ ╭─▶ function MissingOneVariant(c: Color) -> string {
//     ┆ ┆   
//  12 │ ├─▶   }
//     │ │         
//     │ ╰───────── non-exhaustive match on `user.Color`; missing: Color.Blue
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: non-exhaustive match on `user.Color`; missing: Color.Blue, Color.Green
//     ╭─[ match_match_non_exhaustive.baml:15:55 ]
//     │
//  15 │ ╭─▶ function MissingMultipleVariants(c: Color) -> string {
//     ┆ ┆   
//  18 │ ├─▶   }
//     │ │         
//     │ ╰───────── non-exhaustive match on `user.Color`; missing: Color.Blue, Color.Green
//     │     
//     │     Note: Error code: E0001
// ────╯
