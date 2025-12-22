class Message {
  role "user" | "assistant" | string
  message string
}

function Bot(convo: Message[]) -> string {
  client "openai/gpt-4o"
  prompt #"
    You are a helpful assistant.
    {{ ctx.output_format }}

    {% for m in convo %}
    {{ _.role(m.role) }}
    {{ m.message }}
    {% endfor %}
  "#
}

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
