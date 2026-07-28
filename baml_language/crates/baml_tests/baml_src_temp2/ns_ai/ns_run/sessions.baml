// Runs a typed task inside provider-owned session state while keeping Task.run
// as the only application-facing task execution surface.
class InSession<T> {
    session: root.ai.Session,

    function new(session: root.ai.Session) -> InSession<T> throws never {
        InSession<T> { session: session }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.CallError | baml.errors.UnknownError {
            //# Execute the task inside the provider-owned session state
            self.session._execute<T>(task)
        }
    }
}
