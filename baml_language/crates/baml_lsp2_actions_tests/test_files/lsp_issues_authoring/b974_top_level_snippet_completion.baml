fun<[CURSOR]

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[b974_top_level_snippet_completion.baml:1:1]
//  1 │ fun
//    · ─┬─
//    ·  ╰── expected `top-level declaration`, found `identifier`
//    ╰────
//
//- completions
// (The inline action harness checks the label; the Rust action test checks snippet text and format.)
// SHOULD_CONTAIN: function
