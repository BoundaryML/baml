// Test LLM functions (these should work with client/prompt)
function GenerateTitle(transcript: string) -> string {
    client GPT4
    prompt `Generate a title from: ${transcript}`
}

function SummarizeVideo(transcript: string) -> string {
    client GPT4
    prompt `Summarize: ${transcript}`
}

function CombineOutputs(email: string, x: string, linkedin: string) -> string {
    client GPT4
    prompt `Combine all outputs`
}

client GPT4 = openai.ResponsesClient.new(
    model = "gpt-4",
    api_key = env.OPENAI_API_KEY,
);

//----
//- diagnostics
// <no-diagnostics-expected>
