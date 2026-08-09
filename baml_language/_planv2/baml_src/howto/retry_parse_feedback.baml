// How-to: retry a failed parse with feedback, from public primitives only.
// Every attempt is committed: the journal is the complete record, and the
// re-ask sees the failure because it IS in the journal.
// Mirrors _planv2/pages/03_how_to/01_retry_a_failed_parse_with_feedback.md.

function turn_with_feedback<Out>(
    c: ai.Client,
    spec: ai.FunctionSpec<Out>,
    j: ai.Journal,
    attempts: int,
) -> ai.ModelTurn {
    let turn = c.invoke(
        ai.ModelTurnInput {
            prompt: spec.prompt(),
            journal: j,
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        },
    );
    let candidate = turn.terminal_text() ?? "";
    let _ = baml.sap.parse<Out>(candidate) catch_all (e) {
        _ => {
            if (attempts > 1) {
                // commit the failed reply and the correction request, then ask again
                j.append_all(
                    [
                        ai.AssistantMessage { content: turn.content, client_id: c.id() },
                        ai.UserMessage {
                            content: "The reply did not match the required schema. Answer again with only the corrected value.",
                        },
                    ],
                );
                return turn_with_feedback<Out>(c, spec, j, attempts - 1);
            } else {
                throw e // out of attempts; the journal already shows every try
            }
        },
    };
    turn // it parsed; the caller commits this turn
}

test "parse feedback re-asks and the journal records every attempt" {
    let scripted = ScriptedClient.new(
        [
            // attempt 1: garbage that cannot parse as Itinerary
            ai.ModelTurn {
                content: [ai.Text { text: "sorry, here is prose instead of JSON" }],
                stop_reason: ai.StopReason.Complete,
                usage: null,
            },
            // attempt 2: a valid final answer
            ai.ModelTurn {
                content: [ai.Text { text: `{"flights": [], "hotels": [], "total_cost_usd": 0.0}` }],
                stop_reason: ai.StopReason.Complete,
                usage: null,
            },
        ],
    );
    let spec = PlanTrip@spec(trip_request = "anywhere");
    let j = ai.Journal.new(spec);
    let turn = turn_with_feedback<Itinerary>(scripted, spec, j, 3);
    assert.is_true(turn.terminal_text() != null);
    // two invocations happened; the second saw the committed feedback
    assert.equal(scripted.received().length(), 2);
    let second_saw_feedback = scripted.received().at(1)?.journal?.entries()?.some((e: ai.Event) -> {
        match (e) {
            let u: ai.UserMessage => true,
            _ => false,
        }
    })
        ?? false;
    assert.is_true(second_saw_feedback);
    // the journal is the complete record: RunStarted, the failed reply, the feedback
    assert.equal(j.entries().length(), 3)
}
