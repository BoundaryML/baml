// Live coverage for the exact provider continuation used by the interactive
// memory-agent session. These tests make two model requests and verify that the
// second request sees information retained only by the first provider turn.

function _MemoryAgentLiveContinuation(
    provider: ai.Provider,
    nonce: string,
) -> string {
    provider: provider
    prompt: `
        Remember the exact nonce ${nonce}. Reply with only ACK.
        ${ctx.output_format}
    `
}

function _memory_agent_live_exact_follow_up(
    task: ai.Task<string>,
) -> string {
    let first = done_or_fail(
            task
                .run(
                    runner = ai
                        .run
                        .Agent<string>
                        .new(max_tool_calls_per_step = 0),
                ),
        );
    //# The session pairs the task with its exact provider continuation;
    //# complete demands the finished value or throws IncompleteRun
    let session = ai.run.AgentSession<string>.of(task, first);
    session.complete(ai.ChatMessage.user(
            "What exact nonce did I ask you to remember? Reply with only that nonce.",
        ),
        runner = ai.run.Agent<string>.new(max_tool_calls_per_step = 0),)
}

testset "integ-test-memory-agent-exact-continuation" {
    test "memory agent live OpenAI: fresh user message keeps Responses state" {
        let nonce = "cobalt-417";
        let answer = _memory_agent_live_exact_follow_up(
            _MemoryAgentLiveContinuation@task(root.ai_scenarios.memory.openai_provider(), nonce),
        );
        assert.contains(answer.to_lower_case(), nonce)
    }

    test "memory agent live Google AI: fresh user message keeps Gemini state" {
        let nonce = "saffron-928";
        let answer = _memory_agent_live_exact_follow_up(
            _MemoryAgentLiveContinuation@task(
                root.ai_scenarios.memory.live_google_provider(),
                nonce,
            ),
        );
        assert.contains(answer.to_lower_case(), nonce)
    }
}
