// 22 — realtime voice. The connection is a Live resource; Channel is only the
// input/output adapter passed when that resource is opened.

class s22_RecordingChannel {
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

class s22_Live {
  script: root.ai.LiveEvent[],
  channel: root.ai.Channel,
  cancelled: bool,
  truncated_at: int?,

  implements root.ai.Live {
    function events(self) -> root.ai.LiveEventStream throws never { self.script }
    function cancel_response(self) -> null throws baml.errors.UnknownError {
      self.cancelled = true;
      self.channel.send(`{"type":"response.cancel"}`) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["cancel send failed"] },
      }
    }
    function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError {
      self.truncated_at = played_ms;
      self.channel.send(`{"type":"conversation.item.truncate","audio_end_ms":${played_ms}}`) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["truncate send failed"] },
      }
    }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.close() }
  }
}

class ScriptedRealtime {
  transcript: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "scripted-realtime" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Realtime {
    function open_live<T>(
      self,
      request: root.ai.Request<T>,
      channel: root.ai.Channel,
    ) -> root.ai.Live throws baml.errors.UnknownError {
      channel.send(`{"type":"session.created"}`) catch (e) { _ => null };
      channel.send(`{"type":"response.output_text.delta","delta":"${self.transcript}"}`) catch (e) { _ => null };
      s22_Live {
        script: [root.ai.TranscriptDelta { text: self.transcript }, root.ai.LiveClosed {}],
        channel: channel, cancelled: false, truncated_at: null,
      }
    }
  }
}

class NullChannel {
  implements root.ai.Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io { null }
    function close(self) -> null throws never { null }
  }
}

function s22_collect_text(live: root.ai.Live) -> string throws never {
  let text = "";
  for (let event in live.events()) {
    match (event) {
      let delta: root.ai.TranscriptDelta => { text = text + delta.text; },
      _ => null,
    }
  }
  text
}
