function FooBar(arg: int) {
  client "openai/gpt-3.5-turbo"
  prompt #""#
}

// error: Invalid syntax for function "FooBar". Use:
// function FooBar(params...) -> ReturnType { ... }
//   -->  functions_v2/invalid_no_return.baml:1
//    | 
//    | 
//  1 | function FooBar(arg: int) {
//    |

//----
//- diagnostics
// Error: Expected return type (->), found LBrace
//    ╭─[ 0:1:27 ]
//    │
//  1 │ function FooBar(arg: int) {
//    │                           ┬  
//    │                           ╰── Expected return type (->), found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected client name, found Quote
//    ╭─[ 0:2:10 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:2:10 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:2:11 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:2:17 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-3', found Word
//    ╭─[ 0:2:18 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                  ──┬──  
//    │                    ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-3', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '.', found Dot
//    ╭─[ 0:2:23 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                       ┬  
//    │                       ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '.', found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '5', found IntegerLiteral
//    ╭─[ 0:2:24 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                        ┬  
//    │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '5', found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '-', found Minus
//    ╭─[ 0:2:25 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                         ┬  
//    │                         ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '-', found Minus
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'turbo', found Word
//    ╭─[ 0:2:26 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                          ──┬──  
//    │                            ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'turbo', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:2:31 ]
//    │
//  2 │   client "openai/gpt-3.5-turbo"
//    │                               ┬  
//    │                               ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
