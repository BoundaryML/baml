// scenario 07 — reasoning (+ 08 enriched outputs): ../../../../../llm-provider/ideas/scenarios/07-reasoning/README.md
// ResponseMeta dimensions ride the $with companion; absent dimensions answer
// Unavailable — a value, not an error.

client<llm> DeadS07 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function ThinkHard(q: string) -> string {
  client DeadS07
  prompt #"{{ q }}"#
}

test "07: an absent reasoning dimension projects as Unavailable, not an error" {
    let r = ThinkHard$with(
        "why?",
        (m: baml.ai.ResponseMeta) -> string {
            match (m.reasoning()) {
                let u: baml.ai.Unavailable => "unavailable: " + u.reason,
                let s: string => s,
            }
        },
        client = EchoProvider { reply: "because" },
    ) catch (e) {
        _ => baml.ai.CallResult<string, string> { value: "failed", meta: "" },
    };
    assert.equal(r.value, "because")
    assert.is_true(r.meta.includes("unavailable"))
}

test "08: logprobs project the same way (enriched outputs are meta dimensions)" {
    let r = ThinkHard$with(
        "why?",
        (m: baml.ai.ResponseMeta) -> string {
            match (m.logprobs()) {
                let u: baml.ai.Unavailable => "no logprobs",
                let l: baml.ai.Logprob[] => "have " + l.length().to_string(),
            }
        },
        client = EchoProvider { reply: "b" },
    ) catch (e) {
        _ => baml.ai.CallResult<string, string> { value: "failed", meta: "" },
    };
    assert.equal(r.meta, "no logprobs")
}
