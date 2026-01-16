client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4"
    }
}

function Chat() -> string {
    client GPT4
    prompt #"
        ctx.client.<[CURSOR]
    "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (testing ctx.client property completions)
// SHOULD_CONTAIN: name, provider
