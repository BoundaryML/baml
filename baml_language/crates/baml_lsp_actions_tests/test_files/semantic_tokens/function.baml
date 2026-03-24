class Input {
    text string
}

function classify(input: Input) -> string {
    input.text
}

//----
//- diagnostics
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
//
//- semantic_tokens
// function.baml:1:1 (keyword) len=5 "class"
// function.baml:1:7 (class) len=5 "Input"
// function.baml:2:5 (property) len=4 "text"
// function.baml:2:10 (type) len=6 "string"
// function.baml:5:1 (keyword) len=8 "function"
// function.baml:5:10 (function) len=8 "classify"
// function.baml:5:19 (parameter) len=5 "input"
// function.baml:5:26 (namespace) len=5 "Input"
// function.baml:5:36 (type) len=6 "string"
