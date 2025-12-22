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
// Error: Expected client name, found Quote
//    ╭─[ 0:6:10 ]
//    │
//  6 │   client "openai/gpt-4o-mini"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:6:10 ]
//    │
//  6 │   client "openai/gpt-4o-mini"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:6:11 ]
//    │
//  6 │   client "openai/gpt-4o-mini"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:6:17 ]
//    │
//  6 │   client "openai/gpt-4o-mini"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o-mini', found Word
//    ╭─[ 0:6:18 ]
//    │
//  6 │   client "openai/gpt-4o-mini"
//    │                  ─────┬─────  
//    │                       ╰─────── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o-mini', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:6:29 ]
//    │
//  6 │   client "openai/gpt-4o-mini"
//    │                             ┬  
//    │                             ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:13:5 ]
//     │
//  13 │     {
//     │     ┬  
//     │     ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:15:6 ]
//     │
//  15 │     },,
//     │      ┬  
//     │      ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Comma
//     ╭─[ 0:15:7 ]
//     │
//  15 │     },,
//     │       ┬  
//     │       ╰── Expected top-level declaration, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBracket
//     ╭─[ 0:16:3 ]
//     │
//  16 │   ]
//     │   ┬  
//     │   ╰── Expected top-level declaration, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:17:1 ]
//     │
//  17 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Duplicate test 'Foo'
//    ╭─[ 0:1:1 ]
//    │
//  1 │ class Input {
//    │ │ 
//    │ ╰─ test 'Foo' defined in misc_bad_syntax.baml
//    │ │ 
//    │ ╰─ 'Foo' previously defined in misc_bad_syntax.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
