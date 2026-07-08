// Offline tests for the legacy bridge (DCP §1.1 Phase B): baml.llm.Client
// IS a baml.ai.Provider (out-of-body implements in ns_ai/core/legacy.baml).
// Wire-shape only — no network: build_request runs the full messages ->
// PromptAst -> specialize -> legacy request-builder pipeline and we inspect
// the resulting http.Request.

function legacy_openai() -> baml.llm.Client {
    baml.llm.Client {
        name: "openai/gpt-4o",
        client_type: baml.llm.ClientType.Primitive,
        sub_clients: [],
        retry: null,
        counter: 0,
    }
}

test "legacy bridge builds a chat-completions request from messages" {
    let bridge = legacy_openai();
    let messages = [
        baml.ai.ChatMessage.system("be terse"),
        baml.ai.ChatMessage.user("say pong"),
    ];
    let request = bridge.build_request<string>(messages) catch (e) {
        _ => baml.http.Request { method: "FAIL", url: "", headers: {}, body: "" },
    };
    assert.equal(request.method, "POST")
    assert.is_true(request.url.includes("chat/completions"))
    assert.is_true(request.body.includes("say pong"))
    assert.is_true(request.body.includes("be terse"))
    assert.is_true(request.body.includes("gpt-4o"))
}

test "legacy bridge is a Provider — negotiation sees HttpProvider" {
    let p: baml.ai.Provider = legacy_openai();
    let label = match (p) {
        let h: baml.ai.HttpProvider => "http-capable",
        _ => "not-http",
    };
    assert.equal(label, "http-capable")
}

test "legacy bridge refuses strategy configs with a typed Unsupported" {
    let strategy = baml.llm.Client {
        name: "resilient",
        client_type: baml.llm.ClientType.Fallback,
        sub_clients: [legacy_openai()],
        retry: null,
        counter: 0,
    };
    let outcome = strategy.build_request<string>([baml.ai.ChatMessage.user("x")]) catch (e) {
        let u: baml.errors.Unsupported => baml.http.Request { method: "UNSUPPORTED", url: "", headers: {}, body: "" },
        _ => baml.http.Request { method: "OTHER", url: "", headers: {}, body: "" },
    };
    assert.equal(outcome.method, "UNSUPPORTED")
}
