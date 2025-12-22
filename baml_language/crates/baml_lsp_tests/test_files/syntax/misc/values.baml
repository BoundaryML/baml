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
// Error: Expected client name, found Quote
//     ╭─[ 0:12:10 ]
//     │
//  12 │   client "openai/gpt-4o-mini"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:12:10 ]
//     │
//  12 │   client "openai/gpt-4o-mini"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:12:11 ]
//     │
//  12 │   client "openai/gpt-4o-mini"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:12:17 ]
//     │
//  12 │   client "openai/gpt-4o-mini"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o-mini', found Word
//     ╭─[ 0:12:18 ]
//     │
//  12 │   client "openai/gpt-4o-mini"
//     │                  ─────┬─────  
//     │                       ╰─────── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o-mini', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:12:29 ]
//     │
//  12 │   client "openai/gpt-4o-mini"
//     │                             ┬  
//     │                             ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Comma
//     ╭─[ 0:23:29 ]
//     │
//  23 │     str_array_field ["hello", "world"]
//     │                             ┬  
//     │                             ╰── Expected config key, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Quote
//     ╭─[ 0:23:31 ]
//     │
//  23 │     str_array_field ["hello", "world"]
//     │                               ┬  
//     │                               ╰── Expected config key, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unclosed string literal, found EOF
//     ╭─[ 0:40:7 ]
//     │
//  40 │ //    |
//     │       ┬  
//     │       ╰── Expected Unclosed string literal, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found EOF
//     ╭─[ 0:40:7 ]
//     │
//  40 │ //    |
//     │       ┬  
//     │       ╰── Expected RBrace, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found EOF
//     ╭─[ 0:40:7 ]
//     │
//  40 │ //    |
//     │       ┬  
//     │       ╰── Expected RBrace, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate test 'Foo'
//    ╭─[ 0:1:1 ]
//    │
//  1 │ class Input {
//    │ │ 
//    │ ╰─ test 'Foo' defined in misc_values.baml
//    │ │ 
//    │ ╰─ 'Foo' previously defined in misc_values.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
