// A type alias to a type with a check.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

// That alias is used as a parameter.
function AliasWithMultipleAttrs(money: MultipleAttrs) -> MultipleAttrs {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer without additional context:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

// error: Error validating: Types with checks are not allowed as function parameters.
//   -->  functions_v2/check_in_parameter_type_alias.baml:5
//    | 
//  4 | // That alias is used as a parameter.
//  5 | function AliasWithMultipleAttrs(money: MultipleAttrs) -> MultipleAttrs {
//    |

//----
//- diagnostics
// Error: Expected client name, found Quote
//    ╭─[ 0:6:12 ]
//    │
//  6 │     client "openai/gpt-4o"
//    │            ┬  
//    │            ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:6:12 ]
//    │
//  6 │     client "openai/gpt-4o"
//    │            ┬  
//    │            ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:6:13 ]
//    │
//  6 │     client "openai/gpt-4o"
//    │             ───┬──  
//    │                ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:6:19 ]
//    │
//  6 │     client "openai/gpt-4o"
//    │                   ┬  
//    │                   ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:6:20 ]
//    │
//  6 │     client "openai/gpt-4o"
//    │                    ───┬──  
//    │                       ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:6:26 ]
//    │
//  6 │     client "openai/gpt-4o"
//    │                          ┬  
//    │                          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
