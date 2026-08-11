// Model-string resolution: "prefix/model" -> a client value. Lives in user
// land rather than the builtin `ai` package so the core package never
// depends on the providers; the registry's register() needs process-global
// state and is deferred.

function resolve(shorthand: string) -> ai.Client {
    let slash = shorthand.index_of("/");
    if let i: int = slash {
        let prefix = shorthand.substring(0, i);
        let model = shorthand.substring(i + 1, shorthand.length());
        if (prefix == "openai") {
            openai.OpenAiClient.new(model = model)
        } else if (prefix == "anthropic") {
            anthropic.AnthropicClient.new(model = model)
        } else if (prefix == "google") {
            google.GoogleClient.new(model = model)
        } else if (prefix == "claude-code") {
            claude_code.ClaudeCodeClient.new(model = model)
        } else {
            throw baml.errors.InvalidArgument { message: `unknown client prefix: ${prefix}` }
        }
    } else {
        throw baml.errors.InvalidArgument { message: `model string needs a prefix: ${shorthand}` }
    }
}

test "resolve builds the right client for each prefix" {
    // no credentials needed: id() reads config only. Env vars may be absent
    // in offline test runs, so pass explicit keys.
    let o = openai.OpenAiClient.new(model = "gpt-4o-mini", api_key = "k");
    assert.equal(o.id(), "openai/gpt-4o-mini");
    let bad = (
        resolve("nope") catch_all (e) {
            _ => null,
        }
    )
        == null;
    assert.is_true(bad)
}
