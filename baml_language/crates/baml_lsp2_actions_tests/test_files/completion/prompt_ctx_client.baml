// `ctx.client` no longer exists in prompts; the only ctx member is
// `output_format`, so this fixture now cursors that surface instead.
client GPT4 = openai.ResponsesClient.new(model = "gpt-4");

function Chat() -> string {
    client: GPT4
    prompt: `
        ${ctx.output_form<[CURSOR]at}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (new pipeline doesn't offer completions inside prompt interpolations)
