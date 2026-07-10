// 20 — provider-stored conversations are Session resources.

class s20_ScriptedSession {
  id: string,
  history: string,
  owner: root.ai.Provider,

  implements root.ai.Session {
    function run<T>(self, request: root.ai.Request<T>) -> T throws baml.errors.UnknownError {
      // Session context wins: the request is always rebound to the session provider.
      let turn = baml.llm.prompt_to_text(request.for_provider(self.owner).prompt);
      self.history = if (self.history == "") { turn } else { self.history + "\n" + turn };
      baml.sap.parse<T>(self.history) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s20 session parse failed"] },
      }
    }

    function run_with_meta<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.UnknownError {
      root.ai.Response<T> {
        value: self.run<T>(request),
        meta: root.ai.Meta {
          provider: self.owner.display_name(), model: null, request_id: self.id,
          finish_reason: "stop", usage: null, attributes: {}, raw: null,
        },
      }
    }

    function provider(self) -> root.ai.Provider throws never { self.owner }
    function token(self) -> root.ai.SessionToken throws never {
      root.ai.SessionToken { provider: self.owner.display_name(), id: self.history }
    }
    function cleanup(self) -> null throws never { null }
  }

  implements root.ai.ForkableSession {
    function fork(self) -> root.ai.ForkableSession throws baml.errors.UnknownError {
      s20_ScriptedSession { id: self.id + "-fork", history: self.history, owner: self.owner }
    }
  }
}

class s20_ScriptedSessions {
  owner: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { self.owner }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Sessions {
    function open_session(self, options: root.ai.SessionOptions) -> root.ai.Session
        throws baml.errors.UnknownError {
      s20_ScriptedSession { id: "session-1", history: "", owner: self }
    }

    function resume_session(self, token: root.ai.SessionToken) -> root.ai.Session
        throws baml.errors.UnknownError {
      if (token.provider != self.owner) {
        throw baml.errors.UnknownError {
          data: token, message: ["foreign session token owned by " + token.provider],
        };
      }
      s20_ScriptedSession { id: "resumed", history: token.id, owner: self }
    }
  }
}
