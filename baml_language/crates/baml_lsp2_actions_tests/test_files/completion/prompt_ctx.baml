client GPT4 = openai.ResponsesClient.new(model = "gpt-4");

function Chat() -> string {
    client GPT4
    prompt `
        ${ctx.<[CURSOR]output_format}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (new pipeline doesn't offer completions inside prompt interpolations)
