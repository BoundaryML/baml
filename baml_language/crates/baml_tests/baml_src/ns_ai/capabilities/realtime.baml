interface Channel {
  function on(self, handler: (string) -> null throws never) -> null throws never
  function send(self, data: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
}

class RecordingChannel {
  frames: string[],

  implements Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io {
      self.frames.push(data);
      null
    }
    function close(self) -> null throws never { null }
  }
}

class NullChannel {
  implements Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io { null }
    function close(self) -> null throws never { null }
  }
}

class TranscriptDelta { text: string }
class UserSpeechStarted { played_ms: int }
class LiveClosed {}
type LiveEvent = TranscriptDelta | UserSpeechStarted | LiveClosed
type LiveEventStream = LiveEvent[]

interface Live {
  function events(self) -> LiveEventStream throws never
  function cancel_response(self) -> null throws baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError
  function close(self) -> null throws never
  function cleanup(self) -> null throws never
}

class BufferedLive {
  items: LiveEvent[],
  channel: Channel,

  implements Live {
    function events(self) -> LiveEventStream throws never { self.items }
    function cancel_response(self) -> null throws baml.errors.UnknownError { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError { null }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.close() }
  }
}

function collect_live_text(live: Live) -> string throws never {
  let text = "";
  // Materialize the virtual return at its declared alias before iterating.
  let events: LiveEventStream = live.events();
  for (let event in events) {
    match (event) {
      let delta: TranscriptDelta => { text = text + delta.text; },
      _ => {},
    }
  }
  text
}

interface Realtime requires Provider {
  function open_live<T>(self, request: Request<T>, channel: Channel) -> Live
    throws baml.errors.UnknownError
}

function open_live<T>(request: Request<T>, channel: Channel) -> Live throws baml.errors.UnknownError {
  let realtime = match (request.provider) {
    let capability: Realtime => capability,
    _ => throw baml.errors.UnknownError { data: request.provider, message: ["provider cannot open live sessions"] },
  };
  realtime.open_live<T>(request, channel)
}
