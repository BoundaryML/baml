// A package-defined runner participates in exactly the same Task.run
// protocol as a built-in runner. It can still call a provider capability
// directly when it needs custom lifecycle policy.
class GuideGenerationCapability<T> {
    provider: root.ai.GenerationProvider,

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
            self.provider.generate<T>(task.with_provider(self.provider))
        }
    }
}
