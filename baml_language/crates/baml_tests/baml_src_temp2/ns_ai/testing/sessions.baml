class FakeSession {
    owner: FakeSessionProvider,
    session_id: string,
    history: MessageHistory,
    is_cleaned: bool,
    cleanup_count: int,

    implements Session {
        function provider(self) -> Provider throws never {
            self.owner
        }
        function id(self) -> string throws never {
            self.session_id
        }

        function run<T>(
            self,
            task: Task<T>,
        ) -> Response<T> throws root.ai.CallError | baml.errors.UnknownError {
            //# Reject runs after a provider closes the session
            if (self.is_cleaned) {
                throw baml.errors.UnknownError {
          data: self.session_id,
          message: ["session has been cleaned up"],
        };
            }
            //# Resolve the session completion provider
            let completion = match (self.owner.inner) {
                let value: CompletionProvider => value,
                _ => {
                    throw baml.errors.Unsupported {
          message: "session inner cannot complete",
        }
                },
            };
            //# Run and retain the latest conversation
            let response = completion.complete<T>(task.with_provider(self.owner.inner));
            self.history = task.messages().to_history();
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

    implements Resource {}
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
    //# Allocate and register a new session
    let session = FakeSession {
      owner: self,
      session_id: options.session_id
        ?? "session-" + (self.sessions.length() + 1).to_string(),
      history: MessageHistory.empty(),
      is_cleaned: false,
      cleanup_count: 0,
    };
    self.sessions.push(session);
    session
  }

  function resume_session(
    self,
    token: SessionToken,
  ) -> Session throws baml.errors.UnknownError {
    //# Validate session token ownership
    if (token.provider != self.name() || token.version != 1) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["session token does not belong to this provider/version"],
      };
    }
    //# Find the saved session
    for (let session in self.sessions) {
      if (session.session_id == token.id) { return session; }
    }
    throw baml.errors.UnknownError {
      data: token,
      message: ["session token not found"],
    }
  }
}
