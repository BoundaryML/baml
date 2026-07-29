// A package-defined runner participates in exactly the same Task.run
// protocol as the built-in Agent. This example pins an AgentProvider, then
// delegates the actual conversation loop to the standard Agent lifecycle.
class GuideAgentCapability<T> {
    provider: root.ai.AgentProvider,

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
            root.ai.internal._run_agent_to_response<T>(task.with_provider(self.provider))
        }
    }
}
