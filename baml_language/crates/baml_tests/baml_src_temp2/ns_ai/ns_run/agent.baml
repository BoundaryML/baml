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
            root.ai
                .run_agent<T>(
                    task,
                    self.budget,
                    self.tools,
                    self.tool_registry,
                    self.prepare_step,
                    self.before_tool_call,
                    self.after_tool_call,
                    self.on_event,
                    self.observers,
                    self.recorders,
                    self.state,
                    self.run_id,
                    self.conversation,
                )
        }
    }
}
