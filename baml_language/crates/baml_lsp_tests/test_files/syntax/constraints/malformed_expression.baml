class Foo {
  bar string @check(bar_check, {{ ) }})
}


function FunctionName(arg:string) -> "foo" {
  client "openai/gpt-4o"
  prompt #"
    Your prompt here in jinja format
    {{ ) }}
  "#
}


function FunctionName2(arg:string) -> "foo" {
  client "openai/gpt-4o"
  prompt #"
    Your prompt here in jinja format
    {{ if foo }}
    {{ foo }}
    {{ endif }}
  "#
}

// error: Error validating: Error parsing jinja template: syntax error: unexpected `)` (in <expression>:1)
//   -->  constraints/malformed_expression.baml:2
//    | 
//  1 | class Foo {
//  2 |   bar string @check(bar_check, {{ ) }})
//    | 
// error: Error validating: Error parsing jinja template: syntax error: unexpected `)` (in FunctionName:3)
//   -->  constraints/malformed_expression.baml:10
//    | 
//  9 |     Your prompt here in jinja format
// 10 |     {{ ) }}
//    | 
// error: Error validating: Error parsing jinja template: syntax error: unexpected identifier, expected end of variable block (in FunctionName2:3)
//   -->  constraints/malformed_expression.baml:19
//    | 
// 18 |     Your prompt here in jinja format
// 19 |     {{ if foo }}
//    |

//----
//- diagnostics
// Error: Expected client name, found Quote
//    ╭─[ 0:7:10 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:7:10 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:7:11 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:7:17 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:7:18 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │                  ───┬──  
//    │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:7:24 ]
//    │
//  7 │   client "openai/gpt-4o"
//    │                        ┬  
//    │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected client name, found Quote
//     ╭─[ 0:16:10 ]
//     │
//  16 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:16:10 ]
//     │
//  16 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:16:11 ]
//     │
//  16 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:16:17 ]
//     │
//  16 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:16:18 ]
//     │
//  16 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:16:24 ]
//     │
//  16 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
