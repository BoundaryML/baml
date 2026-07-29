// Runs a typed task inside provider-owned session state while keeping root.ai.Task.run
// as the only application-facing task execution surface.
class InSession<T> {
    session: root.ai.sessions.Session,

    function new(session: root.ai.sessions.Session) -> InSession<T> throws never {
        InSession<T> { session: session }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
            //# Execute the task inside the provider-owned session state
            self.session._execute<T>(task)
        }
    }
}
