// This file contains invalid MDX header usage patterns

function HeaderInPrompt() -> string {
    client GPT4
    prompt #"
        //# This is inside a prompt
        It should be treated as literal text, not a header
    "#
}

function MixedOnSameLine() -> string {
    let x = "hello" ## Inline Header // Should fail
    x
}

function HeaderWithoutSpace() -> string {
    //#No Space After Hash // Should potentially fail depending on grammar rules
    let x = "test"
    x
}

function EmptyHeader() -> string {
    //#
    // Empty header title - this should fail
    let x = "test"
    //##
    // Another empty header
    x
}

// Test headers in places they shouldn't parse
function HeaderInExpression() -> string {
    let x = "start" + # Middle Header // Should fail
        "end"
    x
}

function HeaderInStringLiteral() -> string {
    let x = "This string contains # Header Text"
    let y = #"
        Raw string with # Header
        Should not be parsed
    "#
    x + y
}

// LLM function (not expression function) - headers should not be allowed
function LLMFunction(input: string) -> string {
    client GPT4
    //# Section Header
    // Should fail - only allowed in expression functions
    prompt #"Process: {{ input }}"#
}

client<llm> GPT4 {
    provider openai
    options {
        model gpt-4
        api_key env.OPENAI_API_KEY
    }
}

//----
//- diagnostics
// (expect errors for invalid header patterns)
// error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:21 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                     ┬  
//     │                     ╰── Expected expression, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:22 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                      ┬  
//     │                      ╰── Expected expression, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:33:23 ]
//     │
//  33 │     let x = "start" + # Middle Header // Should fail
//     │                       ┬  
//     │                       ╰── Expected expression, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: Header comments (//#) are not allowed inside LLM functions
//     ╭─[ headers_invalid.baml:50:5 ]
//     │
//  50 │     //# Section Header
//     │     ─────────┬────────  
//     │              ╰────────── Header comments (//#) are not allowed inside LLM functions
//     │ 
//     │ Note: Error code: E0010
// ────╯
// error: unresolved name: Inline
//     ╭─[ headers_invalid.baml:12:24 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                        ───┬──  
//     │                           ╰──── unresolved name: Inline
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: Header
//     ╭─[ headers_invalid.baml:12:31 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                               ───┬──  
//     │                                  ╰──── unresolved name: Header
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: Middle
//     ╭─[ headers_invalid.baml:33:25 ]
//     │
//  33 │     let x = "start" + # Middle Header // Should fail
//     │                         ───┬──  
//     │                            ╰──── unresolved name: Middle
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: Header
//     ╭─[ headers_invalid.baml:33:32 ]
//     │
//  33 │     let x = "start" + # Middle Header // Should fail
//     │                                ───┬──  
//     │                                   ╰──── unresolved name: Header
//     │ 
//     │ Note: Error code: E0003
// ────╯
