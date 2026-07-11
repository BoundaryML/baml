// 26 — transport is an implementation detail of a provider. Capabilities say
// what the application may do; a separate descriptive interface says which wire
// and state location this particular provider uses.

function s26_response<T>(reply: string, name: string) -> root.ai.Response<T>
    throws baml.errors.UnknownError {
  let value = root.ai.fixture_value<T, string>(reply);
  root.ai.Response<T> {
    value: value,
    meta: root.ai.Meta {
      provider: name, model: null, request_id: null, finish_reason: "stop",
      usage: null, attributes: {}, raw: null,
    },
  }
}

class s26_HttpTransport {
  reply: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s26-http" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      s26_response<T>(self.reply, self.display_name())
    }
  }
  implements root.ai.TransportInfo {
    function transport(self) -> root.ai.TransportKind throws never { root.ai.TransportKind.Http }
    function state_ownership(self) -> root.ai.StateOwnership throws never { root.ai.StateOwnership.ClientOwned }
  }
}

class s26_StreamTransport {
  reply: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s26-sse" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      s26_response<T>(self.reply, self.display_name())
    }
  }
  implements root.ai.Streaming {
    function stream<TPartial, T>(self, request: root.ai.Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      // Pure BAML cannot manufacture the host stream object. Live OpenAi below
      // proves the same request over native SSE.
      throw baml.errors.UnknownError { data: null, message: ["offline stream fixture has no host stream"] }
    }
  }
  implements root.ai.TransportInfo {
    function transport(self) -> root.ai.TransportKind throws never { root.ai.TransportKind.HttpSse }
    function state_ownership(self) -> root.ai.StateOwnership throws never { root.ai.StateOwnership.ClientOwned }
  }
}

class s26_WarmSocketTransport {
  reply: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s26-warm-websocket" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      s26_response<T>(self.reply, self.display_name())
    }
  }
  implements root.ai.TransportInfo {
    function transport(self) -> root.ai.TransportKind throws never { root.ai.TransportKind.WebSocket }
    function state_ownership(self) -> root.ai.StateOwnership throws never { root.ai.StateOwnership.ServerStoredById }
  }
}

class s26_RealtimeTransport {
  reply: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s26-realtime-websocket" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements root.ai.Realtime {
    function open_live<T>(
      self,
      request: root.ai.Request<T>,
      channel: root.ai.Channel,
    ) -> root.ai.Live throws baml.errors.UnknownError {
      channel.send(`{"type":"response.output_text.delta","delta":"${self.reply}"}`) catch (e) { _ => null };
      root.ai.BufferedLive {
        items: [root.ai.TranscriptDelta { text: self.reply }, root.ai.LiveClosed {}],
        channel: channel,
      }
    }
  }
  implements root.ai.TransportInfo {
    function transport(self) -> root.ai.TransportKind throws never { root.ai.TransportKind.WebSocket }
    function state_ownership(self) -> root.ai.StateOwnership throws never { root.ai.StateOwnership.ServerSession }
  }
}
