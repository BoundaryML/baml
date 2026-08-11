function FooBar(arg: int) {
  client "openai/gpt-3.5-turbo"
  prompt ``
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid_no_return.baml:1:27]
//  1 │ function FooBar(arg: int) {
//    ·                           ┬
//    ·                           ╰── expected `return type (->)`, found `'{'`
//    ╰────
// E0010
//
//   × invalid syntax
//    ╭─[functions_v2_invalid_no_return.baml:3:10]
//  3 │   prompt ``
//    ·          ┬
//    ·          ╰── Empty multi-tick backtick string (``) has no matching `` close. Use "" for empty strings (BEP-049 §8).
//    ╰────
