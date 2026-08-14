// Bigint literals (`42n`) should highlight as numbers, like int/float.

function BigMath() -> bigint {
  let a: bigint = 42n;
  let b = a + 100n;
  b
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// bigint_literals.baml:1:1 (comment) len=71 "// Bigint literals (`42n`) should highlight as numbers, like int/float."
// bigint_literals.baml:3:1 (keyword) len=8 "function"
// bigint_literals.baml:3:10 (function) [declaration] len=7 "BigMath"
// bigint_literals.baml:3:23 (type) [defaultLibrary] len=6 "bigint"
// bigint_literals.baml:4:3 (keyword) len=3 "let"
// bigint_literals.baml:4:7 (variable) [declaration] len=1 "a"
// bigint_literals.baml:4:10 (type) [defaultLibrary] len=6 "bigint"
// bigint_literals.baml:4:17 (operator) len=1 "="
// bigint_literals.baml:4:19 (number) len=3 "42n"
// bigint_literals.baml:5:3 (keyword) len=3 "let"
// bigint_literals.baml:5:7 (variable) [declaration] len=1 "b"
// bigint_literals.baml:5:9 (operator) len=1 "="
// bigint_literals.baml:5:11 (variable) len=1 "a"
// bigint_literals.baml:5:13 (operator) len=1 "+"
// bigint_literals.baml:5:15 (number) len=4 "100n"
// bigint_literals.baml:6:3 (variable) len=1 "b"
