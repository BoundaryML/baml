class Input {
  str_field string
}

function Foo(input: Input[]) -> string {
  client "openai/gpt-4o-mini"
  prompt #"Tell me a haiku."#
}

test Foo {
  functions [Foo]
  input [
    {
      str_field "hello"
    },,
  ]
}

// error: Error validating: Invalid array syntax detected.
//   -->  tests/bad_syntax.baml:15
//    | 
// 14 |       str_field "hello"
// 15 |     },,
// 16 |   ]
//    | 
// error: Property not known: "input". Did you mean one of these: "args", "functions"?
//   -->  tests/bad_syntax.baml:12
//    | 
// 11 |   functions [Foo]
// 12 |   input [
//    | 
// error: Error validating: Missing `args` property
//   -->  tests/bad_syntax.baml:10
//    | 
//  9 | 
// 10 | test Foo {
//    |

//----
//- diagnostics
// Error: Property not known: "input". Did you mean one of these: "functions", "args", "type_builder"?
//     ╭─[ misc_bad_syntax.baml:12:3 ]
//     │
//  12 │   input [
//     │   ──┬──  
//     │     ╰──── Property not known: "input". Did you mean one of these: "functions", "args", "type_builder"?
//     │ 
//     │ Note: Error code: E0034
// ────╯
// Error: Missing `args` property
//     ╭─[ misc_bad_syntax.baml:10:6 ]
//     │
//  10 │ test Foo {
//     │      ─┬─  
//     │       ╰─── Missing `args` property
//     │ 
//     │ Note: Error code: E0035
// ────╯
