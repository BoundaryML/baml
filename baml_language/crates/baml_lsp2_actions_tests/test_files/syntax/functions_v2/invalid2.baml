function Foo1(arg: int) -> float {
  client Foo
}

function Foo2(arg: int) -> float {
  prompt #"..."#
}

function Foo3(arg: int) -> float {
  client bar
  prompt "..."
}


function Foo4(arg: int) -> float {
  client bar
  prompt
}


function Foo5(arg: int) -> float {
  client
  prompt #"..."#
}


function Foo6(arg: int) -> float {
  client
  prompt
}

// error: Error validating Function "Foo4": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:17
//    |
// 16 |   client bar
// 17 |   prompt
//    |
// error: Error validating Function "Foo5": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:22
//    |
// 21 | function Foo5(arg: int) -> float {
// 22 |   client
//    |
// error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:28
//    |
// 27 | function Foo6(arg: int) -> float {
// 28 |   client
//    |
// error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:29
//    |
// 28 |   client
// 29 |   prompt
//    |
// error: Error validating: Missing `prompt` field in function. Add to the block:
// ```
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:1
//    |
//    |
//  1 | function Foo1(arg: int) -> float {
//    |
// error: Error validating: Missing `client` field in function. Add to the block:
// ```
// client GPT4
// ```
//   -->  functions_v2/invalid2.baml:5
//    |
//  4 |
//  5 | function Foo2(arg: int) -> float {
//    |
// error: Expected a template_string value, but received string value `"..."`.
//   -->  functions_v2/invalid2.baml:11
//    |
// 10 |   client bar
// 11 |   prompt "..."
//    |
// error: Error validating: Missing `prompt` field in function. Add to the block:
// ```
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:9
//    |
//  8 |
//  9 | function Foo3(arg: int) -> float {
//    |
// error: Error validating: Missing `prompt` field in function. Add to the block:
// ```
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:15
//    |
// 14 |
// 15 | function Foo4(arg: int) -> float {
//    |
// error: Error validating: Missing `client` field in function. Add to the block:
// ```
// client GPT4
// ```
//   -->  functions_v2/invalid2.baml:21
//    |
// 20 |
// 21 | function Foo5(arg: int) -> float {
//    |
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:27
//    |
// 26 |
// 27 | function Foo6(arg: int) -> float {
//    |

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid2.baml:3:1]
//  3 │ }
//    · ┬
//    · ╰── expected `LLM function missing 'prompt' field`, found `'}'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid2.baml:7:1]
//  7 │ }
//    · ┬
//    · ╰── expected `LLM function missing 'client' field`, found `'}'`
//    ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:18:1]
//  18 │ }
//     · ┬
//     · ╰── expected `prompt string`, found `'}'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[functions_v2_invalid2.baml:30:1]
//  30 │ }
//     · ┬
//     · ╰── expected `prompt string`, found `'}'`
//     ╰────
// E0003
//
//   × unresolved name: `Foo`
//    ╭─[functions_v2_invalid2.baml:1:34]
//  1 │ ╭─▶ function Foo1(arg: int) -> float {
//  2 │ │     client Foo
//  3 │ ╰─▶ }
//    ╰────
// E0001
//
//   × mismatched types
//    ╭─[functions_v2_invalid2.baml:5:34]
//  5 │ ╭─▶ function Foo2(arg: int) -> float {
//  6 │ │     prompt #"..."#
//  7 │ ├─▶ }
//    · ╰──── expected `baml.llm.Client`, found `null`
//    ╰────
// E0001
//
//   × mismatched types
//    ╭─[functions_v2_invalid2.baml:5:1]
//  5 │ ╭─▶ function Foo2(arg: int) -> float {
//  6 │ │     prompt #"..."#
//  7 │ ├─▶ }
//    · ╰──── expected `baml.llm.Client`, found `null`
//    ╰────
// E0003
//
//   × unresolved name: `bar`
//     ╭─[functions_v2_invalid2.baml:9:34]
//   9 │ ╭─▶ function Foo3(arg: int) -> float {
//  10 │ │     client bar
//  11 │ │     prompt "..."
//  12 │ ╰─▶ }
//     ╰────
// E0003
//
//   × unresolved name: `bar`
//     ╭─[functions_v2_invalid2.baml:15:34]
//  15 │ ╭─▶ function Foo4(arg: int) -> float {
//  16 │ │     client bar
//  17 │ │     prompt
//  18 │ ╰─▶ }
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[functions_v2_invalid2.baml:21:34]
//  21 │ ╭─▶ function Foo5(arg: int) -> float {
//  22 │ │     client
//  23 │ │     prompt #"..."#
//  24 │ ├─▶ }
//     · ╰──── expected `baml.llm.Client`, found `null`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[functions_v2_invalid2.baml:21:1]
//  21 │ ╭─▶ function Foo5(arg: int) -> float {
//  22 │ │     client
//  23 │ │     prompt #"..."#
//  24 │ ├─▶ }
//     · ╰──── expected `baml.llm.Client`, found `null`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[functions_v2_invalid2.baml:27:34]
//  27 │ ╭─▶ function Foo6(arg: int) -> float {
//  28 │ │     client
//  29 │ │     prompt
//  30 │ ├─▶ }
//     · ╰──── expected `baml.llm.Client`, found `null`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[functions_v2_invalid2.baml:27:1]
//  27 │ ╭─▶ function Foo6(arg: int) -> float {
//  28 │ │     client
//  29 │ │     prompt
//  30 │ ├─▶ }
//     · ╰──── expected `baml.llm.Client`, found `null`
//     ╰────
