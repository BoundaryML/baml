function Foo() -> string {
    client "openai/gpt-4o"
    prompt #"
        You are a helpful assistant.
        {{ ctx.output_format }}

        {{ _.role("assistant", cache_control={"type": "ephemeral"}) }}
        Hi
    "#
}

//----
//- diagnostics
// Error: Expected client name, found Quote
//    ╭─[ 0:2:12 ]
//    │
//  2 │     client "openai/gpt-4o"
//    │            ┬  
//    │            ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:2:12 ]
//    │
//  2 │     client "openai/gpt-4o"
//    │            ┬  
//    │            ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:2:13 ]
//    │
//  2 │     client "openai/gpt-4o"
//    │             ───┬──  
//    │                ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:2:19 ]
//    │
//  2 │     client "openai/gpt-4o"
//    │                   ┬  
//    │                   ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:2:20 ]
//    │
//  2 │     client "openai/gpt-4o"
//    │                    ───┬──  
//    │                       ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:2:26 ]
//    │
//  2 │     client "openai/gpt-4o"
//    │                          ┬  
//    │                          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
