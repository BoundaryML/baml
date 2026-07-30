// A package-defined runner participates in exactly the same Task.run
// protocol as the built-in Agent. This example pins an AgentProvider, then
// delegates the actual conversation loop to the standard Agent lifecycle.
class GuideAgentCapability<T> {
    provider: ai.AgentProvider,

    implements ai.Runner<ai.Task<T>> {
        type Output = ai.ResponseWithMetadata<T>
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.Task<T>,
        ) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError {
            ai.internal._run_agent_to_response<T>(task.with_provider(self.provider))
        }
    }
}
