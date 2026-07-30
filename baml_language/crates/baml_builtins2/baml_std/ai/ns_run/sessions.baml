// Runs a typed task inside provider-owned session state while keeping ai.Task.run
// as the only application-facing task execution surface.
class InSession<T> {
    session: ai.sessions.Session,

    function new(session: ai.sessions.Session) -> InSession<T> throws never {
        InSession<T> { session: session }
    }

    implements ai.Runner<ai.Task<T>> {
        type Output = ai.ResponseWithMetadata<T>
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.Task<T>,
        ) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError {
            //# Execute the task inside the provider-owned session state
            self.session._execute<T>(task)
        }
    }
}
