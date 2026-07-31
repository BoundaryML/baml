// Hold a typed Task and its exact provider continuation as one value, then
// continue it by declared intent: `send` starts a new turn, `fork` branches
// two independent continuations from a shared prefix, and `ask` unwraps the
// typed value at call sites where anything short of Done is exceptional.
//
// baml run ai_scenarios.agent_session --from crates/baml_tests/baml_src_temp2
function agent_session() -> string {
    let provider = ai.testing.FakeToolProvider {
        provider_name: "agent-session-demo",
        tool_calls: [],
        final_output: `"ack"`,
        provider_tools: [],
    };
    let task = _SessionProbe@task().with_provider(provider);

    //# Any outcome pairs with its task to form a session
    let opening = task.run(runner = ai.run.Agent<string>.new());
    let base = ai.run.AgentSession<string>.of(task, opening);

    //# Fork first: each branch owns an independent copy of the exact prefix,
    //# then advances in place as it is continued. This is the EXACT pole —
    //# the portable counterpart is forking a MessageHistory by value
    //# (03_fork_a_conversation.baml); session.export()/import() cross
    //# between the two, always visibly
    let branch_a = base.fork();
    let branch_b = base.fork();
    let _ = branch_a.send(ai.ChatMessage.user("explore option a"));
    let _ = branch_b.send(ai.ChatMessage.user("explore option b"));

    //# A new turn continues the exact conversation; the prompt renders once.
    //# complete demands the finished value or throws IncompleteRun.
    let follow_up = base.complete(ai.ChatMessage.user("summarize what we agreed"));

    log.info(
        {
            "base_messages": base.conversation().messages().items().length(),
            "branch_a_messages": branch_a.conversation().messages().items().length(),
            "branch_b_messages": branch_b.conversation().messages().items().length(),
            "follow_up": follow_up,
        },
    );
    follow_up
}

function _SessionProbe() -> string {
    provider: ai.testing.fake_output_provider(`"ack"`)
    prompt: `
        You are a terse planning assistant. Answer the latest user message.
        ${ctx.output_format}
    `
}
