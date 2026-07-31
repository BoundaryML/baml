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
    total.add(next)
}

// A cooperative Agent cancellation is intentionally separate from cancelling
// the Future running the Agent. Future.cancel() makes the Future terminal and
// cannot return a resumable outcome. Call this helper only at boundaries where
// the provider conversation has no unsubmitted application tool results.
function _interrupted_if_requested(
    cancel: baml.spawn.CancelToken?,
    conversation: ai.Conversation,
    steps_taken: int,
    usage: ai.Usage,
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
                usage: usage,
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
            let unavailable = ai.tools.ToolError.of(original, "tool is not in the active registry");
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
                let blocked = ai.tools.ToolError.of(original, decision.blocked_reason ?? "blocked");
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
        null => ai.tools.ToolError.of(call, "rewritten tool is not in the active registry"),
      }
    }
  }));
    //# Publish one result for every approved provider call ID
    for (let result in dispatched) {
        let call = match (
            approved.find((candidate: ai.tools.ToolCall) -> bool {
                candidate.id == ai.tools.result_id(result)
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
// Ownership also holds THROUGH delegation: a thin wrapper that forwards to
// an inner provider declares it via Provider.delegate(), and the check
// walks that chain — so a delegating middleware is legal at any depth.
function _same_provider_instance(
    left: ai.Provider,
    right: ai.Provider,
) -> bool throws never {
    _reaches_provider(left, right) || _reaches_provider(right, left)
}

// True when `outer` IS `target` or forwards to it through delegate().
// Depth-capped: a delegation cycle is a bug, not an infinite loop.
function _reaches_provider(
    outer: ai.Provider,
    target: ai.Provider,
) -> bool throws never {
    let current: ai.Provider? = outer;
    let depth = 0;
    while (depth < 16) {
        match (current) {
            null => return false,
            let candidate: ai.Provider => {
                // Class == is structural in BAML. Bound-method equality
                // includes receiver identity, so equal provider
                // configurations cannot impersonate one another's
                // in-process continuation.
                if (candidate.name == target.name) {
                    return true;
                }
                current = candidate.delegate();
            },
        }
        depth = depth + 1;
    }
    false
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
// task.run(runner = ai.run.Agent...), then shares `_done`'s unwrap exactly:
// a stop state throws `ai.IncompleteRun` carrying the resumable outcome (the
// committed conversation intact), and a `Failed` outcome rethrows its cause.
// Direct calls and task.complete() therefore have one contract, as the
// design docs state.
function _run_agent_to_response<T>(
    task: ai.Task<T>,
) -> ai.ResponseWithMetadata<T> throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError {
    //# Drive the task with the bounded default Agent policy, then share
    //# task.complete()'s unwrap
    _response_from_done<T>(_done<T>(ai.run.Agent<T>.new().run(task)))
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
        //# Null means "not reported": the output-type guard is skipped for
        //# such conversations, exactly like pending_calls' null convention.
        //# The interface docstring promises application-defined
        //# conversations may return null, and the runtime honors it.
        null => {},
    }
    null
}

// The four ways a run can start. The private wire between AgentSession's
// continuation methods and the one Agent loop; never part of user code.
// Totality here is what lets append and submit happen INSIDE the execution
// boundary, after invariants, instead of in application code.
class FreshStart {}

class ResumeStart {
    conversation: ai.Conversation,
}

class NextStart {
    conversation: ai.Conversation,
    messages: ai.Message[],
}

class AnswerStart {
    conversation: ai.Conversation,
    results: ai.tools.ToolResult[],
}

type AgentStart = FreshStart | ResumeStart | NextStart | AnswerStart

/// The committed conversation carried by any outcome variant.
function _outcome_conversation<T>(
    outcome: ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed,
) -> ai.Conversation throws never {
    match (outcome) {
        let done: ai.Done<T> => done.conversation,
        let stopped: ai.Stopped => stopped.conversation,
        let handoff: ai.Handoff => handoff.conversation,
        let interrupted: ai.Interrupted => interrupted.conversation,
        let failed: ai.Failed => failed.conversation,
    }
}

/// Every pending call answered exactly once, and nothing else. One uniform
/// check with one uniform message, replacing per-adapter wording.
function _require_exact_correlation(
    pending: ai.tools.ToolCall[],
    results: ai.tools.ToolResult[],
    provider_name: string,
) -> null throws ai.InvalidRequest {
    let expected = pending.map((call: ai.tools.ToolCall) -> string {
        call.id
    });
    let seen: string[] = [];
    for (let result in results) {
        let id = ai.tools.result_id(result);
        if (!expected.includes(id) || seen.includes(id)) {
            throw ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: `tool results must answer each pending call exactly once; unexpected or duplicate result id "${id}"`,
            };
        }
        seen.push(id);
    }
    if (seen.length() != expected.length()) {
        throw ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: `tool results must answer each pending call exactly once; got ${seen.length()} of ${expected.length()}`,
        };
    }
    null
}

/// Internal unwrap shared by the completion verbs (`task.complete()`,
/// `session.complete(...)`). Application code that needs the full Done —
/// metadata or conversation alongside the value — matches the outcome
/// union.
function _done<T>(
    outcome: ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed,
) -> ai.Done<T> throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError {
    match (outcome) {
        let done: ai.Done<T> => done,
        let stopped: ai.Stopped => throw ai.IncompleteRun { outcome: stopped },
        let handoff: ai.Handoff => throw ai.IncompleteRun { outcome: handoff },
        let interrupted: ai.Interrupted => throw ai.IncompleteRun { outcome: interrupted },
        let failed: ai.Failed => {
            //# The caller demanded completion; a fault is a fault. The
            //# session (if any) has still advanced to the committed state.
            match (failed.cause) {
                let failure: ai.Failure => throw failure,
                let unknown: baml.errors.UnknownError => throw unknown,
            }
        },
    }
}
