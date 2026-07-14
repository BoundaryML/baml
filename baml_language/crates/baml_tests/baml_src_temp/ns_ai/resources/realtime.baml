interface Channel {
  function send(self, frame: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
}

class RecordingChannel {
  frames: string[],
  implements Channel {
    function send(self, frame: string) -> null throws never { self.frames.push(frame); null }
    function close(self) -> null throws never { null }
  }
}

interface LiveEvent {
  function kind(self) -> string throws never
}

class TranscriptDelta {
  text: string,
  implements LiveEvent {
    function kind(self) -> string throws never { "transcript_delta" }
  }
}

class UserSpeechStarted {
  played_ms: int,
  implements LiveEvent {
    function kind(self) -> string throws never { "user_speech_started" }
  }
}

class LiveClosed {
  implements LiveEvent {
    function kind(self) -> string throws never { "closed" }
  }
}

interface Live {
  function events(self) -> LiveEvent[] throws never
  function cancel_response(self) -> null throws baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError
  function close(self) -> null throws never
}

interface RealtimeProvider requires Provider {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> Live
    throws baml.errors.UnknownError
}

class BufferedLive {
  items: LiveEvent[],
  channel: Channel,
  implements Live {
    function events(self) -> LiveEvent[] throws never { self.items }
    function cancel_response(self) -> null throws never { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws never { null }
    function close(self) -> null throws never { self.channel.close() }
  }
}

class FakeRealtimeProvider {
  transcript: string,
  implements Provider {
    function name(self) -> string throws never { "fake-realtime" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
  implements RealtimeProvider {
    function open_live<T>(self, task: Task<T>, channel: Channel) -> Live throws never {
      channel.send(baml.json.stringify({ "type": "session.created" })) catch (e) { _ => null };
      BufferedLive {
        items: [TranscriptDelta { text: self.transcript }, LiveClosed {}],
        channel: channel,
      }
    }
  }
}

class OpenAiRealtime {
  model: string,
  voice: string,
  api_key: string,
  implements Provider {
    function name(self) -> string throws never { "openai-realtime" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }
}

implements RealtimeProvider for OpenAiRealtime {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> Live
      throws baml.errors.UnknownError {
    let ws = baml.ws.connect(
      "wss://api.openai.com/v1/realtime?model=" + self.model,
      { "Authorization": "Bearer " + self.api_key },
    ) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["realtime connect failed"] },
    };
    ws.send(baml.json.stringify({
      "type": "conversation.item.create",
      "item": {
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": baml.llm.prompt_to_text(task.prompt) }],
      },
    })) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["realtime send failed"] },
    };
    ws.send(baml.json.stringify({
      "type": "response.create",
      "response": { "output_modalities": ["text"] },
    })) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["realtime response.create failed"] },
    };
    let text = "";
    let done = false;
    while (!done) {
      let next = ws.next() catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["realtime receive failed"] },
      };
      match (next) {
        null => { done = true; },
        let raw: string => {
          channel.send(raw) catch (e) { _ => null };
          let event = baml.json.parse(raw) catch (e) {
            _ => throw baml.errors.UnknownError { data: raw, message: ["realtime event JSON failed"] },
          };
          let kind = baml.json.path_or(event, ".type", "");
          if (kind == "response.output_text.delta" || kind == "response.text.delta") {
            text = text + baml.json.path_or(event, ".delta", "");
          }
          if (kind == "response.done" || kind == "response.completed") { done = true; }
        },
      }
    }
    ws.close();
    BufferedLive {
      items: [TranscriptDelta { text: text }, LiveClosed {}],
      channel: channel,
    }
  }
}

function collect_live_text(live: Live) -> string throws never {
  let text = "";
  for (let event in live.events()) {
    match (event) {
      let delta: TranscriptDelta => { text = text + delta.text; },
      _ => {},
    }
  }
  text
}
