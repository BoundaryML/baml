// A package-defined runner participates in exactly the same Task.run
// protocol as the built-in Agent. This example pins an AgentProvider, then
// delegates the actual conversation loop to the standard Agent lifecycle.
class GuideAgentCapability<T> {
    provider: ai.AgentProvider,

    implements ai.Runner<ai.Task<T>> {
        type Output = ai.ResponseWithMetadata<T>
        type Error = ai.IncompleteRun | ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.Task<T>,
        ) -> ai.ResponseWithMetadata<T> throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError {
            let outcome = task
                .with_provider(self.provider)
                .run(runner = ai.run.Agent<T>.new());
            match (outcome) {
                let done: ai.Done<T> => done.response(),
                let failed: ai.Failed => throw failed.cause,
                let stopped: ai.Stopped => throw ai.IncompleteRun { outcome: stopped },
                let stopped: ai.Handoff => throw ai.IncompleteRun { outcome: stopped },
                let stopped: ai.Interrupted => throw ai.IncompleteRun { outcome: stopped },
            }
        }
    }
}
