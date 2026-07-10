// 26 — transport is an implementation detail of a provider. Capabilities say
// what the application may do; a separate descriptive interface says which wire
// and state location this particular provider uses.

enum s26_Wire { Http, HttpSse, WebSocket, WebRtc }
enum s26_State { ClientOwned, ServerStoredById, ServerSession }

interface s26_TransportInfo requires root.ai.Provider {
  function wire(self) -> s26_Wire throws never
  function state_location(self) -> s26_State throws never
}

function s26_response<T>(reply: string, name: string) -> root.ai.Response<T>
    throws baml.errors.UnknownError {
  let value = baml.sap.parse<T>(reply) catch (e) {
    _ => throw baml.errors.UnknownError { data: e, message: ["s26 fixture parse failed"] },
  };
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
  implements s26_TransportInfo {
    function wire(self) -> s26_Wire throws never { s26_Wire.Http }
    function state_location(self) -> s26_State throws never { s26_State.ClientOwned }
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
  implements s26_TransportInfo {
    function wire(self) -> s26_Wire throws never { s26_Wire.HttpSse }
    function state_location(self) -> s26_State throws never { s26_State.ClientOwned }
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
  implements s26_TransportInfo {
    function wire(self) -> s26_Wire throws never { s26_Wire.WebSocket }
    function state_location(self) -> s26_State throws never { s26_State.ServerStoredById }
  }
}

class s26_RecordingChannel {
  events: string[],

  implements root.ai.Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io {
      self.events.push(data);
      null
    }
    function close(self) -> null throws never { null }
  }
}

class s26_Live {
  reply: string,
  channel: root.ai.Channel,

  implements root.ai.Live {
    function events(self) -> root.ai.LiveEventStream throws never {
      [root.ai.TranscriptDelta { text: self.reply }, root.ai.LiveClosed {}]
    }
    function cancel_response(self) -> null throws baml.errors.UnknownError { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError { null }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.close() }
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
      s26_Live { reply: self.reply, channel: channel }
    }
  }
  implements s26_TransportInfo {
    function wire(self) -> s26_Wire throws never { s26_Wire.WebSocket }
    function state_location(self) -> s26_State throws never { s26_State.ServerSession }
  }
}
