// 19 — fork / branch.
//
// App-owned history is ordinary root.ai.ChatMessage[] data. Provider-owned
// history is a Session resource; code that needs branching asks specifically
// for ForkableSession.

class s19_Branch {
  id: string,
  messages: root.ai.ChatMessage[],
  reply: string,
}

function s19_text_of(message: root.ai.ChatMessage) -> string throws never {
  message.parts.map((part) -> { part.text ?? "" }).join("")
}

function s19_fork(
  prefix: root.ai.ChatMessage[],
  continuation: string,
) -> root.ai.ChatMessage[] throws never {
  prefix.concat([root.ai.ChatMessage.user(continuation)])
}

function s19_fork_at(
  history: root.ai.ChatMessage[],
  at: int,
  continuation: string,
) -> root.ai.ChatMessage[] throws never {
  let start = if (at < 0) { 0 } else { at };
  let cut = if (start > history.length()) { history.length() } else { start };
  history.slice(0, cut).concat([root.ai.ChatMessage.user(continuation)])
}

class s19_ForkEcho {
  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s19-fork-echo" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let rendered = baml.llm.prompt_to_text(request.prompt);
      let value = baml.sap.parse<T>(rendered) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s19 fixture parse failed"] },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: self.display_name(), model: null, request_id: null,
          finish_reason: "stop", usage: null, attributes: {}, raw: null,
        },
      }
    }
  }
}

// A small provider-owned session used to demonstrate the page-6 resource shape.
class s19_ForkSession {
  id: string,
  history: string,
  owner: root.ai.Provider,

  implements root.ai.Session {
    function run<T>(self, request: root.ai.Request<T>) -> T throws baml.errors.UnknownError {
      let turn = baml.llm.prompt_to_text(request.for_provider(self.owner).prompt);
      self.history = if (self.history == "") { turn } else { self.history + "\n" + turn };
      baml.sap.parse<T>(self.history) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["s19 session parse failed"] },
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
      root.ai.SessionToken { provider: self.owner.display_name(), id: self.id }
    }
    function cleanup(self) -> null throws never { null }
  }

  implements root.ai.ForkableSession {
    function fork(self) -> root.ai.ForkableSession throws baml.errors.UnknownError {
      s19_ForkSession { id: self.id + "-fork", history: self.history, owner: self.owner }
    }
  }
}

class s19_SessionProvider {
  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s19-sessions" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Sessions {
    function open_session(self, options: root.ai.SessionOptions) -> root.ai.Session
        throws baml.errors.UnknownError {
      s19_ForkSession { id: "root", history: "", owner: self }
    }

    function resume_session(self, token: root.ai.SessionToken) -> root.ai.Session
        throws baml.errors.UnknownError {
      if (token.provider != self.display_name()) {
        throw baml.errors.UnknownError { data: token, message: ["foreign session token"] };
      }
      s19_ForkSession { id: token.id, history: "", owner: self }
    }
  }
}

class s19_PlainSession {
  owner: root.ai.Provider,

  implements root.ai.Session {
    function run<T>(self, request: root.ai.Request<T>) -> T throws baml.errors.UnknownError {
      root.ai.run<T>(request.for_provider(self.owner)) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["plain session run failed"] },
      }
    }
    function run_with_meta<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.UnknownError {
      root.ai.run_with_meta<T>(request.for_provider(self.owner)) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["plain session run failed"] },
      }
    }
    function provider(self) -> root.ai.Provider throws never { self.owner }
    function token(self) -> root.ai.SessionToken throws never {
      root.ai.SessionToken { provider: self.owner.display_name(), id: "plain" }
    }
    function cleanup(self) -> null throws never { null }
  }
}
