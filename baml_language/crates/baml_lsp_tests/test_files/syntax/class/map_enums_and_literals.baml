enum MapKey {
  A
  B
  C
}

class Fields {
  e map<MapKey, string>
  l1 map<"literal", string>
  l2 map<"one" | "two" | ("three" | "four"), string>
}

function InOutEnumKey(i1: map<MapKey, string>, i2: map<MapKey, string>) -> map<MapKey, string> {
  client "openai/gpt-4o"
  prompt #"
    Merge these: {{i1}} {{i2}}

    {{ ctx.output_format }}
  "#
}

function InOutLiteralStringUnionMapKey(
  i1: map<"one" | "two" | ("three" | "four"), string>, 
  i2: map<"one" | "two" | ("three" | "four"), string>
) -> map<"one" | "two" | ("three" | "four"), string> {
  client "openai/gpt-4o"
  prompt #"
    Merge these:
    
    {{i1}}
    
    {{i2}}

    {{ ctx.output_format }}
  "#
}

//----
//- diagnostics
// Error: Expected client name, found Quote
//     ╭─[ 0:14:10 ]
//     │
//  14 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:14:10 ]
//     │
//  14 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:14:11 ]
//     │
//  14 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:14:17 ]
//     │
//  14 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:14:18 ]
//     │
//  14 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:14:24 ]
//     │
//  14 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:26:10 ]
//     │
//  26 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:26:10 ]
//     │
//  26 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:26:11 ]
//     │
//  26 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:26:17 ]
//     │
//  26 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:26:18 ]
//     │
//  26 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:26:24 ]
//     │
//  26 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
