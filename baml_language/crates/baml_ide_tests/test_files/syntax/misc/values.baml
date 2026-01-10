class Input {
  str_field string
  int_field int
  bool_field bool

  str_array_field string[]
  int_array_field int[]
  bool_array_field bool[]
}

function Foo(input: Input) -> string {
  client "openai/gpt-4o-mini"
  prompt #"Tell me a haiku."#
}

test Foo {
  functions [Foo]
  input {
    str_field "hello"
    int_field 123
    bool_field true

    str_array_field ["hello", "world"]
    int_array_field [1, 2, 3]
    bool_array_field [true, false]
  }
}

// error: Property not known: "input". Did you mean one of these: "args", "functions"?
//   -->  tests/values.baml:18
//    | 
// 17 |   functions [Foo]
// 18 |   input {
//    | 
// error: Error validating: Missing `args` property
//   -->  tests/values.baml:16
//    | 
// 15 | 
// 16 | test Foo {
//    |

//----
//- diagnostics
// Error: Property not known: "input". Did you mean one of these: "functions", "args", "type_builder"?
//     ╭─[ misc_values.baml:18:3 ]
//     │
//  18 │   input {
//     │   ──┬──  
//     │     ╰──── Property not known: "input". Did you mean one of these: "functions", "args", "type_builder"?
//     │ 
//     │ Note: Error code: E0034
// ────╯
// Error: Missing `args` property
//     ╭─[ misc_values.baml:16:6 ]
//     │
//  16 │ test Foo {
//     │      ─┬─  
//     │       ╰─── Missing `args` property
//     │ 
//     │ Note: Error code: E0035
// ────╯
