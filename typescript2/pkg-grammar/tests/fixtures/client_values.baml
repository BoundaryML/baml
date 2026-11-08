// Client values: one line, an initializer on the next line, and no semicolon.
client Fast = openai.ResponsesClient.new(model = "gpt-6-luna", api_key = env.OPENAI_API_KEY);

client Later =
    openai.ResponsesClient.new(model = "gpt-6-luna");

client Smart = anthropic.Client.new(model = "claude-sonnet-5-5")

client Reliable = ai.clients.Retry.new(
    inner = Later,
    max_attempts = 3,
);

function Ask(question: string) -> string {
    client: Reliable
    prompt: `Answer briefly: ${question}`
}

test "a companion may follow a space" {
    let request = Ask @spec("why").build_request(client = Smart);
    assert.not_null(request)
}
