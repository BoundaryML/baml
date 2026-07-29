function _messages_complete<T>(
    provider: root.anthropic.Messages,
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
    //# Use one generation when the task has no application tools
    if (task.tools.length() == 0) {
        return provider.generate<T>(task.with_provider(provider));
    }

    // Direct Completion<T> keeps its implicit tool loop finite. Callers that
    // need another limit, handoffs, callbacks, or resumable stops use Agent.
    let max_steps = 32;
    let tools = task.tools;
    //# Begin the Anthropic Messages conversation
    let conversation = provider.begin<T>(task.with_provider(provider));
    let usage = root.ai.Usage {
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        cost_usd: 0.0,
    };
    let steps = 0;
    //# Run provider steps until a final T is available
    while (steps < max_steps) {
        //## Ask Anthropic for a typed value or application calls
        let result = provider.step<T>(conversation, tools);
        steps = steps + 1;
        match (result.metadata.usage) {
            let step_usage: root.ai.Usage => {
                root.ai.internal._add_usage(usage, step_usage)
            },
            null => {},
        }
        //## Execute requested application tools or finish the completion
        match (result.outcome) {
            let calls: root.ai.tools.ToolCalls => {
                //### Reject handoffs because Completion<T> cannot return them
                for (let call in calls.calls) {
                    if (
                        tools
                            .find((tool: root.ai.tools.Tool) -> bool {
                                tool.name == call.name
                            })
                            ?.handoff
                            ?? false
                    ) {
                        throw baml.errors.UnknownError {
                            data: {
                                "tool": call.name,
                                "steps_taken": steps,
                            },
                            message: [
                                "Anthropic completion reached a handoff; use ai.run.Agent to receive it",
                            ],
                        }
                    }
                }
                //### Invoke BAML tools and return correlated tool_result blocks
                let tool_results = root
                    .ai
                    .internal
                    ._run_tool_calls(calls.calls, tools, null, null, null, [], [], null, steps);
                conversation
                    = provider
                        .as<root.ai.tools.ToolCallingProvider>
                        .submit(conversation, tool_results);
            },
            let value: T => {
                result.metadata.usage = usage;
                result.metadata.attributes.set("steps", steps);
                return root.ai.ResponseWithMetadata<T> {
                    value: value,
                    metadata: result.metadata,
                    conversation: conversation,
                };
            },
        }
    }

    throw baml.errors.UnknownError {
        data: {
            "max_steps": max_steps,
            "steps_taken": steps,
        },
        message: [
            "Anthropic completion exhausted its application-tool step budget",
        ],
    }
}
