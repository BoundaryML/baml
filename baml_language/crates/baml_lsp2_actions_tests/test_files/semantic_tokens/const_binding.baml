function ConstToken() -> int {
  const x = 1;
  let const = x;
  const
}

//----
//- diagnostics
// E0010
//
//   ⚠ `const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.
//    ╭─[const_binding.baml:2:3]
//  2 │   const x = 1;
//    ·   ──┬──
//    ·     ╰── `const` behaves like `let` for now
//    ╰────
// E0010
//
//   × `const` is reserved and cannot be used as a binding name
//    ╭─[const_binding.baml:3:7]
//  3 │   let const = x;
//    ·       ──┬──
//    ·         ╰── `const` is reserved here
//    ╰────
//
//- semantic_tokens
// const_binding.baml:1:1 (keyword) len=8 "function"
// const_binding.baml:1:10 (function) [declaration] len=10 "ConstToken"
// const_binding.baml:1:26 (type) [defaultLibrary] len=3 "int"
// const_binding.baml:2:3 (keyword) len=5 "const"
// const_binding.baml:2:9 (variable) [declaration] len=1 "x"
// const_binding.baml:2:11 (operator) len=1 "="
// const_binding.baml:2:13 (number) len=1 "1"
// const_binding.baml:3:3 (keyword) len=3 "let"
// const_binding.baml:3:7 (variable) [declaration] len=5 "const"
// const_binding.baml:3:13 (operator) len=1 "="
// const_binding.baml:3:15 (variable) len=1 "x"
// const_binding.baml:4:3 (variable) len=5 "const"
