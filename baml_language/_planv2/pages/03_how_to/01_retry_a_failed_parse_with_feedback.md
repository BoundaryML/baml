# Retry a failed parse with feedback

The default runner already re-asks the model when a final candidate
fails to parse
(`../02_guides/02_specs_and_runners/02_the_default_runner.md`). Write
the loop yourself when you want your own feedback wording or attempt
policy. The pieces are public primitives: `spec.prompt()`,
`Journal.with`, `UserMessage`, and `client.invoke`.

```baml
function turn_with_feedback<Out>(
    client: Client,
    spec: FunctionSpec<Out>,
    j: Journal,
    attempts: int,
    extra: Event[] = [],    // feedback for this attempt; rendered, never committed
) -> ModelTurn throws Failure | baml.errors.UnknownError {
    let turn = client.invoke(ModelTurnInput {
        prompt: spec.prompt(),
        journal: j.with(extra),    // an extended copy for rendering; j itself is unchanged
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    });
    let _ = baml.sap.parse<Out>(terminal_text(turn)) catch_all (e) {
        let p: ParseFailed => {
            if (attempts > 1) {
                // show the model its failed reply, then ask again with one less attempt
                return turn_with_feedback<Out>(client, spec, j, attempts - 1, extra = [
                    AssistantMessage { content: turn.content, client_id: client.id() },
                    UserMessage {
                        content: "The reply did not match the required schema. Answer again with only the corrected value.",
                    },
                ]);
            } else {
                throw e    // out of attempts
            }
        },
        _ => throw e,    // not a parse failure; propagate untouched
    };
    turn    // it parsed; the caller commits this turn and nothing else
}
```

`j.with(extra)` renders the failed turn and the feedback for the next
attempt while leaving the underlying journal unchanged, so failed
attempts leave no record. The caller commits only the returned turn
and parses it as usual. A runner built around this helper upholds the
usual rules (`../02_guides/02_specs_and_runners/03_writing_a_runner.md`).
