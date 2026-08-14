// `_.role(...)` and the `_` prompt namespace are gone; in a backtick prompt
// this text is literal, so no completions are expected here.
client GPT4 = openai.ResponsesClient.new(model = "gpt-4");

function Chat() -> string {
    client GPT4
    prompt `
        _.<[CURSOR]
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (new pipeline doesn't support prompt template completions)
