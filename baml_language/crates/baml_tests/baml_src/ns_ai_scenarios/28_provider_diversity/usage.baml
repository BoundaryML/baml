// scenario 28 — provider diversity: ../../../../../llm-provider/ideas/scenarios/28-provider-diversity/README.md
// Routing is a plain function returning a Provider (the "Clean" scenario).
// Live routing test = provider_diversity_routing (Rust suite).

function s28_route(model: string) -> baml.ai.Provider throws never {
    if (model.includes("claude")) {
        return baml.ai.Anthropic {
            model: model,
            api_key: "k",
            base_url: null,
            max_tokens: null,
            anthropic_version: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        };
    }
    baml.ai.OpenAi { model: model, api_key: "k", base_url: null }
}

test "28: the router picks a provider per model, both are just Providers" {
    let a = s28_route("claude-haiku-4-5");
    let o = s28_route("gpt-5.4-mini");
    let both_http = match (a) {
        let h1: baml.ai.HttpProvider => match (o) {
            let h2: baml.ai.HttpProvider => true,
            _ => false,
        },
        _ => false,
    };
    assert.is_true(both_http)
}
