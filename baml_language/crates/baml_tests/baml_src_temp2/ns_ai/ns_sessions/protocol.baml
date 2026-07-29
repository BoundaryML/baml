class SessionOptions {
    session_id: string?,
    attributes: map<string, string>,
}

class SessionToken {
    provider: string,
    id: string,
    version: int,
    sealed: string,
}

interface Session requires root.ai.Resource {
  function provider(self) -> root.ai.Provider throws never
  function id(self) -> string throws never
  function _execute<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T>
    throws root.ai.Failure | baml.errors.UnknownError
  function token(self) -> SessionToken throws never
}

interface SessionProvider requires root.ai.Provider {
  function open_session(self, options: SessionOptions) -> Session
    throws baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session
    throws baml.errors.UnknownError
}
