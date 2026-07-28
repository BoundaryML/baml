class Budget {
    max_steps: int?,
    max_cost_usd: float?,
}

function _is_handoff(tools: Tool[], call: ToolCall) -> bool throws never {
    //# Find whether the requested tool transfers control
    for (let tool in tools) {
        if (tool.name == call.name) {
            return tool.handoff;
        }
    }
    false
}

function _emit_agent_event(
    event: AgentEvent,
    on_event: ((AgentEvent) -> null throws never)?,
    observers: AgentObserver[],
    recorders: AgentRecorder[],
) -> null throws never {
    match (on_event) {
        let callback: ((AgentEvent) -> null throws never) => callback(event),
        null => null,
    };
    _publish_agent_event(event, observers, recorders)
}

function _add_usage(total: Usage, next: Usage) -> null throws never {
    total.input_tokens = total.input_tokens + next.input_tokens;
    total.output_tokens = total.output_tokens + next.output_tokens;
    total.cached_input_tokens = (total.cached_input_tokens ?? 0) + (next.cached_input_tokens ?? 0);
    total.reasoning_tokens = (total.reasoning_tokens ?? 0) + (next.reasoning_tokens ?? 0);
    total.cost_usd = (total.cost_usd ?? 0.0) + (next.cost_usd ?? 0.0);
    null
}

function _run_tool_calls(
    calls: ToolCall[],
    active_tools: Tool[],
    before_tool_call: ((BeforeToolCall) -> ToolDecision throws root.ai.ToolError)?,
    after_tool_call: ((AfterToolCall) -> null throws root.ai.ToolError)?,
    on_event: ((AgentEvent) -> null throws never)?,
    observers: AgentObserver[],
    recorders: AgentRecorder[],
    run_id: string?,
    step: int,
) -> ToolResult[] throws root.ai.ToolError {
    let approved: ToolCall[] = [];
    let results: ToolResult[] = [];
    //# Review proposed tool calls
    for (let original in calls) {
        //## Publish the proposed call
        _emit_agent_event(
            ToolCallEvent { id: run_id, phase: "proposed", call: original, result: null },
            on_event,
            observers,
            recorders,
        );
        //## Reject tools outside the active registry
        let known = active_tools.filter((tool: Tool) -> bool {
            tool.name == original.name
        });
        if (known.length() != 1) {
            let unavailable = ToolResult.error(original, "tool is not in the active registry");
            results.push(unavailable);
            _emit_agent_event(
                ToolCallEvent { id: run_id, phase: "blocked", call: original, result: unavailable },
                on_event,
                observers,
                recorders,
            );
            continue;
        }
        //## Apply the before-tool callback
        let decision = match (before_tool_call) {
            let callback: ((BeforeToolCall) -> ToolDecision throws root.ai.ToolError) => {
                callback(BeforeToolCall { run_id: run_id, step: step, call: original })
            },
            null => ToolDecision.allow(original),
        };
        //## Approve, rewrite, or block the call
        match (decision.call) {
            let call: ToolCall => {
                // Callbacks may rewrite the tool name or arguments, but the provider owns
                // the correlation ID. Normalize it back to the original ID.
                let approved_call = ToolCall { id: original.id, name: call.name, args: call.args };
                approved.push(approved_call);
                _emit_agent_event(
                    ToolCallEvent {
                        id: run_id,
                        phase: "started",
                        call: approved_call,
                        result: null,
                    },
                    on_event,
                    observers,
                    recorders,
                );
            },
            null => {
                let blocked = ToolResult.error(original, decision.blocked_reason ?? "blocked");
                // Provider protocols require every requested call ID to receive a
                // result. A policy block is therefore submitted as an error result,
                // not only emitted as an observability event.
                results.push(blocked);
                _emit_agent_event(
                    ToolCallEvent { id: run_id, phase: "blocked", call: original, result: blocked },
                    on_event,
                    observers,
                    recorders,
                );
            },
        }
    }
    //# Dispatch approved function-backed tools concurrently
    let dispatched = await baml.future.all(approved.map((call: ToolCall) -> {
    spawn {
      match (active_tools.find((tool: Tool) -> bool { tool.name == call.name })) {
        let selected: Tool => invoke_tool(selected, call),
        null => ToolResult.error(call, "rewritten tool is not in the active registry"),
      }
    }
  }));
    //# Publish one result for every approved provider call ID
    for (let result in dispatched) {
        let call = match (
            approved.find((candidate: ToolCall) -> bool {
                candidate.id == result.id
            })
        ) {
            let value: ToolCall => value,
            null => baml.sys.panic("agent produced a result for an unknown tool call"),
        };
        results.push(result);
        //## Apply the after-tool callback
        match (after_tool_call) {
            let callback: ((AfterToolCall) -> null throws root.ai.ToolError) => {
                callback(AfterToolCall { run_id: run_id, step: step, call: call, result: result })
            },
            null => {},
        }
        _emit_agent_event(
            ToolCallEvent { id: run_id, phase: "finished", call: call, result: result },
            on_event,
            observers,
            recorders,
        );
    }
    results
}
