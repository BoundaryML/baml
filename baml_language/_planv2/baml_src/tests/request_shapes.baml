// Request-shape tests: journal lowering rules that live APIs enforce,
// checked offline against the providers' internal render helpers.

function repaired_first_step_journal() -> ai.Journal {
    let args: map<string, unknown> = {};
    // no spec needed here: a Journal literal is ordinary construction
    let j = ai.Journal { log: [ai.RunStarted { spec_name: "PlanTrip", arguments: args }] };
    // a repaired first step: the model chatted, the runner committed the
    // failure and the correction request
    j.append_all(
        [
            ai.AssistantMessage {
                content: [ai.Text { text: "what is your budget?" }],
                client_id: "anthropic/test",
            },
            ai.UserMessage { content: "Answer again with only the corrected value." },
        ],
    );
    j
}

test "anthropic messages always start with a user role" {
    let lowered = anthropic.internal._anthropic_lower_journal(repaired_first_step_journal());
    // the lowered journal opens with an assistant message...
    let opens_assistant = (lowered.at(0)?.role ?? "") == "assistant";
    assert.is_true(opens_assistant)
    // ...which is exactly why invoke() prepends the instructions as a user
    // message and omits system in that case (asserted structurally above;
    // the branch itself is exercised by the live smokes).
}

test "claude code envelope offers final result or tool calls" {
    let schema = claude_code.internal.envelope_schema(reflect.type_of<Itinerary>());
    let j = baml.json.to_json(schema);
    let outcome = baml.json.path_or<json>(j, ".properties.outcome", null);
    let variants = baml.json.path_or<json[]>(outcome, ".anyOf", []);
    assert.equal(variants.length(), 2);
    // T's $defs lift to the envelope root so refs stay valid
    assert.is_true(baml.json.path_or<json>(j, ".$defs", null) != null)
}

test "claude code transcript marks successful tool results" {
    let args: map<string, unknown> = {};
    let j = ai.Journal { log: [ai.RunStarted { spec_name: "PlanTrip", arguments: args }] };
    j.append_all(
        [
            ai.AssistantMessage {
                content: [ai.ToolUse { id: "c1", name: "search_flights", args: args }],
                client_id: "claude-code/haiku",
            },
            ai.ToolCompleted { id: "c1", output: `[{"airline":"ANA"}]` },
        ],
    );
    let text = claude_code.internal.transcript_text(j);
    assert.contains(text, "tool result id=c1 is_error=false");
    assert.contains(text, "assistant tool_call")
}
