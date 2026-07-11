interface Channel {
  function on(self, handler: (string) -> null throws never) -> null throws never
  function send(self, data: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
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
