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
    throws baml.errors.CallError | baml.errors.UnknownError
  function token(self) -> SessionToken throws never
}

interface SessionProvider requires Provider {
  function open_session(self, options: SessionOptions) -> Session
    throws baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session
    throws baml.errors.UnknownError
}

class FakeSession {
  owner: FakeSessionProvider,
  session_id: string,
  history: Conversation,
  is_cleaned: bool,
  cleanup_count: int,

  implements Session {
    function provider(self) -> Provider throws never { self.owner }
    function id(self) -> string throws never { self.session_id }
    function run<T>(self, task: Task<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      if (self.is_cleaned) {
        throw baml.errors.UnknownError {
          data: self.session_id,
          message: ["session has been cleaned up"],
        };
      }
      let drive = match (self.owner.inner) {
        let value: DriveProvider => value,
        _ => throw baml.errors.Unsupported { message: "session inner cannot drive" },
      };
      let response = drive.drive<T>(task.with_provider(self.owner.inner));
      self.history = task.messages().to_conversation();
      response
    }
    function token(self) -> SessionToken throws never {
      SessionToken {
        provider: self.owner.name(),
        id: self.session_id,
        version: 1,
        sealed: baml.json.stringify({ "session_id": self.session_id }),
      }
    }
  }
  implements Resource {
    function cleanup(self) -> null throws never {
      if (!self.is_cleaned) {
        self.is_cleaned = true;
        self.cleanup_count = self.cleanup_count + 1;
      }
      null
    }
  }
}

class FakeSessionProvider {
  inner: Provider,
  sessions: FakeSession[],
}

implements Provider for FakeSessionProvider {
  function name(self) -> string throws never { "fake-session" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements SessionProvider for FakeSessionProvider {
  function open_session(self, options: SessionOptions) -> Session throws never {
    let session = FakeSession {
      owner: self,
      session_id: options.session_id ?? "session-" + (self.sessions.length() + 1).to_string(),
      history: Conversation.empty(),
      is_cleaned: false,
      cleanup_count: 0,
    };
    self.sessions.push(session);
    session
  }

  function resume_session(self, token: SessionToken) -> Session
      throws baml.errors.UnknownError {
    if (token.provider != self.name() || token.version != 1) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["session token does not belong to this provider/version"],
      };
    }
    for (let session in self.sessions) {
      if (session.session_id == token.id) { return session; }
    }
    throw baml.errors.UnknownError { data: token, message: ["session token not found"] }
  }
}
