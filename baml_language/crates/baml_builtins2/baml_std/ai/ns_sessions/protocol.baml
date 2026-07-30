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

interface Session requires ai.Resource {
  function provider(self) -> ai.Provider throws never
  function id(self) -> string throws never
  function _execute<T>(self, task: ai.Task<T>) -> ai.ResponseWithMetadata<T>
    throws ai.Failure | baml.errors.UnknownError
  function token(self) -> SessionToken throws never
}

interface SessionProvider requires ai.Provider {
  function open_session(self, options: SessionOptions) -> Session
    throws baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session
    throws baml.errors.UnknownError
}
