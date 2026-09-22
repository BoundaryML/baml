// docs:start all
// docs:start client
client Fast = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = env.OPENAI_API_KEY
);

// docs:end client

// docs:start summarize
function summarize(transcript: string) -> string {
    client: Fast
    prompt: `
        ${role("system")}
        Summarize this conversation in one sentence.
        ${role("user")}
        ${transcript}
    `
}

// docs:end summarize

function estimate_tokens(text: string, chars_per_token: int = 4) -> int {
    text.trim().length() / chars_per_token
}

function cost_cents(tokens: int, price_cents_per_mtok: int) -> int {
    tokens * price_cents_per_mtok / 1_000_000
}

// docs:start main
function main() -> string {
    let transcript = "user: Where is my order?\nagent: It shipped on Tuesday.";
    log.info(estimate_tokens(transcript));
    summarize@parse("The user asked where their order was; it shipped Tuesday.")
}
// docs:end main
// docs:end all

test "chapter example" {
    assert.equal(main(), "The user asked where their order was; it shipped Tuesday.")
}
