// Stateful external agent runtime. Unlike Tools, a Harness owns its internal
// model/tool loop and exact provider continuation state.
interface Harness requires Provider {
  type HarnessSession

  function open(self, options: SessionOptions) -> HarnessSession
    throws baml.errors.UnknownError
  function run<T>(self, session: HarnessSession, request: Request<T>) -> AgentRun<T>
    throws baml.errors.UnknownError
  function stream<T>(self, session: HarnessSession, request: Request<T>) -> AgentEventStream<T>
    throws baml.errors.UnknownError
  function save_session(self, session: HarnessSession) -> HarnessSessionToken
    throws baml.errors.UnknownError
  function restore_session(self, token: HarnessSessionToken) -> HarnessSession
    throws baml.errors.UnknownError
  function stop(self, session: HarnessSession) -> null throws baml.errors.UnknownError
  function destroy(self, session: HarnessSession) -> null throws baml.errors.UnknownError
}

interface HarnessControlPlane requires Provider {
  function supported_verbs(self) -> string[] throws never
}

function run_harness<T>(
  request: Request<T>,
  options: SessionOptions = SessionOptions { attributes: {} },
) -> AgentRun<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (request.provider) {
    let harness: Harness => {
      let session = harness.open(options);
      let run = harness.run<T>(session, request);
      run.session_token = harness.save_session(session);
      run
    },
    _ => throw baml.errors.Unsupported {
      message: "provider is not an external harness: " + request.provider_name(),
    },
  }
}

function resume_harness<T>(
  request: Request<T>,
  token: HarnessSessionToken,
) -> AgentRun<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (request.provider) {
    let harness: Harness => {
      let session = harness.restore_session(token);
      let run = harness.run<T>(session, request);
      run.session_token = harness.save_session(session);
      run
    },
    _ => throw baml.errors.Unsupported {
      message: "provider is not an external harness: " + request.provider_name(),
    },
  }
}

function stream_harness<T>(
  request: Request<T>,
  options: SessionOptions = SessionOptions { attributes: {} },
) -> AgentEventStream<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
  let harness = match (request.provider) {
    let capability: Harness => capability,
    _ => throw baml.errors.Unsupported {
      message: "provider is not an external harness: " + request.provider_name(),
    },
  };
  let session = harness.open(options);
  harness.stream<T>(session, request)
}

function stop_harness(provider: Provider, token: HarnessSessionToken) -> HarnessSessionToken
    throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (provider) {
    let harness: Harness => {
      let session = harness.restore_session(token);
      harness.stop(session);
      harness.save_session(session)
    },
    _ => throw baml.errors.Unsupported { message: "provider is not an external harness" },
  }
}

function destroy_harness(provider: Provider, token: HarnessSessionToken) -> null
    throws baml.errors.UnknownError | baml.errors.Unsupported {
  match (provider) {
    let harness: Harness => {
      let session = harness.restore_session(token);
      harness.destroy(session)
    },
    _ => throw baml.errors.Unsupported { message: "provider is not an external harness" },
  }
}
