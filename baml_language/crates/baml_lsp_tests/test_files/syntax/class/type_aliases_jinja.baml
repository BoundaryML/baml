type ProjectId = int

function NormalAlias(pid: ProjectId) -> string {
  client "openai/gpt-4o"
  prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
}

type A = float
type B = A
type C = B

function LongerAlias(c: C) -> string {
  client "openai/gpt-4o"
  prompt #"{{ c.value }}"#
}

type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function RecursiveAliases(j: JsonValue) -> string {
  client "openai/gpt-4o"
  prompt #"{{ j.value }}"#
}

type I = J
type J = I

function InvalidAlias(i: I) -> string {
  client "openai/gpt-4o"
  prompt #"{{ i.value }}"#
}

// warning: 'pid' is a type alias ProjectId (resolves to int), expected class
//   -->  class/type_aliases_jinja.baml:5
//    | 
//  4 |   client "openai/gpt-4o"
//  5 |   prompt #"Pid: {{ pid.id }}. Generate a fake name for it."#
//    | 
// warning: 'c' is a type alias C (resolves to float), expected class
//   -->  class/type_aliases_jinja.baml:14
//    | 
// 13 |   client "openai/gpt-4o"
// 14 |   prompt #"{{ c.value }}"#
//    | 
// warning: 'j' is a recursive type alias JsonValue, expected class
//   -->  class/type_aliases_jinja.baml:23
//    | 
// 22 |   client "openai/gpt-4o"
// 23 |   prompt #"{{ j.value }}"#
//    | 
// warning: 'i' is a recursive type alias I, expected class
//   -->  class/type_aliases_jinja.baml:31
//    | 
// 30 |   client "openai/gpt-4o"
// 31 |   prompt #"{{ i.value }}"#
//    | 
// error: Error validating: These aliases form a dependency cycle: I -> J
//   -->  class/type_aliases_jinja.baml:26
//    | 
// 25 | 
// 26 | type I = J
//    |

//----
//- diagnostics
// Error: Expected client name, found Quote
//    ╭─[ 0:4:10 ]
//    │
//  4 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:4:10 ]
//    │
//  4 │   client "openai/gpt-4o"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:4:11 ]
//    │
//  4 │   client "openai/gpt-4o"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:4:17 ]
//    │
//  4 │   client "openai/gpt-4o"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    ╭─[ 0:4:18 ]
//    │
//  4 │   client "openai/gpt-4o"
//    │                  ───┬──  
//    │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:4:24 ]
//    │
//  4 │   client "openai/gpt-4o"
//    │                        ┬  
//    │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected client name, found Quote
//     ╭─[ 0:13:10 ]
//     │
//  13 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:13:10 ]
//     │
//  13 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:13:11 ]
//     │
//  13 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:13:17 ]
//     │
//  13 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:13:18 ]
//     │
//  13 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:13:24 ]
//     │
//  13 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:22:10 ]
//     │
//  22 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:22:10 ]
//     │
//  22 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:22:11 ]
//     │
//  22 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:22:17 ]
//     │
//  22 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:22:18 ]
//     │
//  22 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:22:24 ]
//     │
//  22 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:30:10 ]
//     │
//  30 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:30:10 ]
//     │
//  30 │   client "openai/gpt-4o"
//     │          ┬  
//     │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:30:11 ]
//     │
//  30 │   client "openai/gpt-4o"
//     │           ───┬──  
//     │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:30:17 ]
//     │
//  30 │   client "openai/gpt-4o"
//     │                 ┬  
//     │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:30:18 ]
//     │
//  30 │   client "openai/gpt-4o"
//     │                  ───┬──  
//     │                     ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:30:24 ]
//     │
//  30 │   client "openai/gpt-4o"
//     │                        ┬  
//     │                        ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
