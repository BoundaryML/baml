function run_agent<T>(
  task: root.ai.Task<T>,
  options: root.ai.AgentOptions,
) -> root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError | baml.errors.Unsupported {
  root.ai.run_agent<T>(task, options)
}

