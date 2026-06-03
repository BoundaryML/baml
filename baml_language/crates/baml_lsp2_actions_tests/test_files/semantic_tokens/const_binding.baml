function ConstToken() -> int {
  const x = 1;
  let const = x;
  const
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// const_binding.baml:1:1 (keyword) len=8 "function"
// const_binding.baml:1:10 (function) len=10 "ConstToken"
// const_binding.baml:1:26 (type) len=3 "int"
// const_binding.baml:2:3 (keyword) len=5 "const"
// const_binding.baml:2:9 (variable) len=1 "x"
// const_binding.baml:2:11 (operator) len=1 "="
// const_binding.baml:2:13 (number) len=1 "1"
// const_binding.baml:3:3 (keyword) len=3 "let"
// const_binding.baml:3:7 (variable) len=5 "const"
// const_binding.baml:3:13 (operator) len=1 "="
// const_binding.baml:3:15 (variable) len=1 "x"
