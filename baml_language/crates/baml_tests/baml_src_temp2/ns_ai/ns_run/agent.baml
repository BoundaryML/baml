// The normal model lifecycle. The runner owns every loop decision and every
// application tool effect; the selected AgentProvider owns exactly one model
// turn at a time plus its provider-specific conversation state.
class Agent<T> {
    budget: root.ai.Budget?,
    // A passive, cooperative signal. Do not also attach this token to the
    // Future's spawn options when a resumable Interrupted outcome is needed.
    cancel: baml.spawn.CancelToken?,
    // Null preserves provider-supported parallel tool calling. A number bounds
    // every provider ToolCalls batch before callbacks, handoffs, or handlers
    // can observe it.
    max_tool_calls_per_step: int?,
    tools: root.ai.tools.Tool[]?,
    tool_registry: root.ai.tools.ToolRegistry?,
    prepare_step: ((root.ai.tools.StepContext) -> root.ai.tools.StepPlan throws root.ai.Failure)?,
    before_tool_call: ((root.ai.tools.BeforeToolCall) -> root.ai.tools.ToolDecision throws root.ai.Failure)?,
    after_tool_call: ((root.ai.tools.AfterToolCall) -> null throws root.ai.Failure)?,
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
    observers: root.ai.observe.AgentObserver[],
    recorders: root.ai.observe.AgentRecorder[],
    state: map<string, unknown>,
    run_id: string?,
    conversation: root.ai.Conversation?,

    function new(
        budget: root.ai.Budget? = root.ai.Budget { max_steps: 32, max_cost_usd: null },
        cancel: baml.spawn.CancelToken? = null,
        max_tool_calls_per_step: int? = null,
        tools: root.ai.tools.ToolInput[]? = null,
        tool_registry: root.ai.tools.ToolRegistry? = null,
        prepare_step: (
            (root.ai.tools.StepContext) -> root.ai.tools.StepPlan throws root.ai.Failure
        )? = null,
        before_tool_call: (
            (root.ai.tools.BeforeToolCall) -> root.ai.tools.ToolDecision throws root.ai.Failure
        )? = null,
        after_tool_call: ((root.ai.tools.AfterToolCall) -> null throws root.ai.Failure)? = null,
        on_event: ((root.ai.observe.AgentEvent) -> null throws never)? = null,
        observers: root.ai.observe.AgentObserver[] = [],
        recorders: root.ai.observe.AgentRecorder[] = [],
        state: map<string, unknown> = {},
        run_id: string? = null,
        conversation: root.ai.Conversation? = null,
    ) -> Agent<T> throws never {
        Agent<T> {
            budget: budget,
            cancel: cancel,
            max_tool_calls_per_step: max_tool_calls_per_step,
            tools: match (tools) {
                let inputs: root.ai.tools.ToolInput[] => root.ai.tools.normalize_tools(inputs),
                null => null,
            },
            tool_registry: tool_registry,
            prepare_step: prepare_step,
            before_tool_call: before_tool_call,
            after_tool_call: after_tool_call,
            on_event: on_event,
            observers: observers,
            recorders: recorders,
            state: state,
            run_id: run_id,
            conversation: conversation,
        }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff | root.ai.Interrupted
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff | root.ai.Interrupted throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
            //# Select the conversation-owning provider
            // A provider conversation is authoritative continuation state. Switching its
            // owner requires explicit import; display names are not identity keys.
            let selected: root.ai.Provider = match (self.conversation) {
                let existing: root.ai.Conversation => existing.provider(),
                null => task.provider,
            };
            match (self.conversation) {
                let existing: root.ai.Conversation => {
                    root.ai.internal._require_conversation_invariants<T>(existing, selected)
                },
                null => {},
            }
            let provider = match (selected) {
                let capability: root.ai.AgentProvider => capability,
                _ => {
                    throw baml.errors.Unsupported {
                        message: "provider cannot run an agent: " + selected.name(),
                    }
                },
            };
            if ((self.max_tool_calls_per_step ?? 0) < 0) {
                throw root.ai.InvalidRequest {
                    provider: selected.name(),
                    status_code: null,
                    detail: "Agent.max_tool_calls_per_step must be null or non-negative",
                };
            }
            //# Resume or begin the provider conversation
            let conversation = match (self.conversation) {
                let existing: root.ai.Conversation => existing,
                null => {
                    let created = provider.begin<T>(task);
                    root.ai.internal._require_conversation_invariants<T>(created, selected);
                    created
                },
            };
            //# Build the active tool registry
            let registry = match (self.tool_registry) {
                let existing: root.ai.tools.ToolRegistry => existing,
                null => {
                    root.ai
                        .tools
                        .ToolRegistry
                        .new(root.ai.tools._tool_inputs_from_tools(self.tools ?? task.tools))
                },
            };
            //# Initialize cumulative usage
            let usage = root.ai.Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
                cost_usd: 0.0,
            };
            let steps = 0;
            //# Run provider steps until the agent stops
            while (true) {
                //## Stop only at a fully committed conversation boundary
                match (
                    root.ai.internal._interrupted_if_requested(
                        self.cancel,
                        conversation,
                        steps,
                        selected,
                        self.run_id,
                        self.on_event,
                        self.observers,
                        self.recorders,
                    )
                ) {
                    let interrupted: root.ai.Interrupted => return interrupted,
                    null => {},
                }
                //## Record the current agent state
                log.info(
                    "Agent.run "
                        + {
                            "steps": steps,
                            "usage": usage,
                            "budget": self.budget,
                            "provider": provider.name(),
                            "tools": registry.snapshot().map((tool: root.ai.tools.Tool) -> string {
                                tool.name
                            }),
                        }
                        .to_string(),
                );
                //## Enforce the step budget
                match (self.budget?.max_steps) {
                    let max: int if steps >= max => {
                        return root.ai.BudgetReached {
                            conversation: conversation,
                            steps_taken: steps,
                            reason: "max_steps",
                        };
                    },
                    _ => {},
                }
                //## Enforce the cost budget
                match (self.budget?.max_cost_usd) {
                    let max: float if (usage.cost_usd ?? 0.0) >= max => {
                        return root.ai.BudgetReached {
                            conversation: conversation,
                            steps_taken: steps,
                            reason: "max_cost_usd",
                        };
                    },
                    _ => {},
                }
                let tools = registry.snapshot();
                let roster_version = registry.version;
                //## Prepare the next model step
                match (self.prepare_step) {
                    null => {},
                    let callback: ((root.ai.tools.StepContext) -> root.ai.tools.StepPlan throws root.ai.Failure) => {
                        let plan = callback(
                            root.ai.tools.StepContext {
                                run_id: self.run_id,
                                step: steps,
                                provider: selected,
                                tools: tools,
                                usage: usage,
                                state: self.state,
                                conversation: conversation,
                            },
                        );
                        match (plan.stop) {
                            let stop: root.ai.tools.AgentStop => {
                                return root.ai.BudgetReached {
                                    conversation: conversation,
                                    steps_taken: steps,
                                    reason: stop.reason,
                                };
                            },
                            null => {},
                        }
                        // A non-null plan contains the complete next roster.
                        match (plan.tools) {
                            let replacement: root.ai.tools.Tool[] => {
                                registry
                                    .replace_all(root.ai.tools._tool_inputs_from_tools(replacement))
                            },
                            null => {},
                        }
                        //### Import portable messages before switching providers
                        match (plan.provider) {
                            let next: root.ai.Provider => {
                                let importer = match (next) {
                                    let capability: root.ai.ConversationImportProvider => capability,
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
                                root.ai
                                    .internal
                                    ._require_conversation_invariants<T>(conversation, selected);
                                root.ai
                                    .internal
                                    ._emit_agent_event(
                                        root.ai.observe.ProviderChangedEvent {
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
                    root.ai
                        .internal
                        ._emit_agent_event(
                            root.ai.observe.ToolRosterChangedEvent {
                                id: self.run_id,
                                step: steps,
                                tools: registry.snapshot().map((tool: root.ai.tools.Tool) -> string {
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
                    root.ai.internal._interrupted_if_requested(
                        self.cancel,
                        conversation,
                        steps,
                        selected,
                        self.run_id,
                        self.on_event,
                        self.observers,
                        self.recorders,
                    )
                ) {
                    let interrupted: root.ai.Interrupted => return interrupted,
                    null => {},
                }
                root.ai
                    .internal
                    ._emit_agent_event(
                        root.ai.observe.ModelStartedEvent {
                            id: self.run_id,
                            provider: selected.name(),
                        },
                        self.on_event,
                        self.observers,
                        self.recorders,
                    );
                //## Ask the provider for its next step
                let step_result = provider.step<T>(conversation, tools);
                steps = steps + 1;
                //## Accumulate provider usage
                match (step_result.metadata.usage) {
                    let step_usage: root.ai.Usage => {
                        root.ai.internal._add_usage(usage, step_usage);
                        root.ai
                            .internal
                            ._emit_agent_event(
                                root.ai.observe.UsageEvent { id: self.run_id, usage: usage },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                    },
                    null => {},
                }
                //## Handle tool calls or a final value
                match (step_result.outcome) {
                    let calls: root.ai.tools.ToolCalls => {
                        //### Validate correlation IDs before any application effect
                        root.ai.internal._validate_tool_calls(calls, selected.name());
                        //### Enforce the per-step tool-call policy before any effect
                        match (self.max_tool_calls_per_step) {
                            let max: int if calls.calls.length() > max => {
                                throw root.ai.ToolCallLimitExceeded {
                                    provider: selected.name(),
                                    max_tool_calls_per_step: max,
                                    actual_tool_calls: calls.calls.length(),
                                    calls: calls,
                                    conversation: conversation,
                                    steps_taken: steps,
                                };
                            },
                            _ => {},
                        }
                        //### Stop only for one unambiguous handoff call
                        let handoffs = calls.calls.filter((call: root.ai.tools.ToolCall) -> bool {
                            tools
                                .find((tool: root.ai.tools.Tool) -> bool {
                                    tool.name == call.name
                                })
                                ?.handoff
                                ?? false
                        });
                        if (handoffs.length() > 0 && calls.calls.length() != 1) {
                            throw root.ai.InvalidRequest {
                                provider: selected.name(),
                                status_code: null,
                                detail: "a handoff model step must contain exactly one handoff call and no application calls",
                            };
                        }
                        match (handoffs.at(0)) {
                            let call: root.ai.tools.ToolCall => {
                                root.ai
                                    .internal
                                    ._emit_agent_event(
                                        root.ai.observe.RunFinishedEvent {
                                            id: self.run_id,
                                            outcome: "handoff",
                                        },
                                        self.on_event,
                                        self.observers,
                                        self.recorders,
                                    );
                                return root.ai.Handoff {
                                    call: call,
                                    conversation: conversation,
                                    steps_taken: steps,
                                };
                            },
                            null => {},
                        }
                        //### Dispatch tools and continue the conversation
                        let results = root
                            .ai
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
                            );
                        let submitted = provider.submit(conversation, results);
                        root.ai.internal._require_conversation_invariants<T>(submitted, selected);
                        conversation = submitted;
                        // The whole tool batch and every correlated result are
                        // committed. It is now safe to return a resumable
                        // checkpoint if cancellation raced this transaction.
                        match (
                            root.ai.internal._interrupted_if_requested(
                                self.cancel,
                                conversation,
                                steps,
                                selected,
                                self.run_id,
                                self.on_event,
                                self.observers,
                                self.recorders,
                            )
                        ) {
                            let interrupted: root.ai.Interrupted => return interrupted,
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
                        root.ai
                            .internal
                            ._emit_agent_event(
                                root.ai.observe.RunFinishedEvent {
                                    id: self.run_id,
                                    outcome: "done",
                                },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                        return root.ai.Done<T> { value: value, metadata: meta, conversation: conversation };
                    },
                }
            }
            baml.sys.panic("unreachable")
        }
    }
}
