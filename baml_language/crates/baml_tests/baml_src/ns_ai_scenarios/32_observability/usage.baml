// scenario 32 — observability: ../../../../../llm-provider/ideas/scenarios/32-observability/README.md
// The value+sidecar surface through the desugared companion: `Foo$with`
// projects ResponseMeta alongside the value (DCP §1.3).

client<llm> DeadObs {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function Summarize(q: string) -> string {
  client DeadObs
  prompt #"Summarize: {{ q }}"#
}

test "Foo$with projects metadata alongside the value" {
    let r = Summarize$with(
        "hello",
        (m: baml.ai.ResponseMeta) -> string { m.finish_reason() },
        client = EchoProvider { reply: "summary!" },
    ) catch (e) {
        _ => baml.ai.CallResult<string, string> { value: "with failed", meta: "" },
    };
    assert.equal(r.value, "summary!")
    assert.equal(r.meta, "unknown")
}

test "Foo$with usage projection over the echo fake is zeroed" {
    let r = Summarize$with(
        "hello",
        (m: baml.ai.ResponseMeta) -> int { m.usage().output_tokens },
        client = EchoProvider { reply: "s" },
    ) catch (e) {
        _ => baml.ai.CallResult<string, int> { value: "with failed", meta: -1 },
    };
    assert.equal(r.meta, 0)
}

test "Foo$with on a capability-less client is a typed Unsupported" {
    let r = Summarize$with(
        "hello",
        (m: baml.ai.ResponseMeta) -> string { m.finish_reason() },
        client = NoCapProvider {},
    ) catch (e) {
        let u: baml.errors.Unsupported => baml.ai.CallResult<string, string> { value: "unsupported", meta: "" },
        _ => baml.ai.CallResult<string, string> { value: "wrong error", meta: "" },
    };
    assert.equal(r.value, "unsupported")
}
