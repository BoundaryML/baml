function LLM(x: int) -> int {
    client: GPT4o
    prompt: `Return ${x} ${ctx.output_format}`
}

function UseLLM(x: int) -> int {
    LLM(x)
}

client GPT4o = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = env.OPENAI_API_KEY,
);

//----
//- diagnostics
// <no-diagnostics-expected>
