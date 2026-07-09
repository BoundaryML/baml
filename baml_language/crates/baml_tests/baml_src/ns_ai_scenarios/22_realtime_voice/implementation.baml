// scenario 22 — realtime voice: ../../../../../llm-provider/ideas/scenarios/22-realtime-voice/README.md
//
// LIBRARY / std-lib-author code — the custom fakes the offline tier drives.
//
// The design (implement.baml) authors two full realtime providers (OpenAIRealtime,
// GeminiLive) behind the `Realtime` capability, plus a large net-new duplex wire
// vocabulary. On the REAL surface almost all of that is already stdlib or is
// deliberately narrower, so this file holds only what the OFFLINE tier needs: a
// scripted provider that implements `Realtime`, a recording `Channel`, and a null
// `Channel`. The LIVE tier (usage.baml `testset "integ-test-22-realtime-voice"`)
// uses the real `baml.ai.OpenAiRealtime` over `baml.ws`.
//
// NOTE (shared namespace): `ScriptedRealtime` and `NullChannel` are ALSO reused,
// read-only, by scenario 24 (realtime tools) — keep their names + field shapes
// (`ScriptedRealtime { transcript: string }`, `NullChannel {}`).
//
// What the real surface gives us vs. the design's net-new surface:
//   - `Realtime requires Provider` with `run(self, prompt, io: Channel) -> Transcript`
//     is REAL (ns_ai/capabilities/realtime.baml) — used verbatim. The marker payoff
//     holds: a realtime provider implements ONLY `Realtime` (no degenerate `call`).
//   - The pass-in `io: Channel` handed at the `run`/`$live` call (never a client
//     field) is REAL. The config-only client + call-time io split falls straight out.
//   - The `$live` companion (`Foo$live(args, io, client = ...)` -> drive_live) is REAL
//     and negotiates `Realtime`, erroring `Unsupported` with no honest degrade.
//
// BLOCKED: P8 audio frames / typed duplex event vocabulary. The design's whole
//   InEvent/OutEvent + InKind/OutKind grammar (OutputAudio PCM chunks, AppendAudio
//   mic frames, server-VAD SpeechStarted, Commit/Truncate barge-in) is net-new host
//   surface that does NOT exist. The real `Channel` carries a bare `string` event
//   feed (`on`/`send`/`close`) and `Transcript` is `{ text }`. So audio I/O and the
//   typed event union are host-surface-blocked; the scenario is exercised in TEXT
//   mode over the string channel — exactly what the Rust `ai_realtime.rs` tier and
//   the live `OpenAiRealtime` provider prove.
// BLOCKED: P8 server-VAD vs push-to-talk (`manual_turn`). The design reshapes the
//   body of `run` on a `manual_turn` boolean; the real `OpenAiRealtime` has no such
//   field (model?/voice/api_key only) and drives one text turn. Turn-mode config is
//   therefore not on the real surface.


// A scripted offline Realtime provider. Implements ONLY the `Provider` marker +
// `Realtime` — it has NO `call` (no HttpProvider), which is the marker payoff the
// design leans on: a realtime provider IS a Provider (composable, negotiable)
// without faking request/response. `run` drives the handed-in `Channel` (forwards a
// scripted text event feed, just like a real provider forwards raw wire frames),
// ACCUMULATES the deltas into a transcript, and returns the durable `Transcript`
// record when the "session" ends. The audio side channel is BLOCKED (P8) — this is
// a text-mode stand-in for the duplex the live tier proves.
//
// Shape (`{ transcript: string }`) is fixed by scenario 24's reuse — keep it.
class ScriptedRealtime {
    /// The line the "model" speaks this session (also its config-only seed).
    transcript: string,

    implements baml.ai.Provider {}

    implements baml.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: baml.ai.Channel,
        ) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
            // Session open — the pass-in `io` is the live duplex, handed in at the
            // call, never a client field.
            log.info("22 offline: session open, prompt = " + prompt);
            io.send("{\"type\":\"session.created\"}") catch (e) { _ => null };

            // One turn: forward the output-text delta to the app over `io` (the side
            // channel) AND fold it into the durable transcript. Audio deltas (the
            // design's OutputAudio PCM) are BLOCKED (P8) — text only here.
            let acc = "";
            io.send("{\"type\":\"response.output_text.delta\",\"delta\":\"" + self.transcript + "\"}")
                catch (e) { _ => null };
            acc = acc + self.transcript;
            io.send("{\"type\":\"response.done\"}") catch (e) { _ => null };

            // Transcript tail — the accumulated record run() returns (NOT the audio,
            // which already left over `io`).
            log.info("22 offline: transcript tail = " + acc);
            baml.ai.Transcript { text: acc + " <- " + prompt }
        }
    }
}


// A recording `Channel`: captures every raw event the provider forwards over the
// duplex (mirrors the Rust `ai_realtime.rs` RecordingChannel). The app gets one of
// these from its host audio I/O and hands it to `$live`; the provider drives it. In
// the design this is the host-backed AudioChannel bound to a mic + speaker — BLOCKED
// (P8) here, so we record the string event feed instead of playing PCM.
class s22_RecordingChannel {
    events: string[],

    implements baml.ai.Channel {
        function on(self, handler: (string) -> void) -> null throws never {
            null
        }
        function send(self, data: string) -> null throws baml.errors.Io {
            self.events.push(data);
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}


// A `Channel` that swallows everything — the offline stand-in for a socket when a
// test only needs the `Transcript`, not the event feed. Shape (`NullChannel {}`) is
// reused read-only by scenario 24 — keep the name.
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
