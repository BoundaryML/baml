// =============================================================================
// implement.baml — Barge-in, interruption & mid-session mutation
//
// Library / std-lib author code. Builds on the canonical primitives from
// _conventions.md (Provider, Realtime, Channel, InEvent/OutEvent).
//
// The teaching point of this scenario is that a live realtime session is NOT
// just "frames flowing out and frames flowing in." Three control moves ride the
// SAME open Channel WHILE a response is in flight:
//
//   1. response.cancel              — stop generating the current response now
//   2. conversation.item.truncate   — tell the server "the assistant item was
//                                      only heard up to <ms>"; rewrites memory
//   3. session.update               — change voice/instructions/tools mid-call
//
// All three are events the CALLER sends INTO `io` (OutEvent), correlated with
// events it RECEIVES (InEvent). The hard parts the model must absorb:
//   - played-position bookkeeping (truncate at ms ACTUALLY HEARD, not generated)
//   - races: audio.delta still in flight after we decided to cancel
//   - cancel (stop one response) vs close (end the whole session)
//   - provider divergence: OpenAI truncates by played-ms; Gemini Live discards
//     queued audio server-side and emits an interruption flag — no truncate verb.
//
// The canonical `Channel` (on/send/close) is intentionally minimal. This file
// shows two ways the proposal can carry barge-in over it:
//   (A) as OutEvent variants the app sends raw (leaky, provider-specific), and
//   (B) as a net-new `LiveControl` capability interface that gives the provider
//       a typed place to OWN the cancel/truncate/update encoding + bookkeeping.
// (B) is what makes barge-in portable; (A) is what the bare Channel forces.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. The event vocabulary on the Channel.
//
//    _conventions.md declares `Channel.on(handler: (InEvent) -> void)` and
//    `Channel.send(data: OutEvent)`. It does NOT pin what InEvent/OutEvent are.
//    Barge-in forces us to pin them. These are NET-NEW: the canonical Channel
//    is event-agnostic; this scenario is the first that needs the event algebra.
// -----------------------------------------------------------------------------

// Inbound: what the server pushes at us over the duplex link.
class InEvent {
  kind: InKind
  // Set on Audio: a chunk of generated assistant audio (base64 PCM). The client
  // must PLAY this and remember how many ms it actually pushed to the speaker.
  audio: string?
  // Set on Audio / ItemDone / ResponseDone: the server's id for the assistant
  // conversation item this audio belongs to. truncate() must name THIS id.
  item_id: string?
  // Set on ResponseDone: "completed" | "cancelled" | "interrupted".
  status: string?
  // Set on SpeechStarted: the server-VAD signal that the USER began talking.
  // This is the barge-in trigger when the server is doing turn detection.
  // (nothing extra to carry — the bare presence of the kind is the signal)
}

enum InKind {
  SpeechStarted    // input_audio_buffer.speech_started — user started talking
  Audio            // response.audio.delta — a chunk of assistant audio
  ItemDone         // response.output_item.done — an assistant item finished
  ResponseDone     // response.done — carries status: completed | cancelled
  Interrupted      // Gemini-style: server discarded its own queued audio
}

// Outbound: what the client sends back over the duplex link. The barge-in control
// moves live here. A provider's `encode` (see LiveControl) turns the high-level
// move into one of these tagged frames; a raw app can also build them directly.
class OutEvent {
  kind: OutKind
  // Set on AudioAppend: a chunk of the user's microphone audio (base64 PCM).
  audio: string?
  // Set on Truncate: the assistant item being truncated, and the played position.
  item_id: string?
  audio_end_ms: int?
  content_index: int?
  // Set on SessionUpdate: a JSON patch of session fields (voice/instructions/...).
  session_patch: string?
}

enum OutKind {
  AudioAppend      // input_audio_buffer.append — push user mic audio
  Cancel           // response.cancel — stop the in-flight response
  Truncate         // conversation.item.truncate — rewrite memory to heard-ms
  SessionUpdate    // session.update — mutate voice/instructions/tools live
  ResponseCreate   // response.create — ask the model to produce a new response
}


// -----------------------------------------------------------------------------
// 1. Net-new capability: LiveControl.
//
//    `Realtime.run(prompt, io)` (canonical) drives the duplex but says nothing
//    about cancel/truncate/update. Those are PROVIDER-SPECIFIC encodings with
//    PROVIDER-SPECIFIC semantics (OpenAI ms-truncate vs Gemini server-discard),
//    so they cannot be expressed once for all providers over the bare Channel.
//
//    LiveControl is the typed seam. It `requires Realtime` (and therefore
//    Provider). A provider that can be steered mid-session implements it; the
//    app calls these methods instead of hand-building OutEvents. The provider
//    OWNS: which wire frames to emit, how to name the item, and — critically —
//    what truncate even MEANS on this backend.
//
//    NOTE the shape: these take the SAME `io: Channel` the session is running on
//    (pass-in), because the control plane and the data plane are one connection.
//    There is no second handle. This mirrors §8's "steer/interrupt over the same
//    io" claim and OQ#2 (io is a parameter of the capability method).
// -----------------------------------------------------------------------------

// NET-NEW capability-error interface. LiveControl is a capability BEYOND
// call/stream/realtime/tools (a mid-session control plane), so per the error
// model it gets its own `baml.errors.<Cap>Error` interface with classifiers a
// caller can triage without knowing the concrete class. Flagged net-new in
// evaluation.md (b6). A control frame can fail to reach the server (network), or
// be rejected by the backend (unsupported field / wrong state); the classifiers
// let the app branch without pinning the concrete provider error.
interface baml.errors.LiveControlError {
  function is_network_error(self) -> bool
  function is_rejected(self) -> bool   // backend refused the control frame
}

// Because LiveControl is net-new, the canonical capability-negotiation error
// `baml.errors.Unsupported` (which already implements the four CORE capability
// interfaces) must also implement THIS interface to remain legal on the
// LiveControl channel when a `_` arm rejects a non-steerable provider. Extending
// it here is itself a finding: every net-new capability error must retro-fit
// Unsupported, or the `_`-arm reject can't typecheck (see evaluation b6/c).
extend baml.errors.Unsupported {
  implements baml.errors.LiveControlError {
    function is_network_error(self) -> bool { return false; }
    function is_rejected(self)      -> bool { return false; }
  }
}

interface LiveControl requires Realtime {
  // Stop the response currently being generated. Does NOT end the session.
  // After this, `io` stays open; a new response may follow.
  function cancel(self, io: Channel) -> null
      throws baml.ExtendUnknownError<baml.errors.LiveControlError>

  // Rewrite server-side memory so the assistant item reflects only what the user
  // HEARD. `heard_ms` is the played position the caller tracked. Providers that
  // truncate by ms encode it; providers that don't (Gemini) make this a no-op
  // because the server already discarded the unheard tail on its own.
  function truncate(self, io: Channel, item_id: string, heard_ms: int)
      -> null throws baml.ExtendUnknownError<baml.errors.LiveControlError>

  // Mutate live session config (voice/instructions/temperature/tools/VAD) with
  // no reconnect. `patch` is a JSON object of the fields to change.
  function update_session(self, io: Channel, patch: string) -> null
      throws baml.ExtendUnknownError<baml.errors.LiveControlError>

  // Barge-in is cancel+truncate as ONE atomic intent (the background's core move).
  // Defaulted so every LiveControl provider inherits the correct ordering and the
  // app writes one call. Order matters: cancel first (stop the firehose), THEN
  // truncate at the played position (so memory matches what was heard). A
  // provider can override if its backend wants the opposite order or a fused frame.
  function barge_in(self, io: Channel, item_id: string, heard_ms: int)
      -> null throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
    self.cancel(io);
    self.truncate(io, item_id, heard_ms);
  }
}


// -----------------------------------------------------------------------------
// 2. PlaybackCursor — the played-position bookkeeping the background calls out as
//    "bookkeeping no HTTP API ever needed."
//
//    This is a pure value, NOT a provider. It tracks the two things the truncate
//    decision needs that the server cannot know:
//      - which assistant item is currently playing (item_id), and
//      - how many ms of it actually reached the speaker (played_ms),
//    AND it gates the race: once we decide to interrupt we record a generation
//    boundary so audio.delta frames that arrive AFTER are discarded, not played.
//
//    Net-new: nothing in the canonical set tracks playback. The app owns it
//    because only the app's audio output device knows the true played position.
// -----------------------------------------------------------------------------

class PlaybackCursor {
  // The assistant item we are currently rendering, or null between responses.
  item_id: string?
  // Milliseconds of `item_id` actually pushed to the speaker so far.
  played_ms: int
  // Monotonic counter bumped on every interrupt. Audio frames are tagged with the
  // generation that was live when they were requested; a frame from an older
  // generation that arrives late is dropped (the race fix).
  generation: int
  // True between "we sent cancel" and "we saw response.done cancelled". While
  // true, incoming Audio frames are discarded instead of played.
  interrupting: bool

  function fresh() -> PlaybackCursor {
    PlaybackCursor { item_id: null, played_ms: 0, generation: 0, interrupting: false }
  }

  // Called when an assistant Audio frame arrives. Returns the cursor advanced by
  // however many ms the speaker actually accepted — UNLESS we are mid-interrupt,
  // in which case the frame is discarded (the race) and played_ms is frozen.
  function on_audio(self, ev: InEvent) -> PlaybackCursor
      throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
    if (self.interrupting) {
      // Race: this delta was already in flight when we decided to cancel. The
      // user never heard it. Drop it; do NOT advance played_ms.
      return self;
    }
    // A new item began playing — reset the played counter to this item.
    let same_item = match (ev.item_id) {
      let id: string => (self.item_id == id),
      null           => false,
    };
    let base_ms  = if (same_item) { self.played_ms } else { 0 };
    let base_id  = match (ev.item_id) { let id: string => id, null => self.item_id };
    // $rust_io_function: push the bytes to the audio device and return the ms the
    // device actually buffered/played (accounts for the playback buffer). THIS is
    // the number truncate needs — generated-ms would over-count.
    let pushed_ms = baml.audio.play_and_measure(match (ev.audio) { let a: string => a, null => "" });
    PlaybackCursor {
      item_id: base_id,
      played_ms: base_ms + pushed_ms,
      generation: self.generation,
      interrupting: false,
    }
  } catch (e) {
    // The audio device's foreign Io is not a meaningful classified realtime error
    // here — normalize it into the universal wrapper on the RealtimeError channel.
    _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "audio playback failed");
  }

  // Caller decided to interrupt (server VAD said speech_started, or push-to-talk).
  // Opens the interrupting window and bumps the generation so late frames drop.
  function begin_interrupt(self) -> PlaybackCursor {
    PlaybackCursor {
      item_id: self.item_id,
      played_ms: self.played_ms,
      generation: self.generation + 1,
      interrupting: true,
    }
  }

  // We saw response.done {status: cancelled|interrupted}: the firehose is truly
  // off. Close the interrupting window and clear the item; played_ms is captured
  // by truncate() before this is called.
  function end_interrupt(self) -> PlaybackCursor {
    PlaybackCursor {
      item_id: null,
      played_ms: 0,
      generation: self.generation,
      interrupting: self.interrupting && false,  // -> false; written this way to read as a transition
    }
  }
}


// -----------------------------------------------------------------------------
// 3. OpenAIRealtime — the flagship. Truncates by milliseconds of audio played.
//    cancel/truncate/update are three distinct outbound frames over `io`.
// -----------------------------------------------------------------------------

// A deliberate domain failure raised when a session-establishing control frame
// (session.update to open, or the first response.create) cannot be delivered.
// It can arise while driving the duplex (RealtimeError) and while sending a
// control frame (LiveControlError), so it implements both. `op` names which frame.
class RealtimeSessionError {
  op: string
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return true; }
  }
  implements baml.errors.LiveControlError {
    function is_network_error(self) -> bool { return true; }
    function is_rejected(self)      -> bool { return false; }
  }
}

class OpenAIRealtime {
  voice: string
  api_key: string
  // The model's instructions at session start. update_session can change them live.
  instructions: string?

  function default() -> OpenAIRealtime {
    OpenAIRealtime { voice: "alloy", api_key: env.OPENAI_API_KEY, instructions: null }
  }

  // Realtime (canonical): drive the duplex. The loop here is the part that makes
  // barge-in WORK: it owns a PlaybackCursor, reacts to server VAD by calling its
  // own barge_in (via LiveControl), and discards late audio through the cursor.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      // Open the session with the configured voice/instructions.
      io.send(OutEvent {
        kind: OutKind.SessionUpdate,
        session_patch: self.session_open_patch(),
      }) catch (e) { _ => { throw RealtimeSessionError { op: "realtime open" }; } };
      // Ask for the first response.
      io.send(OutEvent { kind: OutKind.ResponseCreate }) catch (e) {
        _ => { throw RealtimeSessionError { op: "response.create" }; }
      };

      // Mutable session-local state. `cursor` is the played-position bookkeeping.
      let cursor = PlaybackCursor.fresh();

      io.on(ev => {
        match (ev.kind) {
          InKind.Audio => {
            // Play (or discard, if mid-interrupt) and advance the played position.
            cursor = cursor.on_audio(ev) catch (e) { _ => { cursor } };
          },
          InKind.SpeechStarted => {
            // Server VAD: the user is talking over the model. Barge-in NOW, but
            // ONLY if something is actually playing — truncating a null item is a
            // protocol error, and cancelling with nothing in flight is wasteful.
            match (cursor.item_id) {
              let id: string => {
                cursor = cursor.begin_interrupt();
                // cancel + truncate-at-played-ms, in the correct order. We are a
                // LiveControl provider, so we call our own capability method.
                self.barge_in(io, id, cursor.played_ms) catch (e) { _ => {} };
              },
              null => {},  // nothing playing; the user just started a fresh turn
            }
          },
          InKind.ResponseDone => {
            // The cancel landed (or the response finished naturally). Close the
            // interrupting window so the NEXT response's audio plays again.
            cursor = cursor.end_interrupt();
          },
          InKind.ItemDone => {},
          InKind.Interrupted => {
            // OpenAI doesn't emit this; harmless to ignore.
            cursor = cursor.end_interrupt();
          },
        }
      });

      // $rust_io_function: block until the session ends (caller closes io or the
      // server hangs up), then materialize the accumulated transcript.
      baml.realtime.await_transcript(io)
    } catch (e) {
      // Trailing normalization: RealtimeSessionError passes through unchanged
      // (it's a known RealtimeError); any foreign Io from await_transcript / io.on
      // is wrapped into the universal hatch on the RealtimeError channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "openai realtime session failed");
    }
  }

  // LiveControl (net-new): the typed barge-in surface. THIS is where OpenAI's
  // wire encoding + ms-truncate semantics live. The app never builds these frames.
  implements LiveControl {
    function cancel(self, io: Channel) -> null
        throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
      // response.cancel: stop the in-flight response. Session stays open.
      io.send(OutEvent { kind: OutKind.Cancel })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.LiveControlError>(e, "openai cancel failed");
    }

    function truncate(self, io: Channel, item_id: string, heard_ms: int) -> null
        throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
      // conversation.item.truncate at the PLAYED position. content_index 0 is the
      // audio content of the assistant item. heard_ms came from PlaybackCursor —
      // the ms the speaker actually rendered, NOT the ms the server generated.
      io.send(OutEvent {
        kind: OutKind.Truncate,
        item_id: item_id,
        content_index: 0,
        audio_end_ms: heard_ms,
      })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.LiveControlError>(e, "openai truncate failed");
    }

    function update_session(self, io: Channel, patch: string) -> null
        throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
      // session.update with a partial patch. No reconnect; takes effect on the
      // NEXT response. Changing voice mid-response does not retro-rewrite audio
      // already streamed.
      io.send(OutEvent { kind: OutKind.SessionUpdate, session_patch: patch })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.LiveControlError>(e, "openai session.update failed");
    }
    // barge_in inherited from LiveControl's default: cancel THEN truncate.
  }

  // No `implements Provider { call }`. Provider is a bare MARKER
  // (../provider-as-marker.md): it carries no interaction method, and `call` is an
  // HttpProvider capability — not something a realtime session can honestly fake.
  // OpenAIRealtime IS a Provider transitively (LiveControl requires Realtime
  // requires Provider); it exposes only its REAL interactions (Realtime.run +
  // LiveControl.{cancel,truncate,update_session,barge_in}). The old degenerate
  // "best-effort single turn" call is gone — OQ#1 resolved by the marker.

  function session_open_patch(self) -> string {
    // Build a native `map<string, json>` literal and serialize it. Strings, the
    // nested map, and `null` ARE `json` values — no per-field wrapper functions.
    let patch: map<string, json> = {
      "voice": self.voice,
      "instructions": match (self.instructions) {
        let s: string => s,
        null           => null,
      },
      // server-VAD turn detection is what gives us speech_started for barge-in.
      "turn_detection": { "type": "server_vad" },
    };
    baml.json.stringify(patch)
  }
}


// -----------------------------------------------------------------------------
// 4. GeminiLive — the divergent backend. Gemini does NOT have a truncate-by-ms
//    verb. When the user interrupts, the SERVER discards its own queued audio and
//    emits an interruption flag; conversation memory is already correct server-
//    side. So:
//      - cancel maps to Gemini's "interrupt the current turn" control frame, and
//      - truncate is a NO-OP (the played-position bookkeeping is irrelevant here
//        because the server, not the client, is authoritative over what was kept).
//
//    This is the leak the scenario must expose: barge_in() is portable at the
//    CALL site, but heard_ms is MEANINGFUL for OpenAI and MEANINGLESS for Gemini.
//    The capability hides the encoding; it cannot hide that the two backends have
//    different notions of "what the model remembers it said." See evaluation (c).
// -----------------------------------------------------------------------------

class GeminiLive {
  voice: string
  api_key: string
  instructions: string?

  function default() -> GeminiLive {
    GeminiLive { voice: "Puck", api_key: env.GEMINI_API_KEY, instructions: null }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      io.send(OutEvent {
        kind: OutKind.SessionUpdate,
        session_patch: baml.json.stringify({ "voice": self.voice }),
      }) catch (e) { _ => { throw RealtimeSessionError { op: "gemini live open" }; } };

      let cursor = PlaybackCursor.fresh();

      io.on(ev => {
        match (ev.kind) {
          InKind.Audio => {
            cursor = cursor.on_audio(ev) catch (e) { _ => { cursor } };
          },
          InKind.Interrupted => {
            // SERVER-AUTHORITATIVE barge-in: Gemini already discarded its queued
            // audio. We only need to stop playing locally and roll the generation
            // so any frames still in our local buffer are dropped. NO truncate.
            cursor = cursor.begin_interrupt().end_interrupt();
          },
          InKind.SpeechStarted => {
            // Gemini also surfaces this; we still proactively stop local playback,
            // but we send NO truncate frame — there is no such verb.
            match (cursor.item_id) {
              let id: string => {
                cursor = cursor.begin_interrupt();
                self.cancel(io) catch (e) { _ => {} };
              },
              null => {},
            }
          },
          InKind.ResponseDone => { cursor = cursor.end_interrupt(); },
          InKind.ItemDone => {},
        }
      });

      baml.realtime.await_transcript(io)
    } catch (e) {
      // RealtimeSessionError passes through; foreign Io is wrapped on the channel.
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "gemini realtime session failed");
    }
  }

  implements LiveControl {
    function cancel(self, io: Channel) -> null
        throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
      // Gemini's "stop the current model turn" control frame. Different wire shape
      // from OpenAI's response.cancel, same OutKind tag — the provider's encoder
      // (below, at the host boundary) maps OutKind.Cancel to the right JSON.
      io.send(OutEvent { kind: OutKind.Cancel })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.LiveControlError>(e, "gemini cancel failed");
    }

    function truncate(self, io: Channel, item_id: string, heard_ms: int) -> null
        throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
      // NO-OP. Gemini is server-authoritative over what was kept: when it
      // interrupts, it has already dropped the unheard tail. There is no
      // client->server "truncate at ms" message to send, and heard_ms cannot be
      // honored even if we wanted to. We swallow the call so barge_in() stays
      // portable, but this is exactly where the abstraction is THIN (eval part c).
      null
    }

    function update_session(self, io: Channel, patch: string) -> null
        throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
      // Gemini also supports live config updates, but only a subset of fields and
      // some require a turn boundary. We forward the patch; the host encoder
      // rejects unsupported keys.
      io.send(OutEvent { kind: OutKind.SessionUpdate, session_patch: patch })
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.LiveControlError>(e, "gemini session.update failed");
    }
    // barge_in inherited; for Gemini truncate is a no-op so it degenerates to
    // "cancel only" — which is the right behavior for a server-authoritative
    // backend, but only by accident of the default's ordering.
  }

  // No `implements Provider { call }` — same as OpenAIRealtime. Provider is a
  // marker; GeminiLive is a Provider transitively via Realtime, and exposes only
  // its real Realtime.run + LiveControl surface. No degenerate buffered call.
}


// -----------------------------------------------------------------------------
// 5. A combinator note. Fallback/Retry (canonical Provider combinators) do NOT
//    forward LiveControl, and SHOULD NOT: a live session is bound to ONE open
//    connection and one PlaybackCursor. You cannot "fall back" a half-spoken
//    response to a different provider mid-sentence — the item_id and played_ms
//    are owner-bound, exactly like a ChainHandle in scenario 20.
//
//    So if you want resilience you put it AROUND establishing the session
//    (which provider do we .live with), never INSIDE the barge-in loop. We make
//    that explicit by NOT implementing LiveControl on any combinator. A `.live`
//    companion that needs barge-in must match LiveControl directly (usage.baml).
// -----------------------------------------------------------------------------


// -----------------------------------------------------------------------------
// Host boundaries used above (genuine $rust seams):
// -----------------------------------------------------------------------------
// baml.audio.play_and_measure(b64_pcm: string) -> int        // push to speaker,
//     returns ms ACTUALLY buffered/played (the played-position primitive)
// baml.realtime.await_transcript(io: Channel) -> Transcript  // block til session end
// baml.json.stringify(j: json) -> string                     // serialize a native map<string,json>
// Channel.{on,send,close}, InEvent/OutEvent                  // canonical + (this file)
