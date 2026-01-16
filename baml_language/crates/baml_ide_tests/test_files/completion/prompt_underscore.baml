client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4"
    }
}

function Chat() -> string {
    client GPT4
    prompt #"
        _.<[CURSOR]
    "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (testing role helper completions)
// SHOULD_CONTAIN: role("system"), role("user"), role("assistant")
