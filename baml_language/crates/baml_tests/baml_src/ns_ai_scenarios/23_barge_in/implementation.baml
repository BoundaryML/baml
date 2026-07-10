// 23 — barge-in. Cancellation and audio truncation target the specific Live
// resource, so they cannot accidentally steer another provider session.

class s23_RecordingChannel {
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

class s23_ScriptedLive {
  script: root.ai.LiveEvent[],
  channel: root.ai.Channel,

  implements root.ai.Live {
    function events(self) -> root.ai.LiveEventStream throws never { self.script }
    function cancel_response(self) -> null throws baml.errors.UnknownError {
      self.channel.send(`{"type":"response.cancel"}`) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["cancel failed"] },
      }
    }
    function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError {
      self.channel.send(`{"type":"conversation.item.truncate","audio_end_ms":${played_ms}}`) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["truncate failed"] },
      }
    }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.close() }
  }
}

class s23_ScriptedBargeIn {
  script: root.ai.LiveEvent[],

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s23-realtime" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Realtime {
    function open_live<T>(
      self,
      request: root.ai.Request<T>,
      channel: root.ai.Channel,
    ) -> root.ai.Live throws baml.errors.UnknownError {
      s23_ScriptedLive { script: self.script, channel: channel }
    }
  }
}

class s23_PlainProvider {
  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s23-plain" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }
}

class s23_BargeInResult {
  transcript: string,
  interrupted: bool,
}

function s23_drive_live(live: root.ai.Live) -> s23_BargeInResult throws baml.errors.UnknownError {
  let transcript = "";
  let interrupted = false;
  for (let event in live.events()) {
    match (event) {
      let delta: root.ai.TranscriptDelta => { transcript = transcript + delta.text; },
      let speech: root.ai.UserSpeechStarted => {
        // Order matters: stop generation before rewriting memory to what was heard.
        live.cancel_response();
        live.truncate_assistant_audio(speech.played_ms);
        transcript = transcript + " [interrupted@" + speech.played_ms.to_string() + "ms] ";
        interrupted = true;
      },
      _ => null,
    }
  }
  s23_BargeInResult { transcript: transcript, interrupted: interrupted }
}

function s23_manual_barge_in(live: root.ai.Live, played_ms: int) -> string
    throws baml.errors.UnknownError {
  live.cancel_response();
  live.truncate_assistant_audio(played_ms);
  "barged-in"
}
