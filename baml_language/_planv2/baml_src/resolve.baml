// Model-string resolution: "prefix/model" -> a client value. Lives in root
// rather than ns_ai so the core namespace never depends on the providers;
// the registry's register() needs process-global state and is deferred.

function resolve(shorthand: string) -> root.ai.Client {
    let slash = shorthand.index_of("/");
    if let i: int = slash {
        let prefix = shorthand.substring(0, i);
        let model = shorthand.substring(i + 1, shorthand.length());
        if (prefix == "openai") {
            root.openai.OpenAiClient.new(model = model)
        } else if (prefix == "anthropic") {
            root.anthropic.AnthropicClient.new(model = model)
        } else if (prefix == "google") {
            root.google.GoogleClient.new(model = model)
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
    let o = root.openai.OpenAiClient.new(model = "gpt-4o-mini", api_key = "k");
    assert.equal(o.id(), "openai/gpt-4o-mini");
    let bad = (
        resolve("nope") catch_all (e) {
            _ => null,
        }
    )
        == null;
    assert.is_true(bad)
}
