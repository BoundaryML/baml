// o1-family model names construct clients cleanly alongside standard models.
// (The legacy `allowed_roles` option went away with `client<llm>` config
// blocks; roles are the client's wire concern now.)

client O1Client = openai.ResponsesClient.new(
    model = "o1",
    api_key = "sk-test",
);

client O1MiniClient = openai.ResponsesClient.new(
    model = "o1-mini",
    api_key = "sk-test",
);

client Gpt4oClient = openai.ResponsesClient.new(
    model = "gpt-4o",
    api_key = "sk-test",
);

function AskO1(question: string) -> string {
    client O1Client
    prompt `${question}`
}

function AskGpt4o(question: string) -> string {
    client Gpt4oClient
    prompt `${question}`
}
