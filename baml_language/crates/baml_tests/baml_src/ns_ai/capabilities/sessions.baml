class SessionOptions {}
class SessionToken { provider: string, id: string }

interface Session {
  function run<T>(self, request: Request<T>) -> T throws baml.errors.UnknownError
  function run_with_meta<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError
  function provider(self) -> Provider throws never
  function token(self) -> SessionToken throws never
  function cleanup(self) -> null throws never
}

interface Sessions requires Provider {
  function open_session(self, options: SessionOptions) -> Session throws baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session throws baml.errors.UnknownError
}

function open_session(provider: Provider, options: SessionOptions) -> Session
    throws baml.errors.UnknownError {
  let sessions = match (provider) {
    let capability: Sessions => capability,
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider cannot open sessions"] },
  };
  sessions.open_session(options)
}
