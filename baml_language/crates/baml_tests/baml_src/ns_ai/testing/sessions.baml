// Non-forking adapter that scopes ordinary Generate calls behind Session.
class ProviderSession {
  owner: Provider,
  id: string,

  implements Session {
    function run<T>(self, request: Request<T>) -> T throws baml.errors.UnknownError {
      root.ai.run<T>(request.for_provider(self.owner)) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["session run failed"] },
      }
    }
    function run_with_meta<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.UnknownError {
      root.ai.run_with_meta<T>(request.for_provider(self.owner)) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["session run failed"] },
      }
    }
    function provider(self) -> Provider throws never { self.owner }
    function token(self) -> SessionToken throws never {
      SessionToken { provider: self.owner.display_name(), id: self.id }
    }
    function cleanup(self) -> null throws never { null }
  }
}

// Provider-owned deterministic fake session. The public token keeps identity
// in id and exact continuation state in sealed.
class FakeSession {
  id: string,
  history: string,
  owner: Provider,

  implements Session {
    function run<T>(self, request: Request<T>) -> T throws baml.errors.UnknownError {
      let turn = baml.llm.prompt_to_text(request.for_provider(self.owner).prompt);
      self.history = if (self.history == "") { turn } else { self.history + "\n" + turn };
      fake_value<T, string>(self.history)
    }
    function run_with_meta<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.UnknownError {
      Response<T> {
        value: self.run<T>(request),
        meta: Meta {
          provider: self.owner.display_name(),
          model: "fake",
          request_id: self.id,
          finish_reason: "stop",
          usage: null,
          attributes: {},
          raw: null,
        },
      }
    }
    function provider(self) -> Provider throws never { self.owner }
    function token(self) -> SessionToken throws never {
      SessionToken {
        provider: self.owner.display_name(),
        id: self.id,
        runtime: "fake-session",
        sealed: self.history,
      }
    }
    function cleanup(self) -> null throws never { null }
  }

  implements ForkableSession {
    function fork(self) -> ForkableSession throws baml.errors.UnknownError {
      FakeSession { id: self.id + "-fork", history: self.history, owner: self.owner }
    }
  }
}

class FakeSessionProvider {
  name: string,
  initial_id: string,

  implements Provider {
    function display_name(self) -> string throws never { self.name }
  }

  implements Sessions {
    function open_session(self, options: SessionOptions) -> Session
        throws baml.errors.UnknownError {
      FakeSession { id: self.initial_id, history: "", owner: self }
    }
    function resume_session(self, token: SessionToken) -> Session
        throws baml.errors.UnknownError {
      if (token.provider != self.name) {
        throw baml.errors.UnknownError {
          data: token,
          message: ["foreign session token owned by " + token.provider],
        };
      }
      FakeSession {
        id: token.id,
        history: token.sealed ?? "",
        owner: self,
      }
    }
  }
}
