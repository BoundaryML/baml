// scenario 22 — realtime voice: ../../../../../llm-provider/ideas/scenarios/22-realtime-voice/README.md
// (also 23 barge-in, 26 transports). The live WebSocket transport is exercised
// by the Rust `ai_realtime` suite (OPENAI_API_KEY-gated); here the offline
// tier covers negotiation + the Foo$live companion against a scripted fake.

/// Offline Realtime provider: echoes what a live session would transcribe.
class ScriptedRealtime {
    transcript: string,

    implements baml.ai.Provider {}

    implements baml.ai.Realtime {
        function run(self, prompt: string, io: baml.ai.Channel) -> baml.ai.Transcript
            throws baml.errors.RealtimeError | baml.errors.UnknownError {
            baml.ai.Transcript { text: self.transcript + " <- " + prompt }
        }
    }
}

/// A Channel that swallows everything (the offline stand-in for a socket).
class NullChannel {
    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            null
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}

client<llm> DeadVoice {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function VoiceChat(instructions: string) -> string {
  client DeadVoice
  prompt #"You are a voice assistant: {{ instructions }}"#
}

test "22: Foo$live drives a swapped-in Realtime provider" {
    let t = VoiceChat$live("be brief", NullChannel {}, client = ScriptedRealtime { transcript: "session done" }) catch (e) {
        _ => baml.ai.Transcript { text: "live failed" },
    };
    assert.is_true(t.text.includes("session done"))
}

test "22: Foo$live on a non-realtime client is a typed Unsupported" {
    let t = VoiceChat$live("be brief", NullChannel {}) catch (e) {
        let u: baml.errors.Unsupported => baml.ai.Transcript { text: "unsupported" },
        _ => baml.ai.Transcript { text: "wrong error" },
    };
    assert.equal(t.text, "unsupported")
}

test "23: LiveControl is negotiated on top of Realtime" {
    let p: baml.ai.Provider = ScriptedRealtime { transcript: "x" };
    let label = match (p) {
        let lc: baml.ai.LiveControl => "live-control",
        let r: baml.ai.Realtime => "realtime-only",
        _ => "neither",
    };
    assert.equal(label, "realtime-only")
}
