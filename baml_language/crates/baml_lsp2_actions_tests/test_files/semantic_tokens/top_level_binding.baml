let x = 1;

let y = {
    let b = 2;
    [1,2,3]
};

//----
//- diagnostics
// <no-diagnostics-expected>
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
