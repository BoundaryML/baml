// This file contains invalid MDX header usage patterns

function HeaderInPrompt() -> string {
    client GPT4
    prompt #"
        # This is inside a prompt
        It should be treated as literal text, not a header
    "#
}

fn MixedOnSameLine() -> string {
    let x = "hello" ## Inline Header // Should fail
    x
}

fn HeaderWithoutSpace() -> string {
    #No Space After Hash // Should potentially fail depending on grammar rules
    let x = "test"
    x
}

fn EmptyHeader() -> string {
    # 
    // Empty header title - this should fail
    let x = "test"
    ##
    // Another empty header
    x
}

// Test headers in places they shouldn't parse
fn HeaderInExpression() -> string {
    let x = "start" + # Middle Header // Should fail
        "end"
    x
}

fn HeaderInStringLiteral() -> string {
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
    # Section Header
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

