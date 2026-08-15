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
//    ╭─[top_level_binding.baml:1:1]
//  1 │ let x = 1;
//    · ─────┬────
//    ·      ╰── move this binding into a function body
//    ╰────
// E0010
//
//   × top-level `let` bindings are not supported
//    ╭─[top_level_binding.baml:3:1]
//  3 │ ╭─▶ let y = {
//  4 │ │       let b = 2;
//  5 │ │       [1,2,3]
//  6 │ ├─▶ };
//    · ╰──── move this binding into a function body
//    ╰────
//
//- semantic_tokens
// top_level_binding.baml:1:1 (keyword) len=3 "let"
// top_level_binding.baml:1:5 (variable) [declaration] len=1 "x"
// top_level_binding.baml:1:7 (operator) len=1 "="
// top_level_binding.baml:1:9 (number) len=1 "1"
// top_level_binding.baml:3:1 (keyword) len=3 "let"
// top_level_binding.baml:3:5 (variable) [declaration] len=1 "y"
// top_level_binding.baml:3:7 (operator) len=1 "="
// top_level_binding.baml:4:5 (keyword) len=3 "let"
// top_level_binding.baml:4:9 (variable) [declaration] len=1 "b"
// top_level_binding.baml:4:11 (operator) len=1 "="
// top_level_binding.baml:4:13 (number) len=1 "2"
// top_level_binding.baml:5:6 (number) len=1 "1"
// top_level_binding.baml:5:8 (number) len=1 "2"
// top_level_binding.baml:5:10 (number) len=1 "3"
