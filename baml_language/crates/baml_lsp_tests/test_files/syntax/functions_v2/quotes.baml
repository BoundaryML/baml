class Foo {
  name string
}

function Bar(foo: Foo) -> Foo {
  client "openai/gpt-4o"
  prompt #"
    {{ ctx.output_format(quote_class_fields=true) }}
  "#
}

//----
//- diagnostics
// Error: Expected client name, found Quote
//    ╭─[ 0:6:10 ]
//    │
//  6 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:6:10 ]
//    │
//  6 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:6:11 ]
//    │
//  6 │   client "openai/gpt-4o"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:6:17 ]
//    │
//  6 │   client "openai/gpt-4o"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:6:18 ]
//    │
//  6 │   client "openai/gpt-4o"
//    │                  ───┬──  
//    │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:6:24 ]
//    │
//  6 │   client "openai/gpt-4o"
//    │                        ┬  
//    │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
