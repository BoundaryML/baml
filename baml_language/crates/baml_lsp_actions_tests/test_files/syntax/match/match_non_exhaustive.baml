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
// Error: Non-exhaustive match on `Color`: missing cases Color.Blue
//     ╭─[ match_match_non_exhaustive.baml:8:49 ]
//     │
//   8 │ ╭─▶ function MissingOneVariant(c: Color) -> string {
//     ┆ ┆   
//  12 │ ├─▶   }
//     │ │         
//     │ ╰───────── Non-exhaustive match on `Color`: missing cases Color.Blue
//     │     
//     │     Note: Error code: E0062
// ────╯
// Error: Non-exhaustive match on `Color`: missing cases Color.Green, Color.Blue
//     ╭─[ match_match_non_exhaustive.baml:15:55 ]
//     │
//  15 │ ╭─▶ function MissingMultipleVariants(c: Color) -> string {
//     ┆ ┆   
//  18 │ ├─▶   }
//     │ │         
//     │ ╰───────── Non-exhaustive match on `Color`: missing cases Color.Green, Color.Blue
//     │     
//     │     Note: Error code: E0062
// ────╯
