client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4"
    }
}

function Chat() -> string {
    client GPT4
    prompt #"
        ctx.<[CURSOR]
    "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (testing ctx property completions)
// SHOULD_CONTAIN: output_format, client
