// D8 contract: a direct generated call shares task.complete()'s unwrap. A
// stop state throws ai.IncompleteRun carrying the resumable outcome — and a
// generic ai.Failure catch arm must NOT absorb it.

function DirectCallHandsOff(ticket: SupportTicket) -> Resolution {
    provider: fake_tool_provider(
        [
            ai.tools.ToolCall {
                id: "handoff-direct-1",
                name: "lookup_account",
                args: { "customer_id": "C-1" },
            },
        ],
    )
    prompt: `
        Resolve ticket ${ticket.id}.
        ${ctx.output_format}
    `
    tools: [ai.tools.tool(lookup_account).as_handoff()]
}

test "D8: a direct call that stops throws IncompleteRun, not a bare error" {
    let outcome_kind = DirectCallHandsOff(sample_ticket()).category catch (e) {
        //# The Failure arm comes FIRST on purpose: IncompleteRun no longer
        //# implements ai.Failure, so this arm must not swallow the stop.
        let failure: ai.Failure => "wrongly-absorbed-as-failure",
        let incomplete: ai.IncompleteRun => {
            //# The stop is lossless: the committed conversation and the
            //# exact pending handoff call ride along.
            match (incomplete.outcome) {
                let handoff: ai.Handoff => {
                    assert.equal(handoff.call.name, "lookup_account");
                    assert.equal(incomplete.steps_taken(), handoff.steps_taken);
                    "incomplete-handoff"
                },
                _ => "incomplete-other",
            }
        },
        let unknown: baml.errors.UnknownError => "bare-unknown-error",
    };
    assert.equal(outcome_kind, "incomplete-handoff")
}

test "D8: the carried outcome resumes through a session" {
    let task = DirectCallHandsOff@task(sample_ticket());
    let value = task.complete() catch (e) {
        let incomplete: ai.IncompleteRun => {
            let session = ai.run.AgentSession<Resolution>.of(task, incomplete.outcome);
            match (incomplete.outcome) {
                let handoff: ai.Handoff => {
                    let continued = session.submit_tool_results([
                        ai.tools.ToolOk.of(handoff.call, { "status": "resolved by application" }),
                    ]);
                    match (continued) {
                        let done: ai.Done<Resolution> => done.value,
                        _ => baml.sys.panic("continuation did not finish"),
                    }
                },
                _ => baml.sys.panic("expected a handoff stop"),
            }
        },
    };
    assert.equal(value.category, "billing")
}
