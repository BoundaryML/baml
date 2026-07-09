// scenario 23 — barge-in: ../../../../../llm-provider/ideas/scenarios/23-barge-in/README.md
//
// LIBRARY / std-lib-author code — the scripted fakes the offline tier drives.
//
// The design (implement.baml) authors a full `LiveControl requires Realtime`
// capability (cancel + truncate-at-played-ms + update_session + a defaulted
// barge_in), a net-new InEvent/OutEvent duplex vocabulary, an app-owned
// PlaybackCursor that threads played-position + drops racing audio.delta frames,
// and two providers (OpenAIRealtime ms-truncate vs GeminiLive server-discard).
//
// What the REAL surface gives us (ns_ai/capabilities/realtime.baml):
//   - `LiveControl requires Realtime` is REAL, with exactly two methods:
//         cancel(self, io: Channel) -> null
//         truncate(self, io: Channel, played_ms: int) -> null
//     `played_ms` — the design's played-position argument — IS on the real
//     signature, so barge-in-at-heard-ms is expressible verbatim. There is NO
//     `update_session` and NO defaulted `barge_in` on the real interface, so this
//     file spells the atomic barge-in move (cancel THEN truncate, in that order)
//     directly in `run`, which is exactly where the interrupt protocol is the lesson.
//   - `Realtime.run(self, prompt, io: Channel) -> Transcript` and the string-feed
//     `Channel` (on/send/close) are REAL and used verbatim.
//   - `baml.errors.Unsupported` implements `RealtimeError`, so a non-steerable
//     provider is a typed degrade on the LiveControl channel (the dual-capability
//     negotiation point) with no honest fallback.
//
// BLOCKED: P8 audio side channel + typed duplex event vocabulary. The design's
//   InEvent/OutEvent grammar (audio.delta PCM frames, server-VAD SpeechStarted,
//   the played-position measured off the speaker via `baml.audio.play_and_measure`)
//   is net-new host surface that does NOT exist. The real `Channel` carries a bare
//   `string` feed, so this scenario runs in TEXT mode: the "played_ms" is a scripted
//   number (what a real PlaybackCursor would have measured) rather than a live speaker
//   reading, and the barge-in is scripted rather than driven off a real VAD event.
// BLOCKED: P8 update_session (mid-session voice/instructions mutation). The real
//   `LiveControl` has no `update_session` verb, so mid-call config mutation is not on
//   the real surface — the interrupt (cancel + truncate) half of the design is.
// BLOCKED: P8 provider divergence (OpenAI ms-truncate vs Gemini server-discard).
//   Modeling Gemini's truncate-as-no-op needs the same net-new event grammar; on the
//   real surface both would call the same two-method `LiveControl`, so the semantic
//   leak the design exposes is not reproducible here. The negotiation half (steerable
//   vs not) IS.


// The scripted server event feed one "session" plays. In TEXT mode a `delta` is a
// chunk of the assistant's in-flight response and `ms` is the played-position it
// advances (what a real PlaybackCursor would measure off the speaker). `barge_in`
// is the user talking over the model; `commit` is a turn that finished naturally.
class s23_TurnEvent {
    /// "delta" | "barge_in" | "commit"
    kind: string,
    /// delta text (the chunk the model is speaking), when kind == "delta".
    text: string?,
    /// ms this delta advanced the played position, when kind == "delta".
    ms: int?,
}


// A scripted offline provider that implements BOTH `Realtime` and `LiveControl` — the
// steerable voice provider. `run` drives the handed-in `Channel`, folding the scripted
// event feed into the durable `Transcript`. When a `barge_in` event arrives mid-turn it
// performs the design's atomic interrupt move over the SAME open `io`:
//     cancel(io)              — stop the in-flight response NOW (order matters:
//     truncate(io, played_ms) — cancel first to stop the firehose, THEN truncate at
//                               the played position so server memory matches what was
//                               HEARD, not what was generated).
// The transcript records the truncated partial + the interruption marker, then the next
// turn proceeds — proving barge-in semantics end to end.
class s23_ScriptedBargeIn {
    /// The scripted server event feed for this session.
    script: s23_TurnEvent[],

    implements baml.ai.Provider {}

    implements baml.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: baml.ai.Channel,
        ) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
            log.info("23 offline: session open, prompt = " + prompt);
            io.send("{\"type\":\"session.created\"}") catch (e) { _ => null };

            // Durable transcript record + in-flight (not-yet-committed) partial + the
            // played position of the current turn + the turn counter. A real provider
            // threads these through a PlaybackCursor; the real surface has no such type
            // (BLOCKED: P8), so we thread them as plain locals across the event feed.
            let acc = "";
            let buffer = "";
            let played_ms = 0;
            let turn = 1;

            for (let ev in self.script) {
                if (ev.kind == "delta") {
                    // The model speaks a chunk. It rides `io` (the side channel) and
                    // advances the in-flight partial + the played position.
                    io.send("{\"type\":\"response.output_text.delta\",\"delta\":\"" + (ev.text ?? "") + "\"}")
                        catch (e) { _ => null };
                    buffer = buffer + (ev.text ?? "");
                    played_ms = played_ms + (ev.ms ?? 0);
                } else if (ev.kind == "barge_in") {
                    // The user talked over the model. Barge-in NOW — the atomic move.
                    log.info(
                        "23 offline: BARGE-IN on turn " + turn.to_string()
                        + " — truncate at " + played_ms.to_string()
                        + "ms; partial_len=" + buffer.length().to_string()
                        + "; floor -> user",
                    );
                    self.cancel(io);              // stop the in-flight response first
                    self.truncate(io, played_ms); // then rewrite memory to heard-ms
                    // Record what was actually heard + the interruption; hand off the floor.
                    acc = acc + "[turn " + turn.to_string()
                        + " truncated@" + played_ms.to_string() + "ms: " + buffer
                        + " | interrupted]";
                    buffer = "";
                    played_ms = 0;
                    turn = turn + 1;
                    log.info("23 offline: turn " + turn.to_string() + " proceeds after floor handover");
                } else if (ev.kind == "commit") {
                    // A turn finished naturally (no interruption).
                    io.send("{\"type\":\"response.done\"}") catch (e) { _ => null };
                    acc = acc + "[turn " + turn.to_string() + ": " + buffer + "]";
                    buffer = "";
                    played_ms = 0;
                    turn = turn + 1;
                }
            }

            log.info("23 offline: transcript = " + acc);
            baml.ai.Transcript { text: acc }
        }
    }

    implements baml.ai.LiveControl {
        // response.cancel — stop the response currently being generated. Does NOT end
        // the session; `io` stays open and a new turn may follow.
        function cancel(
            self,
            io: baml.ai.Channel,
        ) -> null throws baml.errors.RealtimeError | baml.errors.UnknownError {
            log.info("23 offline: LiveControl.cancel — response.cancel");
            io.send("{\"type\":\"response.cancel\"}") catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["cancel: channel send failed"] },
            }
        }

        // conversation.item.truncate at the PLAYED position — rewrite server memory so
        // the assistant item reflects only what the user HEARD (played_ms), not what was
        // generated. On the real (OpenAI-shaped) surface this carries the audio_end_ms.
        function truncate(
            self,
            io: baml.ai.Channel,
            played_ms: int,
        ) -> null throws baml.errors.RealtimeError | baml.errors.UnknownError {
            log.info("23 offline: LiveControl.truncate — rewrite memory to " + played_ms.to_string() + "ms heard");
            io.send("{\"type\":\"conversation.item.truncate\",\"audio_end_ms\":" + played_ms.to_string() + "}")
                catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["truncate: channel send failed"] },
            }
        }
    }
}


// A plain voice provider: `Realtime` but NOT `LiveControl`. It can run a live session
// but cannot be steered/interrupted — the design's dual-capability contrast. An app
// that wires barge-in to this provider discovers the gap at negotiation time (a typed
// `Unsupported`), never faking an interrupt it cannot honestly perform.
class s23_PlainRealtime {
    transcript: string,

    implements baml.ai.Provider {}

    implements baml.ai.Realtime {
        function run(
            self,
            prompt: string,
            io: baml.ai.Channel,
        ) -> baml.ai.Transcript throws baml.errors.RealtimeError | baml.errors.UnknownError {
            log.info("23 offline: plain (un-steerable) session, prompt = " + prompt);
            baml.ai.Transcript { text: self.transcript + " <- " + prompt }
        }
    }
}


// A recording `Channel`: captures every raw frame the provider forwards, so the offline
// tier can ASSERT the barge-in wire moves actually happened (response.cancel followed by
// conversation.item.truncate at the played position). `send` mutates the shared `events`
// array in place, so a test holding the channel reads the frames back after `run`.
class s23_RecordingChannel {
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


// App-driven barge-in (push-to-talk): the app, not server VAD, decided the user is
// interrupting. It must negotiate `LiveControl` at runtime, because a client only
// guarantees `Realtime`, not steerability. A non-steerable provider is a typed
// `Unsupported` (which implements `RealtimeError`, so it is legal on this channel) —
// there is no honest fallback for an interrupt the backend cannot perform.
function s23_manual_barge_in(
    p: baml.ai.Provider,
    io: baml.ai.Channel,
    played_ms: int,
) -> string throws baml.errors.RealtimeError | baml.errors.UnknownError {
    match (p) {
        let lc: baml.ai.LiveControl => {
            // Same atomic move the auto-loop uses, but triggered by the UI.
            lc.cancel(io);
            lc.truncate(io, played_ms);
            "barged-in"
        },
        _ => throw baml.errors.Unsupported { message: "provider does not support LiveControl (barge-in)" },
    }
}
