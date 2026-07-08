// scenario 35 — deployment shapes: ../../../../../llm-provider/ideas/scenarios/35-deployment-shapes/README.md
// Four homes collapse to config variance over one class.

function edge_client(gateway_url: string, token: string) -> baml.ai.Provider throws never {
    baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: token, base_url: gateway_url }
}

function browser_client(proxy_url: string) -> baml.ai.Provider throws never {
    // No API key in the browser: the key lives behind the proxy (base_url).
    baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: "", base_url: proxy_url }
}

test "35: deployment shapes are config variance, and all negotiate HttpProvider" {
    let shapes = [edge_client("https://gw", "tok"), browser_client("https://proxy")];
    let ok = 0;
    for (let p in shapes) {
        match (p) {
            let h: baml.ai.HttpProvider => {
                ok = ok + 1;
            },
            _ => {},
        }
    }
    assert.equal(ok, 2)
}
