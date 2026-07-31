// D6a–c: one declaration + one override with merge-by-default semantics,
// run-local registry snapshots (no cross-run contamination), and the typed
// tool error channel.

test "D6a: runner tools MERGE with task tools — a task handoff tool survives" {
    //# The audit's exact repro: a handoff tool on the task used to silently
    //# disappear when the runner supplied its own roster, flipping Handoff
    //# into Done. Merge-by-default keeps it.
    let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider(
        [
            ai.tools.ToolCall { id: "h-1", name: "lookup_account", args: { "customer_id": "C-1" } },
        ],
    )).with_tools([handoff_tool]);
    let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]));
    match (outcome) {
        let handoff: ai.Handoff => assert.equal(handoff.call.name, "lookup_account"),
        _ => assert.is_true(false),
    }
}

test "D6a: replace_tools = true makes the runner roster the whole roster" {
    let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider(
        [
            ai.tools.ToolCall { id: "h-1", name: "lookup_account", args: { "customer_id": "C-1" } },
        ],
    )).with_tools([handoff_tool]);
    //# With replacement explicit, the dropped handoff tool becomes an
    //# unknown-tool error result and the run completes — the old silent
    //# behavior, now opted into by name.
    let outcome = task.run(
        runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge], replace_tools = true),
    );
    match (outcome) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
}

test "D6a: tools and tool_registry are mutually exclusive" {
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([]));
    let kind = task.run(
        runner = ai.run.Agent<Resolution>.new(
            tools = [search_knowledge],
            tool_registry = ai.tools.ToolRegistry.new([search_knowledge]),
        ),
    ).to_string() catch (e) {
        let invalid: ai.InvalidRequest => {
            assert.is_true(invalid.detail.includes("not both"));
            "refused"
        },
        _ => "wrong-channel",
    };
    assert.equal(kind, "refused")
}

test "D6b: a supplied registry is live shared state; a tools list is a run-scoped value" {
    //# Registry path: the loop reads AND writes the caller's object — the
    //# documented contract for dynamic rosters. A StepPlan replacement is
    //# visible to the caller after the run.
    let registry = ai.tools.ToolRegistry.new([search_knowledge]);
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()]));
    let outcome = task.run(
        runner = ai.run.Agent<Resolution>.new(
            tool_registry = registry,
            prepare_step = (context: ai.tools.StepContext) -> ai.tools.StepPlan {
                ai.tools.StepPlan {
                    tools: [ai.tools.normalize_tool(lookup_account)],
                    provider: null,
                    stop: null,
                }
            },
        ),
    );
    assert.equal(registry.snapshot().length(), 1);
    assert.equal(registry.snapshot()[0].name, "lookup_account")

    //# Value path: a plain tools list never leaks loop changes — running
    //# the same configured Agent twice sees the same roster both times.
    let value_task = ResolveTicketWithTools@task(sample_ticket()).with_provider(fake_tool_provider([one_knowledge_call()]));
    let runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]);
    match (value_task.run(runner = runner)) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
    match (value_task.run(runner = runner)) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
}

function _rate_limited_tool(query: string) -> json throws ai.RateLimited {
    throw ai.RateLimited { provider: "tool-backend", retry_after_ms: 5 }
}

test "D6c: ToolError carries the typed cause" {
    let failing = ai.tools.tool(_rate_limited_tool);
    let result = ai.tools.invoke_tool(
        failing,
        ai.tools.ToolCall { id: "c-1", name: "_rate_limited_tool", args: { "query": "q" } },
    );
    match (result) {
        let failed: ai.tools.ToolError => {
            match (failed.cause) {
                let limited: ai.RateLimited => assert.equal(limited.retry_after_ms, 5),
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    }
}

function _sub_agent_tool(question: string) -> string
    throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError {
    //# An LLM function as a tool body — the flagship agent-as-tool
    //# composition the old Throws pin rejected at compile time.
    ResolveTicket(sample_ticket()).reply
}

test "D6c: an agent-as-tool handler is accepted and runs" {
    let sub = ai.tools.tool(_sub_agent_tool);
    let result = ai.tools.invoke_tool(
        sub,
        ai.tools.ToolCall { id: "c-2", name: "_sub_agent_tool", args: { "question": "status?" } },
    );
    match (result) {
        let ok: ai.tools.ToolOk => {},
        let failed: ai.tools.ToolError => assert.is_true(false),
    }
}
