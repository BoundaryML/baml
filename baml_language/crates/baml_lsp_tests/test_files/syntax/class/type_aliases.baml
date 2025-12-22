type Primitive = int | string | bool | float

type List = string[]

type Graph = map<string, string[]>

type Combination = Primitive | List | Graph

// Alias with attrs.
type Currency = int @check(gt_ten, {{ this > 10 }})
type Amount = Currency @assert({{ this > 0 }})

// Should be allowed.
type MultipleAttrs = int @assert({{ this > 0 }}) @check(gt_ten, {{ this > 10 }})

enum SomeEnum {
    A
    B
    C
}

type EnumAlias = SomeEnum

class TypeAliasPointsToEnum {
    enm EnumAlias
}

class MergeAttrs {
    amount Amount @description("In USD")
}

function PrimitiveAlias(p: Primitive) -> Primitive {
    client "openai/gpt-4o"
    prompt #"
      Return the given value back: {{ p }}
    "#
}

function MapAlias(m: Graph) -> Graph {
    client "openai/gpt-4o"
    prompt #"
      Return the given Graph back:

      {{ m }}

      {{ ctx.output_format }}
    "#
}

function NestedAlias(c: Combination) -> Combination {
    client "openai/gpt-4o"
    prompt #"
      Return the given value back:

      {{ c }}

      {{ ctx.output_format }}
    "#
}

function MergeAliasAttributes(money: int) -> MergeAttrs {
    client "openai/gpt-4o"
    prompt #"
      Return the given integer in the specified format:

      {{ money }}

      {{ ctx.output_format }}
    "#
}

//----
//- diagnostics
// Error: Expected client name, found Quote
//     ╭─[ 0:33:12 ]
//     │
//  33 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:33:12 ]
//     │
//  33 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:33:13 ]
//     │
//  33 │     client "openai/gpt-4o"
//     │             ───┬──  
//     │                ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:33:19 ]
//     │
//  33 │     client "openai/gpt-4o"
//     │                   ┬  
//     │                   ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:33:20 ]
//     │
//  33 │     client "openai/gpt-4o"
//     │                    ───┬──  
//     │                       ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:33:26 ]
//     │
//  33 │     client "openai/gpt-4o"
//     │                          ┬  
//     │                          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:40:12 ]
//     │
//  40 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:40:12 ]
//     │
//  40 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:40:13 ]
//     │
//  40 │     client "openai/gpt-4o"
//     │             ───┬──  
//     │                ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:40:19 ]
//     │
//  40 │     client "openai/gpt-4o"
//     │                   ┬  
//     │                   ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:40:20 ]
//     │
//  40 │     client "openai/gpt-4o"
//     │                    ───┬──  
//     │                       ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:40:26 ]
//     │
//  40 │     client "openai/gpt-4o"
//     │                          ┬  
//     │                          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:51:12 ]
//     │
//  51 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:51:12 ]
//     │
//  51 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:51:13 ]
//     │
//  51 │     client "openai/gpt-4o"
//     │             ───┬──  
//     │                ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:51:19 ]
//     │
//  51 │     client "openai/gpt-4o"
//     │                   ┬  
//     │                   ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:51:20 ]
//     │
//  51 │     client "openai/gpt-4o"
//     │                    ───┬──  
//     │                       ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:51:26 ]
//     │
//  51 │     client "openai/gpt-4o"
//     │                          ┬  
//     │                          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected client name, found Quote
//     ╭─[ 0:62:12 ]
//     │
//  62 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected client name, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:62:12 ]
//     │
//  62 │     client "openai/gpt-4o"
//     │            ┬  
//     │            ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     ╭─[ 0:62:13 ]
//     │
//  62 │     client "openai/gpt-4o"
//     │             ───┬──  
//     │                ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     ╭─[ 0:62:19 ]
//     │
//  62 │     client "openai/gpt-4o"
//     │                   ┬  
//     │                   ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     ╭─[ 0:62:20 ]
//     │
//  62 │     client "openai/gpt-4o"
//     │                    ───┬──  
//     │                       ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4o', found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     ╭─[ 0:62:26 ]
//     │
//  62 │     client "openai/gpt-4o"
//     │                          ┬  
//     │                          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
