// Application-managed tool loop. The runner owns lifecycle policy while the
// selected ToolCallingProvider owns provider-specific conversation state and
// wire turns.
class Agent<T> {
    budget: root.ai.Budget?,
    tools: root.ai.Tool[]?,
    tool_registry: root.ai.ToolRegistry?,
    prepare_step: ((root.ai.StepContext) -> root.ai.StepPlan throws root.ai.ToolError)?,
    before_tool_call: ((root.ai.BeforeToolCall) -> root.ai.ToolDecision throws root.ai.ToolError)?,
    after_tool_call: ((root.ai.AfterToolCall) -> null throws root.ai.ToolError)?,
    on_event: ((root.ai.AgentEvent) -> null throws never)?,
    observers: root.ai.AgentObserver[],
    recorders: root.ai.AgentRecorder[],
    state: map<string, unknown>,
    run_id: string?,
    conversation: root.ai.Conversation?,

    function new(
        budget: root.ai.Budget? = null,
        tools: root.ai.ToolInput[]? = null,
        tool_registry: root.ai.ToolRegistry? = null,
        prepare_step: ((root.ai.StepContext) -> root.ai.StepPlan throws root.ai.ToolError)? = null,
        before_tool_call: (
            (root.ai.BeforeToolCall) -> root.ai.ToolDecision throws root.ai.ToolError
        )? = null,
        after_tool_call: ((root.ai.AfterToolCall) -> null throws root.ai.ToolError)? = null,
        on_event: ((root.ai.AgentEvent) -> null throws never)? = null,
        observers: root.ai.AgentObserver[] = [],
        recorders: root.ai.AgentRecorder[] = [],
        state: map<string, unknown> = {},
        run_id: string? = null,
        conversation: root.ai.Conversation? = null,
    ) -> Agent<T> throws never {
        Agent<T> {
            budget: budget,
            tools: match (tools) {
                let inputs: root.ai.ToolInput[] => root.ai.normalize_tools(inputs),
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
        type Output = root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
        type Error = root.ai.ToolError | baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff throws root.ai.ToolError | baml.errors.UnknownError | baml.errors.Unsupported {
            //# Select the conversation-owning provider
            // A provider conversation is authoritative continuation state. Switching its
            // owner requires explicit import; display names are not identity keys.
            let selected: root.ai.Provider = match (self.conversation) {
                let existing: root.ai.Conversation => existing.provider(),
                null => task.provider,
            };
            let provider = root.ai._adapt_tool_calling_provider(selected);
            //# Resume or begin the provider conversation
            let conversation = match (self.conversation) {
                let existing: root.ai.Conversation => existing,
                null => provider.begin<T>(task),
            };
            //# Build the active tool registry
            let registry = match (self.tool_registry) {
                let existing: root.ai.ToolRegistry => existing,
                null => {
                    root.ai
                        .ToolRegistry
                        .new(root.ai._tool_inputs_from_tools(self.tools ?? task.tools))
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
                //## Record the current agent state
                log.info(
                    "Agent.run "
                        + {
                            "steps": steps,
                            "usage": usage,
                            "budget": self.budget,
                            "provider": provider.name(),
                            "tools": registry.snapshot().map((tool: root.ai.Tool) -> string {
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
                    let callback: ((root.ai.StepContext) -> root.ai.StepPlan throws root.ai.ToolError) => {
                        let plan = callback(
                            root.ai.StepContext {
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
                            let stop: root.ai.AgentStop => {
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
                            let replacement: root.ai.Tool[] => {
                                registry.replace_all(root.ai._tool_inputs_from_tools(replacement))
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
                                let imported = importer.import_messages(conversation.messages());
                                let previous = selected.name();
                                selected = next;
                                provider = importer;
                                conversation = imported.conversation;
                                root.ai
                                    ._emit_agent_event(
                                        root.ai.ProviderChangedEvent {
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
                    null => {},
                }
                //## Publish changes to the active tool roster
                if (registry.version != roster_version) {
                    root.ai
                        ._emit_agent_event(
                            root.ai.ToolRosterChangedEvent {
                                id: self.run_id,
                                step: steps,
                                tools: registry.snapshot().map((tool: root.ai.Tool) -> string {
                                    tool.name
                                }),
                            },
                            self.on_event,
                            self.observers,
                            self.recorders,
                        );
                }
                tools = registry.snapshot();
                root.ai
                    ._emit_agent_event(
                        root.ai.ModelStartedEvent { id: self.run_id, provider: selected.name() },
                        self.on_event,
                        self.observers,
                        self.recorders,
                    );
                //## Ask the provider for its next step
                let step_result = provider.step<T>(conversation, tools);
                steps = steps + 1;
                //## Accumulate provider usage
                match (step_result.meta.usage) {
                    let step_usage: root.ai.Usage => {
                        root.ai._add_usage(usage, step_usage);
                        root.ai
                            ._emit_agent_event(
                                root.ai.UsageEvent { id: self.run_id, usage: usage },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                    },
                    null => {},
                }
                //## Handle tool calls or a final value
                match (step_result.outcome) {
                    let calls: root.ai.ToolCalls => {
                        //### Stop before dispatching a handoff tool
                        for (let call in calls.calls) {
                            if (root.ai._is_handoff(tools, call)) {
                                root.ai
                                    ._emit_agent_event(
                                        root.ai.RunFinishedEvent {
                                            id: self.run_id,
                                            outcome: "handoff",
                                        },
                                        self.on_event,
                                        self.observers,
                                        self.recorders,
                                    );
                                return root.ai.Handoff {
                                    to: call.name,
                                    args: call.args,
                                    conversation: conversation,
                                    steps_taken: steps,
                                };
                            }
                        }
                        //### Dispatch tools and continue the conversation
                        let results = root
                            .ai
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
                        conversation = provider.submit(conversation, results);
                    },
                    let value: T => {
                        //### Return the typed final value
                        let meta = root.ai.ResponseMetadata {
                            provider: selected.name(),
                            model: null,
                            request_id: self.run_id,
                            finish_reason: "agent_complete",
                            usage: usage,
                            attributes: { "steps": steps },
                            raw: null,
                        };
                        root.ai
                            ._emit_agent_event(
                                root.ai.RunFinishedEvent { id: self.run_id, outcome: "done" },
                                self.on_event,
                                self.observers,
                                self.recorders,
                            );
                        return root.ai.Done<T> { value: value, meta: meta, conversation: conversation };
                    },
                }
            }
            baml.sys.panic("unreachable")
        }
    }
}
