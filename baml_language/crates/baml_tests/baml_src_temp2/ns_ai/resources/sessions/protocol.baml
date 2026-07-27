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

interface Session requires Resource {
  function provider(self) -> Provider throws never
  function id(self) -> string throws never
  function run<T>(self, task: Task<T>) -> Response<T>
    throws root.ai.CallError | baml.errors.UnknownError
  function token(self) -> SessionToken throws never
}

interface SessionProvider requires Provider {
  function open_session(self, options: SessionOptions) -> Session
    throws baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session
    throws baml.errors.UnknownError
}
