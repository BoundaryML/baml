class FakeSession {
    owner: FakeSessionProvider,
    session_id: string,
    history: root.ai.MessageHistory,
    is_cleaned: bool,
    cleanup_count: int,

    implements root.ai.sessions.Session {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function id(self) -> string throws never {
            self.session_id
        }

        function _execute<T>(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
            //# Reject runs after a provider closes the session
            if (self.is_cleaned) {
                throw baml.errors.UnknownError.with_message<never>(self.session_id, "session has been cleaned up");
            }
            //# Resolve the session Agent provider
            let provider = match (self.owner.inner) {
                let value: root.ai.AgentProvider => value,
                _ => {
                    throw baml.errors.Unsupported {
          message: "session inner cannot run an Agent",
        }
                },
            };
            //# Run and retain the latest conversation
            let response = root.ai.internal._run_agent_to_response<T>(task.with_provider(provider));
            self.history = task.messages().to_history();
            response
        }

        function token(self) -> root.ai.sessions.SessionToken throws never {
            root.ai.sessions.SessionToken {
                provider: self.owner.name(),
                id: self.session_id,
                version: 1,
                sealed: baml.json.stringify({ "session_id": self.session_id }),
            }
        }
    }

    implements root.ai.Resource {}
}

class FakeSessionProvider {
    inner: root.ai.Provider,
    sessions: FakeSession[],
}

implements root.ai.Provider for FakeSessionProvider {
  function name(self) -> string throws never { "fake-session" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements root.ai.sessions.SessionProvider for FakeSessionProvider {
  function open_session(self, options: root.ai.sessions.SessionOptions) -> root.ai.sessions.Session throws never {
    //# Allocate and register a new session
    let session = FakeSession {
      owner: self,
      session_id: options.session_id
        ?? "session-" + (self.sessions.length() + 1).to_string(),
      history: root.ai.MessageHistory.empty(),
      is_cleaned: false,
      cleanup_count: 0,
    };
    self.sessions.push(session);
    session
  }

  function resume_session(
    self,
    token: root.ai.sessions.SessionToken,
  ) -> root.ai.sessions.Session throws baml.errors.UnknownError {
    //# Validate session token ownership
    if (token.provider != self.name() || token.version != 1) {
      throw baml.errors.UnknownError.with_message<never>(token, "session token does not belong to this provider/version");
    }
    //# Find the saved session
    for (let session in self.sessions) {
      if (session.session_id == token.id) { return session; }
    }
    throw baml.errors.UnknownError.with_message<never>(token, "session token not found")
  }
}
