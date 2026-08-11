# Retry a failed parse with feedback

The default runner already re-asks the model when a final candidate
fails to parse
(`../02_guides/02_specs_and_runners/02_the_default_runner.md`). Write
the loop yourself when you want your own feedback wording or attempt
policy. The pieces are public primitives: `spec.prompt_template`,
`Journal.append_all`, `UserMessage`, and `client.invoke`.

Every attempt is committed. The journal is the complete record of the
run, so the re-ask sees the failed reply and the correction request
because they are in the journal, not because of any side channel.

```baml
function turn_with_feedback<Out>(
    c: Client,
    spec: FunctionSpec<Out>,
    j: Journal,
    attempts: int,
) -> ModelTurn {
    let turn = c.invoke(ModelTurnInput {
        prompt: spec.prompt_template,
        journal: j,
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    });
    let candidate = turn.terminal_text() ?? "";
    let _ = baml.sap.parse<Out>(candidate) catch_all (e) {
        _ => {
            if (attempts > 1) {
                // commit the failed reply and the correction request, then ask again
                j.append_all([
                    AssistantMessage { content: turn.content, client_id: c.id() },
                    UserMessage {
                        content: "The reply did not match the required schema. Answer again with only the corrected value.",
                    },
                ]);
                return turn_with_feedback<Out>(c, spec, j, attempts - 1);
            } else {
                throw e    // out of attempts; the journal already shows every try
            }
        },
    };
    turn    // it parsed; the caller commits this turn
}
```

A failed attempt stays visible to later turns, which is an ordinary
conversation: the model sees what it got wrong and what was asked of
it. The runner's built-in repair commits the same events; a repair
attempt does not consume a step
(`../02_guides/02_specs_and_runners/02_the_default_runner.md`).
