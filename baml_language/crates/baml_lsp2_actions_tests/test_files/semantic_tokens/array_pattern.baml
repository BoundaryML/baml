// Array destructure in let-else: bare identifiers inside a pattern are
// TYPE patterns by default, so the binding names need explicit `let`
// markers. The whole pattern is refutable on `int[]` because the array
// could be shorter than two elements.
function FirstTwo(xs: int[]) -> int {
  let [let a, let b] = xs else { return -1; };
  a + b
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// array_pattern.baml:1:1 (comment) len=71 "// Array destructure in let-else: bare identifiers inside a pattern are"
// array_pattern.baml:2:1 (comment) len=69 "// TYPE patterns by default, so the binding names need explicit `let`"
// array_pattern.baml:3:1 (comment) len=71 "// markers. The whole pattern is refutable on `int[]` because the array"
// array_pattern.baml:4:1 (comment) len=38 "// could be shorter than two elements."
// array_pattern.baml:5:1 (keyword) len=8 "function"
// array_pattern.baml:5:10 (function) [declaration] len=8 "FirstTwo"
// array_pattern.baml:5:19 (parameter) [declaration] len=2 "xs"
// array_pattern.baml:5:23 (type) [defaultLibrary] len=3 "int"
// array_pattern.baml:5:33 (type) [defaultLibrary] len=3 "int"
// array_pattern.baml:6:3 (keyword) len=3 "let"
// array_pattern.baml:6:8 (keyword) len=3 "let"
// array_pattern.baml:6:12 (variable) [declaration] len=1 "a"
// array_pattern.baml:6:15 (keyword) len=3 "let"
// array_pattern.baml:6:19 (variable) [declaration] len=1 "b"
// array_pattern.baml:6:22 (operator) len=1 "="
// array_pattern.baml:6:24 (parameter) len=2 "xs"
// array_pattern.baml:6:27 (keyword) len=4 "else"
// array_pattern.baml:6:34 (keyword) len=6 "return"
// array_pattern.baml:6:41 (operator) len=1 "-"
// array_pattern.baml:6:42 (number) len=1 "1"
// array_pattern.baml:7:3 (variable) len=1 "a"
// array_pattern.baml:7:5 (operator) len=1 "+"
// array_pattern.baml:7:7 (variable) len=1 "b"
