let x = 1;

let y = {
    let b = 2;
    [1,2,3]
};

//----
//- diagnostics
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_top_level_binding.baml:1:1]
//  1 │ let x = 1;
//    · ─────┬────
//    ·      ╰── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[expr_top_level_binding.baml:3:1]
//  3 │ ╭─▶ let y = {
//  4 │ │       let b = 2;
//  5 │ │       [1,2,3]
//  6 │ ├─▶ };
//    · ╰──── move this binding into a function body
//    ╰────
