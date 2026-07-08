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

// ── Live tier (excluded by default; run via `-i "integ-test::"` with keys) ──
testset "integ-test-01" {
    test "01 live: single turn through a native OpenAI override" {
        let r = AskOnce("Reply with exactly the lowercase word: pong", client = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        }) catch (e) {
            _ => "live call failed",
        };
        assert.is_true(r.includes("pong") || r.includes("Pong"))
    }

    test "01 live: the same function through a native Anthropic override" {
        let r = AskOnce("Reply with exactly the lowercase word: pong", client = baml.ai.Anthropic {
            model: "claude-haiku-4-5-20251001",
            api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
            base_url: null,
            max_tokens: null,
            anthropic_version: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }) catch (e) {
            _ => "live call failed",
        };
        assert.is_true(r.includes("pong") || r.includes("Pong"))
    }
}
