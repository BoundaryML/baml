// `${role(...)}` markers in an LLM function prompt produce structural
// PromptAst messages, just like the standalone `prompt` tag.
function Foo() -> string {
    client: "openai/gpt-4o"
    prompt: `
        You are a helpful assistant.
        ${ctx.output_format()}

        ${role("assistant")}
        Hi
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
