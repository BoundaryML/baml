// Hidden plumbing for the agent runner. Nothing in ai.internal is part of
// the public surface: helpers here may change shape without notice and never
// appear in documentation examples.
function _emit_agent_event(
    event: root.ai.observe.AgentEvent,
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
    observers: root.ai.observe.AgentObserver[],
    recorders: root.ai.observe.AgentRecorder[],
) -> null throws never {
    match (on_event) {
        null => null,
        let callback: ((root.ai.observe.AgentEvent) -> null throws never) => callback(event),
    };
    root.ai.observe._publish_agent_event(event, observers, recorders)
}

function _add_usage(total: root.ai.Usage, next: root.ai.Usage) -> null throws never {
    total.input_tokens = total.input_tokens + next.input_tokens;
    total.output_tokens = total.output_tokens + next.output_tokens;
    total.cached_input_tokens = (total.cached_input_tokens ?? 0) + (next.cached_input_tokens ?? 0);
    total.reasoning_tokens = (total.reasoning_tokens ?? 0) + (next.reasoning_tokens ?? 0);
    total.cost_usd = (total.cost_usd ?? 0.0) + (next.cost_usd ?? 0.0);
    null
}

function _run_tool_calls(
    calls: root.ai.tools.ToolCall[],
    active_tools: root.ai.tools.Tool[],
    before_tool_call: ((root.ai.tools.BeforeToolCall) -> root.ai.tools.ToolDecision throws root.ai.Failure)?,
    after_tool_call: ((root.ai.tools.AfterToolCall) -> null throws root.ai.Failure)?,
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
    observers: root.ai.observe.AgentObserver[],
    recorders: root.ai.observe.AgentRecorder[],
    run_id: string?,
    step: int,
) -> root.ai.tools.ToolResult[] throws root.ai.Failure {
    let approved: root.ai.tools.ToolCall[] = [];
    let results: root.ai.tools.ToolResult[] = [];
    //# Review proposed tool calls
    for (let original in calls) {
        //## Publish the proposed call
        _emit_agent_event(
            root.ai.observe.ToolCallEvent { id: run_id, phase: "proposed", call: original, result: null },
            on_event,
            observers,
            recorders,
        );
        //## Reject tools outside the active registry
        let known = active_tools.filter((tool: root.ai.tools.Tool) -> bool {
            tool.name == original.name
        });
        if (known.length() != 1) {
            let unavailable = root.ai.tools.ToolResult.error(original, "tool is not in the active registry");
            results.push(unavailable);
            _emit_agent_event(
                root.ai.observe.ToolCallEvent { id: run_id, phase: "blocked", call: original, result: unavailable },
                on_event,
                observers,
                recorders,
            );
            continue;
        }
        //## Apply the before-tool callback
        let decision = match (before_tool_call) {
            null => root.ai.tools.ToolDecision.allow(original),
            let callback: ((root.ai.tools.BeforeToolCall) -> root.ai.tools.ToolDecision throws root.ai.Failure) => {
                callback(root.ai.tools.BeforeToolCall { run_id: run_id, step: step, call: original })
            },
        };
        //## Approve, rewrite, or block the call
        match (decision.call) {
            let call: root.ai.tools.ToolCall => {
                // Callbacks may rewrite the tool name or arguments, but the provider owns
                // the correlation ID. Normalize it back to the original ID.
                let approved_call = root.ai.tools.ToolCall { id: original.id, name: call.name, args: call.args };
                approved.push(approved_call);
                _emit_agent_event(
                    root.ai.observe.ToolCallEvent {
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
                let blocked = root.ai.tools.ToolResult.error(original, decision.blocked_reason ?? "blocked");
                // Provider protocols require every requested call ID to receive a
                // result. A policy block is therefore submitted as an error result,
                // not only emitted as an observability event.
                results.push(blocked);
                _emit_agent_event(
                    root.ai.observe.ToolCallEvent { id: run_id, phase: "blocked", call: original, result: blocked },
                    on_event,
                    observers,
                    recorders,
                );
            },
        }
    }
    //# Dispatch approved function-backed tools concurrently
    let dispatched = await baml.future.all(approved.map((call: root.ai.tools.ToolCall) -> {
    spawn {
      match (active_tools.find((tool: root.ai.tools.Tool) -> bool { tool.name == call.name })) {
        let selected: root.ai.tools.Tool => root.ai.tools.invoke_tool(selected, call),
        null => root.ai.tools.ToolResult.error(call, "rewritten tool is not in the active registry"),
      }
    }
  }));
    //# Publish one result for every approved provider call ID
    for (let result in dispatched) {
        let call = match (
            approved.find((candidate: root.ai.tools.ToolCall) -> bool {
                candidate.id == result.id
            })
        ) {
            let value: root.ai.tools.ToolCall => value,
            null => baml.sys.panic("agent produced a result for an unknown tool call"),
        };
        results.push(result);
        //## Apply the after-tool callback
        match (after_tool_call) {
            null => {},
            let callback: ((root.ai.tools.AfterToolCall) -> null throws root.ai.Failure) => {
                callback(root.ai.tools.AfterToolCall { run_id: run_id, step: step, call: call, result: result })
            },
        }
        _emit_agent_event(
            root.ai.observe.ToolCallEvent { id: run_id, phase: "finished", call: call, result: result },
            on_event,
            observers,
            recorders,
        );
    }
    results
}
