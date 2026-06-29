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
// error: non-exhaustive match on type Color; missing: user.Color.Blue
//     ╭─[ match_match_non_exhaustive.baml:9:3 ]
//     │
//   9 │ ╭─▶   match (c) {
//     ┆ ┆
//  12 │ ├─▶   }
//     │ │
//     │ ╰───────── non-exhaustive match on type Color; missing: user.Color.Blue
//     │
//     │     Note: Error code: E0062
// ────╯
// error: non-exhaustive match on type Color; missing: user.Color.Green, user.Color.Blue
//     ╭─[ match_match_non_exhaustive.baml:16:3 ]
//     │
//  16 │ ╭─▶   match (c) {
//     ┆ ┆
//  18 │ ├─▶   }
//     │ │
//     │ ╰───────── non-exhaustive match on type Color; missing: user.Color.Green, user.Color.Blue
//     │
//     │     Note: Error code: E0062
// ────╯
