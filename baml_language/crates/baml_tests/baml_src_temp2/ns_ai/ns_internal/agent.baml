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
    before_tool_call: (
        (root.ai.tools.BeforeToolCall) -> root.ai.tools.ToolDecision throws root.ai.Failure
    )?,
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
            root.ai.observe.ToolCallEvent {
                id: run_id,
                phase: "proposed",
                call: original,
                result: null,
            },
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
                root.ai.observe.ToolCallEvent {
                    id: run_id,
                    phase: "blocked",
                    call: original,
                    result: unavailable,
                },
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
                    root.ai.observe.ToolCallEvent {
                        id: run_id,
                        phase: "blocked",
                        call: original,
                        result: blocked,
                    },
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
                callback(
                    root.ai.tools.AfterToolCall {
                        run_id: run_id,
                        step: step,
                        call: call,
                        result: result,
                    },
                )
            },
        }
        _emit_agent_event(
            root.ai.observe.ToolCallEvent {
                id: run_id,
                phase: "finished",
                call: call,
                result: result,
            },
            on_event,
            observers,
            recorders,
        );
    }
    results
}

// Validate the provider-controlled correlation envelope before any application
// handler or handoff can observe it. Providers still validate their own wire
// state again during submit.
function _validate_tool_calls(
    calls: root.ai.tools.ToolCalls,
    provider_name: string,
) -> null throws root.ai.InvalidRequest {
    if (calls.calls.length() == 0) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "an Agent model step must not return an empty ToolCalls batch",
        };
    }
    let ids: string[] = [];
    for (let call in calls.calls) {
        if (call.id.length() == 0) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "Agent tool calls must have non-empty correlation IDs",
            };
        }
        if (ids.includes(call.id)) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "Agent tool-call correlation IDs must be unique within a model step",
            };
        }
        if (call.name.length() == 0) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "Agent tool calls must have non-empty names",
            };
        }
        ids.push(call.id);
    }
    null
}

// Every continuation returned through AgentProvider is owned by the exact
// provider object that created/imported it and is bound to the same T.
function _same_provider_instance(
    left: root.ai.Provider,
    right: root.ai.Provider,
) -> bool throws never {
    // Class == is structural in BAML. Bound-method equality includes receiver
    // identity, so equal provider configurations cannot impersonate one
    // another's in-process continuation.
    left.name == right.name
}

function _require_conversation_invariants<T>(
    conversation: root.ai.Conversation,
    provider: root.ai.Provider,
) -> null throws root.ai.InvalidRequest {
    if (!_same_provider_instance(conversation.provider(), provider)) {
        throw root.ai.InvalidRequest {
            provider: provider.name(),
            status_code: null,
            detail: "AgentProvider returned a conversation owned by another provider instance",
        };
    }
    _require_conversation_output_type<T>(conversation, provider.name())
}

// Default direct-call lowering. It uses the same Agent lifecycle as explicit
// task.run(runner = ai.run.Agent...), then projects the final response shape
// expected by generated LLM functions and provider-owned resources.
function _run_agent_to_response<T>(
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
    //# Drive the task with the bounded default Agent policy
    let outcome = root.ai.run.Agent<T>.new().run(task);
    //# Project a final response or explain why a value-only call cannot finish
    match (outcome) {
        let done: root.ai.Done<T> => {
            root.ai.ResponseWithMetadata<T> {
                value: done.value,
                metadata: done.metadata,
                conversation: done.conversation,
            }
        },
        let stopped: root.ai.BudgetReached => {
            throw baml.errors.UnknownError {
                data: {
                    "steps_taken": stopped.steps_taken,
                    "reason": stopped.reason,
                },
                message: ["default Agent stopped before producing the task value"],
            }
        },
        let handoff: root.ai.Handoff => {
            throw baml.errors.UnknownError {
                data: {
                    "tool": handoff.call.name,
                    "args": handoff.call.args,
                    "call_id": handoff.call.id,
                    "steps_taken": handoff.steps_taken,
                },
                message: ["default Agent reached a handoff; run the task with an explicit Agent"],
            }
        },
    }
}

function _require_done<T>(
    outcome: root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff,
) -> root.ai.Done<T> throws baml.errors.UnknownError {
    match (outcome) {
        let done: root.ai.Done<T> => done,
        let stopped: root.ai.BudgetReached => {
            throw baml.errors.UnknownError {
                data: {
                    "steps_taken": stopped.steps_taken,
                    "reason": stopped.reason,
                },
                message: ["Agent stopped before producing the expected value"],
            }
        },
        let handoff: root.ai.Handoff => {
            throw baml.errors.UnknownError {
                data: {
                    "tool": handoff.call.name,
                    "args": handoff.call.args,
                    "call_id": handoff.call.id,
                    "steps_taken": handoff.steps_taken,
                },
                message: ["Agent handed off before producing the expected value"],
            }
        },
    }
}

function _response_from_done<T>(
    done: root.ai.Done<T>,
) -> root.ai.ResponseWithMetadata<T> throws never {
    root.ai.ResponseWithMetadata<T> {
        value: done.value,
        metadata: done.metadata,
        conversation: done.conversation,
    }
}

function _canonicalize_output_schema(value: json) -> json throws never {
    match (value) {
        let object: map<string, json> => {
            let canonical: map<string, json> = {};
            let keys = object.keys();
            keys.sort();
            for (let key in keys) {
                canonical.set(key, _canonicalize_output_schema(object[key]));
            }
            canonical
        },
        let items: json[] => {
            items.map((item: json) -> json {
                _canonicalize_output_schema(item)
            })
        },
        _ => value,
    }
}

// Versioned, deterministic output identity for persisted conversations. The
// nominal name keeps diagnostics readable; the canonical schema detects a
// class or union whose shape changed without being renamed.
function _output_type_fingerprint<T>() -> string throws never {
    let nominal = reflect.type_of<T>().to_string();
    let schema_text: string? = baml.json.stringify(_canonicalize_output_schema(baml.schema.json_schema(reflect.type_of<T>()))) catch (e) {
        _ => null,
    };
    match (schema_text) {
        let value: string => {
            "baml-output/v1;nominal=" + nominal + ";schema=" + value
        },
        null => {
            // Some runtime-only types cannot be represented as JSON Schema.
            // Mark the weaker fallback explicitly so it is never confused with
            // a structural fingerprint.
            "baml-output/v1-fallback;nominal=" + nominal + ";schema=unsupported"
        },
    }
}

function _require_conversation_output_type<T>(
    conversation: root.ai.Conversation,
    provider_name: string,
) -> null throws root.ai.InvalidRequest {
    match (conversation.output_type_fingerprint()) {
        let stored: string => {
            let nominal = reflect.type_of<T>().to_string();
            let fingerprint = _output_type_fingerprint<T>();
            if (stored != fingerprint) {
                throw root.ai.InvalidRequest {
                    provider: provider_name,
                    status_code: null,
                    detail: "conversation output fingerprint "
                        + stored
                        + " cannot resume task output "
                        + nominal
                        + " (expected "
                        + fingerprint
                        + ")",
                }
            }
        },
        null => {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "AgentProvider conversation has no versioned output fingerprint",
            }
        },
    }
    null
}
