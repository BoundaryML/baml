// Nominal agent-loop driver for the interface-driven comparison surface.
// The value is immutable execution policy: the existing agent loop continues
// to own transcripts, registries, events, and every other mutable resource.
class RunAgent<T> {
  options: root.ai.AgentOptions,
}

implements<T> root.ai.Driver<root.ai.Task<T>> for RunAgent<T> {
  type Output = root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
  type Error = baml.errors.ToolError | baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, task: root.ai.Task<T>)
      -> root.ai.Done<T> | root.ai.BudgetReached | root.ai.Handoff
      throws baml.errors.ToolError | baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.run_agent<T>(task, self.options)
  }
}

function agent<T>(options: root.ai.AgentOptions) -> RunAgent<T> throws never {
  RunAgent<T> { options: options }
}
