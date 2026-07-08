// scenario 01 — single-turn text: ../../../../../llm-provider/ideas/scenarios/01-single-turn-text/README.md
// The plain desugared call over declared, swapped, and legacy clients.
// Live tier = openai_live_call / anthropic_live_call (Rust suites).

client<llm> DeadS01 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function AskOnce(q: string) -> string {
  client DeadS01
  prompt #"Answer briefly: {{ q }}"#
}

test "01: the plain call routes through a swapped-in provider" {
    let r = AskOnce("ping?", client = EchoProvider { reply: "pong" }) catch (e) {
        _ => "call failed",
    };
    assert.equal(r, "pong")
}

test "01: a capability-less client is a typed Unsupported" {
    let r = AskOnce("ping?", client = NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}
