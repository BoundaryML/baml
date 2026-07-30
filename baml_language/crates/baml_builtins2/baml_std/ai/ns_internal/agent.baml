// Hidden plumbing for the agent runner. Nothing in ai.internal is part of
// the public surface: helpers here may change shape without notice and never
// appear in documentation examples.
function _emit_agent_event(
    event: ai.observe.AgentEvent,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,
    observers: ai.observe.AgentObserver[],
    recorders: ai.observe.AgentRecorder[],
) -> null throws never {
    match (on_event) {
        null => null,
        let callback: ((ai.observe.AgentEvent) -> null throws never) => callback(event),
    };
    ai.observe._publish_agent_event(event, observers, recorders)
}

function _add_usage(total: ai.Usage, next: ai.Usage) -> null throws never {
    total.input_tokens = total.input_tokens + next.input_tokens;
    total.output_tokens = total.output_tokens + next.output_tokens;
    total.cached_input_tokens = (total.cached_input_tokens ?? 0) + (next.cached_input_tokens ?? 0);
    total.reasoning_tokens = (total.reasoning_tokens ?? 0) + (next.reasoning_tokens ?? 0);
    total.cost_usd = (total.cost_usd ?? 0.0) + (next.cost_usd ?? 0.0);
    null
}

// A cooperative Agent cancellation is intentionally separate from cancelling
// the Future running the Agent. Future.cancel() makes the Future terminal and
// cannot return a resumable outcome. Call this helper only at boundaries where
// the provider conversation has no unsubmitted application tool results.
function _interrupted_if_requested(
    cancel: baml.spawn.CancelToken?,
    conversation: ai.Conversation,
    steps_taken: int,
    provider: ai.Provider,
    run_id: string?,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,
    observers: ai.observe.AgentObserver[],
    recorders: ai.observe.AgentRecorder[],
) -> ai.Interrupted? throws never {
    match (cancel) {
        let token: baml.spawn.CancelToken if token.is_cancelled() => {
            let reason = "cancelled";
            _emit_agent_event(
                ai.observe.RunInterruptedEvent {
                    id: run_id,
                    provider: provider.name(),
                    steps_taken: steps_taken,
                    reason: reason,
                },
                on_event,
                observers,
                recorders,
            );
            // Keep the generic terminal signal so observers that do not know
            // about the dedicated interruption event still close the run.
            _emit_agent_event(
                ai.observe.RunFinishedEvent {
                    id: run_id,
                    outcome: "interrupted",
                },
                on_event,
                observers,
                recorders,
            );
            ai.Interrupted {
                conversation: conversation,
                steps_taken: steps_taken,
                reason: reason,
            }
        },
        _ => null,
    }
}

function _run_tool_calls(
    calls: ai.tools.ToolCall[],
    active_tools: ai.tools.Tool[],
    before_tool_call: (
        (ai.tools.BeforeToolCall) -> ai.tools.ToolDecision throws ai.Failure
    )?,
    after_tool_call: ((ai.tools.AfterToolCall) -> null throws ai.Failure)?,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,
    observers: ai.observe.AgentObserver[],
    recorders: ai.observe.AgentRecorder[],
    run_id: string?,
    step: int,
) -> ai.tools.ToolResult[] throws ai.Failure {
    let approved: ai.tools.ToolCall[] = [];
    let results: ai.tools.ToolResult[] = [];
    //# Review proposed tool calls
    for (let original in calls) {
        //## Publish the proposed call
        _emit_agent_event(
            ai.observe.ToolCallEvent {
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
        let known = active_tools.filter((tool: ai.tools.Tool) -> bool {
            tool.name == original.name
        });
        if (known.length() != 1) {
            let unavailable = ai.tools.ToolResult.error(original, "tool is not in the active registry");
            results.push(unavailable);
            _emit_agent_event(
                ai.observe.ToolCallEvent {
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
            null => ai.tools.ToolDecision.allow(original),
            let callback: ((ai.tools.BeforeToolCall) -> ai.tools.ToolDecision throws ai.Failure) => {
                callback(ai.tools.BeforeToolCall { run_id: run_id, step: step, call: original })
            },
        };
        //## Approve, rewrite, or block the call
        match (decision.call) {
            let call: ai.tools.ToolCall => {
                // Callbacks may rewrite the tool name or arguments, but the provider owns
                // the correlation ID. Normalize it back to the original ID.
                let approved_call = ai.tools.ToolCall { id: original.id, name: call.name, args: call.args };
                approved.push(approved_call);
                _emit_agent_event(
                    ai.observe.ToolCallEvent {
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
                let blocked = ai.tools.ToolResult.error(original, decision.blocked_reason ?? "blocked");
                // Provider protocols require every requested call ID to receive a
                // result. A policy block is therefore submitted as an error result,
                // not only emitted as an observability event.
                results.push(blocked);
                _emit_agent_event(
                    ai.observe.ToolCallEvent {
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
    let dispatched = await baml.future.all(approved.map((call: ai.tools.ToolCall) -> {
    spawn {
      match (active_tools.find((tool: ai.tools.Tool) -> bool { tool.name == call.name })) {
        let selected: ai.tools.Tool => ai.tools.invoke_tool(selected, call),
        null => ai.tools.ToolResult.error(call, "rewritten tool is not in the active registry"),
      }
    }
  }));
    //# Publish one result for every approved provider call ID
    for (let result in dispatched) {
        let call = match (
            approved.find((candidate: ai.tools.ToolCall) -> bool {
                candidate.id == result.id
            })
        ) {
            let value: ai.tools.ToolCall => value,
            null => baml.sys.panic("agent produced a result for an unknown tool call"),
        };
        results.push(result);
        //## Apply the after-tool callback
        match (after_tool_call) {
            null => {},
            let callback: ((ai.tools.AfterToolCall) -> null throws ai.Failure) => {
                callback(
                    ai.tools.AfterToolCall {
                        run_id: run_id,
                        step: step,
                        call: call,
                        result: result,
                    },
                )
            },
        }
        _emit_agent_event(
            ai.observe.ToolCallEvent {
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
    calls: ai.tools.ToolCalls,
    provider_name: string,
) -> null throws ai.InvalidRequest {
    if (calls.calls.length() == 0) {
        throw ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "an Agent model step must not return an empty ToolCalls batch",
        };
    }
    let ids: string[] = [];
    for (let call in calls.calls) {
        if (call.id.length() == 0) {
            throw ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "Agent tool calls must have non-empty correlation IDs",
            };
        }
        if (ids.includes(call.id)) {
            throw ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "Agent tool-call correlation IDs must be unique within a model step",
            };
        }
        if (call.name.length() == 0) {
            throw ai.InvalidRequest {
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
    left: ai.Provider,
    right: ai.Provider,
) -> bool throws never {
    // Class == is structural in BAML. Bound-method equality includes receiver
    // identity, so equal provider configurations cannot impersonate one
    // another's in-process continuation.
    left.name == right.name
}

function _require_conversation_invariants<T>(
    conversation: ai.Conversation,
    provider: ai.Provider,
) -> null throws ai.InvalidRequest {
    if (!_same_provider_instance(conversation.provider(), provider)) {
        throw ai.InvalidRequest {
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
    task: ai.Task<T>,
) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError {
    //# Drive the task with the bounded default Agent policy
    let outcome = ai.run.Agent<T>.new().run(task);
    //# Project a final response or explain why a value-only call cannot finish
    match (outcome) {
        let done: ai.Done<T> => {
            ai.ResponseWithMetadata<T> {
                value: done.value,
                metadata: done.metadata,
                conversation: done.conversation,
            }
        },
        let stopped: ai.BudgetReached => {
            throw baml.errors.UnknownError {
                data: {
                    "steps_taken": stopped.steps_taken,
                    "reason": stopped.reason,
                },
                message: ["default Agent stopped before producing the task value"],
            }
        },
        let handoff: ai.Handoff => {
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
        let interrupted: ai.Interrupted => {
            throw baml.errors.UnknownError {
                data: {
                    "steps_taken": interrupted.steps_taken,
                    "reason": interrupted.reason,
                },
                message: ["default Agent was interrupted before producing the task value"],
            }
        },
    }
}

function _require_done<T>(
    outcome: ai.Done<T> | ai.BudgetReached | ai.Handoff | ai.Interrupted,
) -> ai.Done<T> throws baml.errors.UnknownError {
    match (outcome) {
        let done: ai.Done<T> => done,
        let stopped: ai.BudgetReached => {
            throw baml.errors.UnknownError {
                data: {
                    "steps_taken": stopped.steps_taken,
                    "reason": stopped.reason,
                },
                message: ["Agent stopped before producing the expected value"],
            }
        },
        let handoff: ai.Handoff => {
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
        let interrupted: ai.Interrupted => {
            throw baml.errors.UnknownError {
                data: {
                    "steps_taken": interrupted.steps_taken,
                    "reason": interrupted.reason,
                },
                message: ["Agent was interrupted before producing the expected value"],
            }
        },
    }
}

function _response_from_done<T>(
    done: ai.Done<T>,
) -> ai.ResponseWithMetadata<T> throws never {
    ai.ResponseWithMetadata<T> {
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
    conversation: ai.Conversation,
    provider_name: string,
) -> null throws ai.InvalidRequest {
    match (conversation.output_type_fingerprint()) {
        let stored: string => {
            let nominal = reflect.type_of<T>().to_string();
            let fingerprint = _output_type_fingerprint<T>();
            if (stored != fingerprint) {
                throw ai.InvalidRequest {
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
            throw ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "AgentProvider conversation has no versioned output fingerprint",
            }
        },
    }
    null
}
