// The runner's within-turn repair: a Complete turn whose candidate does not
// parse triggers a committed re-ask; the journal records every attempt.

test "the runner re-asks within the step and records every attempt" {
    let scripted = ScriptedClient
        .new(
            [
                // attempt 1: the model chats instead of answering
                root.ai.ModelTurn {
                    content: [root.ai.Text { text: "Could you tell me your budget first?" }],
                    stop_reason: root.ai.StopReason.Complete,
                    usage: root.ai.Usage {
                        input_tokens: 7,
                        output_tokens: 3,
                        cached_input_tokens: null,
                        reasoning_tokens: null,
                    },
                },
                // attempt 2 (ephemeral re-ask): a valid final answer
                root.ai.ModelTurn {
                    content: [root.ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 5.0}` }],
                    stop_reason: root.ai.StopReason.Complete,
                    usage: root.ai.Usage {
                        input_tokens: 9,
                        output_tokens: 4,
                        cached_input_tokens: null,
                        reasoning_tokens: null,
                    },
                },
            ],
        );
    let spec = plan_trip_spec("anywhere", scripted);
    let result = root.ai.Agent<Itinerary>.new().run(spec);
    assert.equal(result.value.total_cost_usd, 5.0);
    // one step, two invocations; usage counts BOTH attempts
    assert.equal(scripted.received().length(), 2);
    assert.equal(result.usage.input_tokens, 16);
    // the journal is the complete record: the failed attempt AND the
    // correction request are events, alongside the accepted turn
    let chatty = result.journal.entries().some((e: root.ai.Event) -> {
        match (e) {
            let a: root.ai.AssistantMessage => {
                a.content.some((b: root.ai.ContentBlock) -> {
                    match (b) {
                        let t: root.ai.Text => t.text.includes("budget"),
                        _ => false,
                    }
                })
            },
            _ => false,
        }
    });
    assert.is_true(chatty);
    let feedback = result.journal.entries().some((e: root.ai.Event) -> {
        match (e) {
            let u: root.ai.UserMessage => true,
            _ => false,
        }
    });
    assert.is_true(feedback);
    // a repair attempt is not a step: the run still fits max_steps = 1
    let one_step = root.ai.Agent<Itinerary>.new(max_steps = 1).run(plan_trip_spec("anywhere", scripted_two_repair()));
    assert.equal(one_step.value.total_cost_usd, 5.0)
}

function scripted_two_repair() -> ScriptedClient {
    ScriptedClient
        .new(
            [
                root.ai.ModelTurn {
                    content: [root.ai.Text { text: "let me think about that" }],
                    stop_reason: root.ai.StopReason.Complete,
                    usage: null,
                },
                root.ai.ModelTurn {
                    content: [root.ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 5.0}` }],
                    stop_reason: root.ai.StopReason.Complete,
                    usage: null,
                },
            ],
        )
}
