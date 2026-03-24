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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: non-exhaustive match on `user.Color`; missing: user.Color.Blue
//     ╭─[ match_match_non_exhaustive.baml:8:49 ]
//     │
//   8 │ ╭─▶ function MissingOneVariant(c: Color) -> string {
//     ┆ ┆   
//  12 │ ├─▶   }
//     │ │         
//     │ ╰───────── non-exhaustive match on `user.Color`; missing: user.Color.Blue
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: non-exhaustive match on `user.Color`; missing: user.Color.Blue, user.Color.Green
//     ╭─[ match_match_non_exhaustive.baml:15:55 ]
//     │
//  15 │ ╭─▶ function MissingMultipleVariants(c: Color) -> string {
//     ┆ ┆   
//  18 │ ├─▶   }
//     │ │         
//     │ ╰───────── non-exhaustive match on `user.Color`; missing: user.Color.Blue, user.Color.Green
//     │     
//     │     Note: Error code: E0001
// ────╯
