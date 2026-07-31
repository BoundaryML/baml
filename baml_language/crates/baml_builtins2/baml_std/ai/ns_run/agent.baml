// The normal model lifecycle. The runner owns every loop decision and every
// application tool effect; the selected AgentProvider owns exactly one model
// turn at a time plus its provider-specific conversation state.
class Agent<T> {
    // Null means unlimited steps; the default construction caps at 32.
    max_steps: int?,
    // Optional stop policy, consulted at every committed boundary with the
    // same context `prepare_step` receives. Returning true stops the run
    // with an `ai.Stopped { reason: "stop_when" }` outcome.
    stop_when: ((ai.tools.StepContext) -> bool throws ai.Failure)?,
    // A passive, cooperative signal. Do not also attach this token to the
    // Future's spawn options when a resumable Interrupted outcome is needed.
    cancel: baml.spawn.CancelToken?,
    // Null preserves provider-supported parallel tool calling. A number bounds
    // every provider ToolCalls batch before callbacks, handoffs, or handlers
    // can observe it.
    max_tool_calls_per_step: int?,
    // Runner-supplied tools MERGE with task-declared tools by default; a
    // same-named runner tool wins. Set replace_tools to make the runner
    // roster the whole roster. tools and tool_registry are mutually
    // exclusive supply points.
    tools: ai.tools.Tool[]?,
    tool_registry: ai.tools.ToolRegistry?,
    replace_tools: bool,
    prepare_step: ((ai.tools.StepContext) -> ai.tools.StepPlan throws ai.Failure)?,
    before_tool_call: ((ai.tools.BeforeToolCall) -> ai.tools.ToolDecision throws ai.Failure)?,
    after_tool_call: ((ai.tools.AfterToolCall) -> null throws ai.Failure)?,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,
    observers: ai.observe.AgentObserver[],
    recorders: ai.observe.AgentRecorder[],
    state: map<string, unknown>,
    run_id: string?,

    function new(
        max_steps: int? = 32,
        stop_when: ((ai.tools.StepContext) -> bool throws ai.Failure)? = null,
        cancel: baml.spawn.CancelToken? = null,
        max_tool_calls_per_step: int? = null,
        tools: ai.tools.ToolInput[]? = null,
        tool_registry: ai.tools.ToolRegistry? = null,
        replace_tools: bool = false,
        prepare_step: (
            (ai.tools.StepContext) -> ai.tools.StepPlan throws ai.Failure
        )? = null,
        before_tool_call: (
            (ai.tools.BeforeToolCall) -> ai.tools.ToolDecision throws ai.Failure
        )? = null,
        after_tool_call: ((ai.tools.AfterToolCall) -> null throws ai.Failure)? = null,
        on_event: ((ai.observe.AgentEvent) -> null throws never)? = null,
        observers: ai.observe.AgentObserver[] = [],
        recorders: ai.observe.AgentRecorder[] = [],
        state: map<string, unknown> = {},
        run_id: string? = null,
    ) -> Agent<T> throws never {
        Agent<T> {
            max_steps: max_steps,
            stop_when: stop_when,
            cancel: cancel,
            max_tool_calls_per_step: max_tool_calls_per_step,
            tools: match (tools) {
                let inputs: ai.tools.ToolInput[] => ai.tools.normalize_tools(inputs),
                null => null,
            },
            tool_registry: tool_registry,
            replace_tools: replace_tools,
            prepare_step: prepare_step,
            before_tool_call: before_tool_call,
            after_tool_call: after_tool_call,
            on_event: on_event,
            observers: observers,
            recorders: recorders,
            state: state,
            run_id: run_id,
        }
    }

    implements ai.Runner<ai.Task<T>> {
        type Output = ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.Task<T>,
        ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
            //# task.run is always a fresh start; continuations enter through
            //# AgentSession's intent methods
            self._run(task, ai.internal.FreshStart {})
        }
    }

    function _run(
        self,
        task: ai.Task<T>,
        start: ai.internal.AgentStart,
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
            //# Select the conversation-owning provider
            // A provider conversation is authoritative continuation state. Switching its
            // owner requires explicit import; display names are not identity keys.
            let selected: ai.Provider = match (start) {
                let fresh: ai.internal.FreshStart => task.provider,
                let resume: ai.internal.ResumeStart => resume.conversation.provider(),
                let next: ai.internal.NextStart => next.conversation.provider(),
                let answer: ai.internal.AnswerStart => answer.conversation.provider(),
            };
            match (start) {
                let fresh: ai.internal.FreshStart => {},
                let resume: ai.internal.ResumeStart => {
                    ai.internal._require_conversation_invariants<T>(resume.conversation, selected);
                },
                let next: ai.internal.NextStart => {
                    ai.internal._require_conversation_invariants<T>(next.conversation, selected);
                },
                let answer: ai.internal.AnswerStart => {
                    ai.internal._require_conversation_invariants<T>(answer.conversation, selected);
                },
            }
            let provider = match (selected) {
                let capability: ai.AgentProvider => capability,
                _ => {
                    throw baml.errors.Unsupported {
                        message: "provider cannot run an agent: " + selected.name(),
                    }
                },
            };
            if ((self.max_tool_calls_per_step ?? 0) < 0) {
                throw ai.InvalidRequest {
                    provider: selected.name(),
                    status_code: null,
                    detail: "Agent.max_tool_calls_per_step must be null or non-negative",
                };
            }
            //# Surface a malformed render shorthand as a classified failure
            //# up front (D9b) instead of a panic mid-render.
            let _ = selected._render_client();
            //# A task whose OUTPUT TYPE is ai.tools.ToolCalls is ambiguous: a
            //# provider step returns `T | ToolCalls`, so the Agent could not
            //# distinguish a final value of that type from a request to
            //# execute tools. Refuse up front instead of burning the step
            //# limit on unknown-tool errors.
            if (task.output_type() == reflect.type_of<ai.tools.ToolCalls>()) {
                throw ai.InvalidRequest {
                    provider: selected.name(),
                    status_code: null,
                    detail: "a task's output type cannot be ai.tools.ToolCalls — return your own type and let the Agent execute the tools",
                };
            }
            //# Acquire the conversation by declared start mode. Everything
            //# that mutates it happens HERE, inside the boundary, after the
            //# invariant checks above. `progressed` is the D11 marker: once
            //# true, failures return `Failed` at the last committed state
            //# instead of throwing.
            let progressed = false;
            let conversation = match (start) {
                let fresh: ai.internal.FreshStart => {
                    let created = provider.begin<T>(task);
                    ai.internal._require_conversation_invariants<T>(created, selected);
                    created
                },
                let resume: ai.internal.ResumeStart => resume.conversation,
                let next: ai.internal.NextStart => {
                    //# Runtime typestate: a conversation with unanswered tool
                    //# calls is a handoff, not a place to append user text.
                    match (next.conversation.pending_calls()) {
                        let pending: ai.tools.ToolCall[] if pending.length() != 0 => {
                            throw ai.InvalidRequest {
                                provider: selected.name(),
                                status_code: null,
                                detail: "the conversation has unanswered tool calls — this is a handoff; call submit_tool_results, not send",
                            };
                        },
                        _ => {},
                    }
                    //# The append IS the first commit of this continuation: a
                    //# failing append leaves the conversation unchanged (the
                    //# adapter atomicity rule); a successful one means every
                    //# later failure resumes without re-appending. Append
                    //# mutates the conversation in place.
                    let _ = next
                        .conversation
                        .append_messages(ai.MessageHistory { entries: next.messages });
                    progressed = true;
                    next.conversation
                },
                let answer: ai.internal.AnswerStart => {
                    //# Runtime typestate + strict correlation, when the
                    //# provider reports its pending calls.
                    match (answer.conversation.pending_calls()) {
                        let pending: ai.tools.ToolCall[] => {
                            if (pending.length() == 0) {
                                throw ai.InvalidRequest {
                                    provider: selected.name(),
                                    status_code: null,
                                    detail: "no pending tool calls to answer — did you mean send?",
                                };
                            }
                            let _ = ai.internal._require_exact_correlation(pending, answer.results, selected.name());
                        },
                        null => {},
                    }
                    let submitted = provider.submit(answer.conversation, answer.results);
                    ai.internal._require_conversation_invariants<T>(submitted, selected);
                    progressed = true;
                    submitted
                },
            };
            //# Resolve the tool roster (D6). Two supply points, one rule:
            //# the runner override MERGES with task-declared tools by
            //# default (a same-named override wins); `replace_tools = true`
            //# makes the override the whole roster.
            //#
            //# `tools` is a VALUE: the roster is snapshotted into a
            //# run-local registry, so nothing the loop does is visible
            //# outside this run. `tool_registry` is deliberately LIVE
            //# SHARED STATE — its reason to exist is dynamic rosters (MCP
            //# bootstrap, handler-driven adds/removes): the loop re-reads
            //# it before every step, and StepPlan roster changes apply to
            //# it and remain visible to the caller after the run.
            if (self.tools != null && self.tool_registry != null) {
                throw ai.InvalidRequest {
                    provider: selected.name(),
                    status_code: null,
                    detail: "supply either Agent.tools or Agent.tool_registry, not both",
                };
            }
            let registry = match (self.tool_registry) {
                let live: ai.tools.ToolRegistry => {
                    //# Seed the live registry with task-declared tools it
                    //# does not already name (skipped under replace_tools).
                    if (!self.replace_tools) {
                        for (let declared in task.tools) {
                            if (!live.contains(declared.name)) {
                                let _ = live.add(declared);
                            }
                        }
                    }
                    live
                },
                null => {
                    let roster: ai.tools.Tool[] = match (self.tools) {
                        null => task.tools,
                        let supplied: ai.tools.Tool[] => {
                            if (self.replace_tools) {
                                supplied
                            } else {
                                let merged: ai.tools.Tool[] = [];
                                for (let declared in task.tools) {
                                    match (
                                        supplied.find((candidate: ai.tools.Tool) -> bool {
                                            candidate.name == declared.name
                                        })
                                    ) {
                                        null => { merged.push(declared); },
                                        _ => {},
                                    }
                                }
                                for (let extra in supplied) {
                                    merged.push(extra);
                                }
                                merged
                            }
                        },
                    };
                    ai.tools.ToolRegistry.new(ai.tools.as_inputs(roster))
                },
            };
            //# Initialize cumulative usage
            let usage = ai.Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
            };
            let steps = 0;
            //# Run provider steps until the agent stops
            while (true) {
                //## Stop only at a fully committed conversation boundary
                match (
                    ai.internal._interrupted_if_requested(
                        self.cancel,
                        conversation,
                        steps,
                        usage,
                        selected,
                        self.run_id,
                        self.on_event,
                        self.observers,
                        self.recorders,
                    )
                ) {
                    let interrupted: ai.Interrupted => return interrupted,
                    null => {},
                }
                //## Record the current agent state
                log.info(
                    "Agent.run "
                        + {
                            "steps": steps,
                            "usage": usage,
                            "max_steps": self.max_steps,
                            "provider": provider.name(),
                            "tools": registry.snapshot().map((tool: ai.tools.Tool) -> string {
                                tool.name
                            }),
                        }
                        .to_string(),
                );
                let tools = registry.snapshot();
                let roster_version = registry.version;
                let context = ai.tools.StepContext {
                    run_id: self.run_id,
                    step: steps,
                    provider: selected,
                    tools: tools,
                    usage: usage,
                    state: self.state,
                    conversation: conversation,
                };
                //## Enforce the step limit
                match (self.max_steps) {
                    let max: int if steps >= max => {
                        return ai.Stopped {
                            conversation: conversation,
                            steps_taken: steps,
                            usage: usage,
                            reason: "max_steps",
                        };
                    },
                    _ => {},
                }
                //## Consult the caller's stop policy
                match (self.stop_when) {
                    null => {},
                    let should_stop: ((ai.tools.StepContext) -> bool throws ai.Failure) => {
                        let stop = should_stop(context) catch (e) {
                            let failure: ai.Failure => {
                                if (progressed) {
                                    return self._failed(failure, conversation, steps, usage);
                                }
                                throw failure;
                            },
                        };
                        if (stop) {
                            return ai.Stopped {
                                conversation: conversation,
                                steps_taken: steps,
                                usage: usage,
                                reason: "stop_when",
                            };
                        }
                    },
                }
                //## Prepare the next model step
                match (self.prepare_step) {
                    null => {},
                    let callback: ((ai.tools.StepContext) -> ai.tools.StepPlan throws ai.Failure) => {
                        let plan = callback(
                            context,
                        ) catch (e) {
                            let failure: ai.Failure => {
                                if (progressed) {
                                    return self._failed(failure, conversation, steps, usage);
                                }
                                throw failure;
                            },
                        };
                        match (plan.stop) {
                            let stop: ai.tools.AgentStop => {
                                return ai.Stopped {
                                    conversation: conversation,
                                    steps_taken: steps,
                                    usage: usage,
                                    reason: stop.reason,
                                };
                            },
                            null => {},
                        }
                        // A non-null plan contains the complete next roster.
                        match (plan.tools) {
                            let replacement: ai.tools.Tool[] => {
                                registry
                                    .replace_all(ai.tools.as_inputs(replacement))
                            },
                            null => {},
                        }
                        //### Import portable messages before switching providers
                        match (plan.provider) {
                            let next: ai.Provider => {
                                let importer = match (next) {
                                    let capability: ai.ConversationImportProvider => capability,
                                    _ => {
                                        throw baml.errors.Unsupported {
                                            message: "provider switch requires conversation import: "
                                                + next.name(),
                                        }
                                    },
                                };
                                let imported = importer.import_messages<T>(conversation.messages());
                                let previous = selected.name();
                                selected = next;
                                provider = importer;
                                conversation = imported.conversation;
                                ai
                                    .internal
                                    ._require_conversation_invariants<T>(conversation, selected);
                                ai
                                    .internal
                                    ._emit_agent_event(
                                        ai.observe.ProviderChangedEvent {
                                            id: self.run_id,
                                            from_provider: previous,
                                            to_provider: selected.name(),
                                            fidelity: imported.fidelity,
                                            warnings: imported.warnings,
                                        },
                                        self.on_event,
                                        self.observers,
                                        self.recorders,
                                    );
                            },
                            null => {},
                        }
                    },
                }
                //## Publish changes to the active tool roster
                if (registry.version != roster_version) {
                    ai
                        .internal
                        ._emit_agent_event(
                            ai.observe.ToolRosterChangedEvent {
                                id: self.run_id,
                                step: steps,
                                tools: registry.snapshot().map((tool: ai.tools.Tool) -> string {
                                    tool.name
                                }),
                            },
                            self.on_event,
                            self.observers,
                            self.recorders,
                        );
                }
                tools = registry.snapshot();
                //## Honor cancellation requested during step preparation
                // No provider request or application tool effect has started.
                match (
                    ai.internal._interrupted_if_requested(
                        self.cancel,
                        conversation,
                        steps,
                        usage,
                        selected,
                        self.run_id,
                        self.on_event,
                        self.observers,
                        self.recorders,
                    )
                ) {
                    let interrupted: ai.Interrupted => return interrupted,
                    null => {},
                }
                ai
                    .internal
                    ._emit_agent_event(
                        ai.observe.ModelStartedEvent {
                            id: self.run_id,
                            provider: selected.name(),
                        },
                        self.on_event,
                        self.observers,
                        self.recorders,
                    );
                //## Ask the provider for its next step. Before any progress
                //## a failure throws (nothing committed — ordinary catch and
                //## step-level retry semantics apply); after progress it
                //## returns Failed at the last committed boundary.
                let step_result = provider.step<T>(conversation, tools) catch (e) {
                    let failure: ai.Failure => {
                        if (progressed) {
                            return self._failed(failure, conversation, steps, usage);
                        }
                        throw failure;
                    },
                    let unknown: baml.errors.UnknownError => {
                        if (progressed) {
                            return self._failed(unknown, conversation, steps, usage);
                        }
                        throw unknown;
                    },
                };
                progressed = true;
                steps = steps + 1;
                //## Telemetry floor (D10e): one event per completed step
                //## with the step's OWN metadata and usage delta, so
                //## intermediate request IDs survive multi-step runs.
                ai
                    .internal
                    ._emit_agent_event(
                        ai.observe.StepFinishedEvent {
                            id: self.run_id,
                            step: steps,
                            metadata: step_result.metadata,
                            usage_delta: step_result.metadata.usage,
                            at: baml.time.Instant.now(),
                        },
                        self.on_event,
                        self.observers,
                        self.recorders,
                    );
                //## Display channels (D10c), when the provider exposes them
                match (step_result.assistant_text) {
                    let text: string => {
                        ai
                            .internal
                            ._emit_agent_event(
                                ai.observe.AssistantTextEvent {
                                    id: self.run_id,
                                    step: steps,
                                    text: text,
                                    at: baml.time.Instant.now(),
                                },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                    },
                    null => {},
                }
                match (step_result.reasoning_text) {
                    let text: string => {
                        ai
                            .internal
                            ._emit_agent_event(
                                ai.observe.ReasoningEvent {
                                    id: self.run_id,
                                    step: steps,
                                    text: text,
                                    at: baml.time.Instant.now(),
                                },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                    },
                    null => {},
                }
                //## Accumulate provider usage
                match (step_result.metadata.usage) {
                    let step_usage: ai.Usage => {
                        ai.internal._add_usage(usage, step_usage);
                        ai
                            .internal
                            ._emit_agent_event(
                                ai.observe.UsageEvent { id: self.run_id, usage: usage },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                    },
                    null => {},
                }
                //## Handle tool calls or a final value
                match (step_result.outcome) {
                    let calls: ai.tools.ToolCalls => {
                        //### Validate correlation IDs before any application effect
                        let _ = ai.internal._validate_tool_calls(calls, selected.name()) catch (e) {
                            let failure: ai.Failure => {
                                return self._failed(failure, conversation, steps, usage);
                            },
                        };
                        //### Enforce the per-step tool-call policy before any effect
                        match (self.max_tool_calls_per_step) {
                            let max: int if calls.calls.length() > max => {
                                //# The step is committed; the batch is the
                                //# fault. Return Failed so the application
                                //# can reject the calls with error results
                                //# via submit_tool_results and continue.
                                return self._failed(
                                    ai.ToolCallLimitExceeded {
                                        provider: selected.name(),
                                        max_tool_calls_per_step: max,
                                        actual_tool_calls: calls.calls.length(),
                                        calls: calls,
                                        conversation: conversation,
                                        steps_taken: steps,
                                    },
                                    conversation,
                                    steps,
                                    usage,
                                );
                            },
                            _ => {},
                        }
                        //### Stop only for one unambiguous handoff call
                        let handoffs = calls.calls.filter((call: ai.tools.ToolCall) -> bool {
                            tools
                                .find((tool: ai.tools.Tool) -> bool {
                                    tool.name == call.name
                                })
                                ?.handoff
                                ?? false
                        });
                        if (handoffs.length() > 0 && calls.calls.length() != 1) {
                            return self._failed(
                                ai.InvalidRequest {
                                    provider: selected.name(),
                                    status_code: null,
                                    detail: "a handoff model step must contain exactly one handoff call and no application calls",
                                },
                                conversation,
                                steps,
                                usage,
                            );
                        }
                        match (handoffs.at(0)) {
                            let call: ai.tools.ToolCall => {
                                ai
                                    .internal
                                    ._emit_agent_event(
                                        ai.observe.RunFinishedEvent {
                                            id: self.run_id,
                                            outcome: "handoff",
                                        },
                                        self.on_event,
                                        self.observers,
                                        self.recorders,
                                    );
                                return ai.Handoff {
                                    call: call,
                                    conversation: conversation,
                                    steps_taken: steps,
                                    usage: usage,
                                };
                            },
                            null => {},
                        }
                        //### Dispatch tools and continue the conversation
                        let results = ai
                            .internal
                            ._run_tool_calls(
                                calls.calls,
                                tools,
                                self.before_tool_call,
                                self.after_tool_call,
                                self.on_event,
                                self.observers,
                                self.recorders,
                                self.run_id,
                                steps,
                            ) catch (e) {
                            let failure: ai.Failure => {
                                return self._failed(failure, conversation, steps, usage);
                            },
                        };
                        let submitted = provider.submit(conversation, results) catch (e) {
                            let failure: ai.Failure => {
                                return self._failed(failure, conversation, steps, usage);
                            },
                            let unknown: baml.errors.UnknownError => {
                                return self._failed(unknown, conversation, steps, usage);
                            },
                        };
                        let _ = ai.internal._require_conversation_invariants<T>(submitted, selected) catch (e) {
                            let failure: ai.Failure => {
                                return self._failed(failure, submitted, steps, usage);
                            },
                        };
                        conversation = submitted;
                        // The whole tool batch and every correlated result are
                        // committed. It is now safe to return a resumable
                        // checkpoint if cancellation raced this transaction.
                        match (
                            ai.internal._interrupted_if_requested(
                                self.cancel,
                                conversation,
                                steps,
                                usage,
                                selected,
                                self.run_id,
                                self.on_event,
                                self.observers,
                                self.recorders,
                            )
                        ) {
                            let interrupted: ai.Interrupted => return interrupted,
                            null => {},
                        }
                    },
                    let value: T => {
                        //### Return the typed final value
                        // Keep the provider's final response identity and raw
                        // payload. Only cumulative run fields are overlaid.
                        let meta = step_result.metadata;
                        meta.usage = usage;
                        meta.attributes.set("steps", steps);
                        match (self.run_id) {
                            let id: string => {
                                meta.attributes.set("run_id", id)
                            },
                            null => {},
                        }
                        ai
                            .internal
                            ._emit_agent_event(
                                ai.observe.RunFinishedEvent {
                                    id: self.run_id,
                                    outcome: "done",
                                },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                        return ai.Done<T> {
                            value: value,
                            metadata: meta,
                            conversation: conversation,
                            steps_taken: steps,
                            usage: usage,
                        };
                    },
                }
            }
            baml.sys.panic("unreachable")
    }

    /// Constructs the involuntary-stop outcome and closes the run for
    /// observers. Only called once progress exists.
    function _failed(
        self,
        cause: ai.Failure | baml.errors.UnknownError,
        conversation: ai.Conversation,
        steps: int,
        usage: ai.Usage,
    ) -> ai.Failed throws never {
        ai
            .internal
            ._emit_agent_event(
                ai.observe.RunFinishedEvent {
                    id: self.run_id,
                    outcome: "failed",
                },
                self.on_event,
                self.observers,
                self.recorders,
            );
        ai.Failed {
            cause: cause,
            conversation: conversation,
            steps_taken: steps,
            usage: usage,
        }
    }
}
