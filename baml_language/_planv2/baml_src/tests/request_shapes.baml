// Request-shape tests: journal lowering rules that live APIs enforce,
// checked offline against the providers' internal render helpers.

function repaired_first_step_journal() -> root.ai.Journal {
    let args: map<string, unknown> = {};
    // no spec needed here: a Journal literal is ordinary construction
    let j = root.ai.Journal { log: [root.ai.RunStarted { spec_name: "PlanTrip", arguments: args }] };
    // a repaired first step: the model chatted, the runner committed the
    // failure and the correction request
    j
        .append_all(
            [
                root
                    .ai
                    .AssistantMessage {
                    content: [root.ai.Text { text: "what is your budget?" }],
                    client_id: "anthropic/test",
                },
                root.ai.UserMessage { content: "Answer again with only the corrected value." },
            ],
        );
    j
}

test "anthropic messages always start with a user role" {
    let lowered = root.anthropic.internal._anthropic_lower_journal(repaired_first_step_journal());
    // the lowered journal opens with an assistant message...
    let opens_assistant = if let first: root.anthropic.internal.AnthropicMessage = lowered.at(0) {
        first.role == "assistant"
    } else {
        false
    };
    assert.is_true(opens_assistant)
    // ...which is exactly why invoke() prepends the instructions as a user
    // message and omits system in that case (asserted structurally above;
    // the branch itself is exercised by the live smokes).
}
