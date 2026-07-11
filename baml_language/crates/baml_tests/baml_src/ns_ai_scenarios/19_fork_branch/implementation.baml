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
      let value = root.ai.fixture_value<T, string>(rendered);
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
