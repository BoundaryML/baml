// Application-managed tool loop. The runner owns lifecycle policy while the
// selected ToolCallingProvider owns provider-specific conversation state and
// wire turns.
class Agent<T> {
  budget: root.ai.Budget?,
  tools: root.ai.Tool[]?,
  tool_registry: root.ai.ToolRegistry?,
  hooks: root.ai.AgentHooks?,
  observers: root.ai.AgentObserver[],
  recorders: root.ai.AgentRecorder[],
  state: map<string, unknown>,
  run_id: string?,
  conversation: root.ai.Conversation?,

  function new(
    budget: root.ai.Budget? = null,
    tools: root.ai.Tool[]? = null,
    tool_registry: root.ai.ToolRegistry? = null,
    hooks: root.ai.AgentHooks? = null,
    observers: root.ai.AgentObserver[] = [],
    recorders: root.ai.AgentRecorder[] = [],
    state: map<string, unknown> = {},
    run_id: string? = null,
    conversation: root.ai.Conversation? = null,
  ) -> Agent<T> throws never {
    Agent<T> {
      budget: budget,
      tools: tools,
      tool_registry: tool_registry,
      hooks: hooks,
      observers: observers,
      recorders: recorders,
      state: state,
      run_id: run_id,
      conversation: conversation,
    }
  }

  implements root.ai.Runner<root.ai.Task<T>> {
    type Output = root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
    type Error = baml.errors.ToolError | baml.errors.UnknownError | baml.errors.Unsupported

    function run(
      self,
      task: root.ai.Task<T>,
    ) -> root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
        throws baml.errors.ToolError | baml.errors.UnknownError | baml.errors.Unsupported {
      root.ai.run_agent<T>(
        task,
        self.budget,
        self.tools,
        self.tool_registry,
        self.hooks,
        self.observers,
        self.recorders,
        self.state,
        self.run_id,
        self.conversation,
      )
    }
  }
}
