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
// E0062
//
//   × non-exhaustive match on type Color; missing: user.Color.Blue
//     ╭─[match_match_non_exhaustive.baml:9:3]
//   9 │ ╭─▶   match (c) {
//  10 │ │       Color.Red => "red",
//  11 │ │       Color.Green => "green"
//  12 │ ╰─▶   }
//     ╰────
// E0062
//
//   × non-exhaustive match on type Color; missing: user.Color.Green, user.Color.Blue
//     ╭─[match_match_non_exhaustive.baml:16:3]
//  16 │ ╭─▶   match (c) {
//  17 │ │       Color.Red => "red"
//  18 │ ╰─▶   }
//     ╰────
