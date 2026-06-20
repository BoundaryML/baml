// ============================================================================
// usage.baml — Realtime / voice, from the APPLICATION author's side.
//
// End-user code. Uses the providers + Realtime capability from implement.baml.
//
// The thing to watch for: the client block is CONFIG ONLY (voice, instructions,
// turn mode). There is no channel in it — the live duplex does not exist until
// you connect. The channel is handed in at the .live call. The whole "what's
// hard" of voice (concurrency, side-channel audio, server-initiated turns)
// lives below the surface; the app writes handlers and a capture loop.
// ============================================================================


// ---- Two config-only clients over two divergent providers --------------------

// Server-VAD phone-call feel: the SERVER decides when the user stopped and
// auto-replies. The app never commits or asks for a response.
client PhoneAssistant() {
  OpenAIRealtime {
    model: "gpt-4o-realtime-preview",
    voice: "alloy",
    api_key: env.OPENAI_API_KEY,
    instructions: "You are a friendly voice assistant. Keep replies short.",
    manual_turn: false,                    // server_vad
  }
}

// Same capability, different provider. Gemini Live. Note `voice` is absent —
// Gemini's voice catalog is not interchangeable, so the OpenAI field would be a
// lie here. (This is the portability leak called out in evaluation.md.)
client GeminiPhone() {
  GeminiLive {
    model: "gemini-2.0-flash-live-001",
    api_key: env.GEMINI_API_KEY,
    instructions: "Be brief.",
    manual_turn: false,
  }
}

// Push-to-talk over OpenAI: the UI has a "hold to talk" button, so the app
// commits the buffer and asks for a response itself. ONLY the boolean differs
// from PhoneAssistant — the divergence is config, not a different client shape.
client PushToTalk() {
  OpenAIRealtime {
    model: "gpt-4o-realtime-preview",
    voice: "echo",
    api_key: env.OPENAI_API_KEY,
    instructions: "Answer only when asked.",
    manual_turn: true,
  }
}


// ---- The function. `client:` takes the config-only provider. The live channel
//      is NOT named here — it arrives on the .live companion. -------------------

function VoiceChat(system: string) -> Transcript {
  client: PhoneAssistant()
  prompt #"{{ system }}"#
}


// ============================================================================
// How the app drives a server-VAD call. This is plain application code (would
// live in Python/TS calling the generated VoiceChat.live), shown here in BAML
// to make the ergonomics concrete.
// ============================================================================

function run_phone_call() -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  // 1. Get a Channel from the host, bound to the mic + speaker. The app does
  //    NOT construct the wire — it hands the provider an io it can drive.
  let io = baml.audio.open_channel();           // returns an AudioChannel : Channel

  // 2. Register what to do with server frames. THIS is the side channel:
  //    audio is played as it arrives, never returned. Captions update the UI.
  io.on_kind(InKind.OutputAudio, ev => { baml.audio.play(ev.audio); });
  io.on_kind(InKind.OutputText, ev => { ui.append_caption(ev.text); });
  io.on_kind(InKind.InputTranscript, ev => { ui.append_user_line(ev.text); });
  io.on_kind(InKind.Error, ev => { ui.toast(ev.error_message); });

  // 3. Pump the mic UP. Under server VAD we send ONLY AppendAudio — no commit,
  //    no response.create. The server detects silence and replies on its own.
  //    `spawn { }` (BEP-034) runs the capture loop concurrently with the .live
  //    call below and returns a Future immediately.
  let pump: baml.future.Future<null, baml.errors.Io> = spawn {
    for (let chunk = mic.next(); chunk != null; chunk = mic.next()) {
      io.send(OutEvent {
        kind: OutKind.AppendAudio, audio: chunk,
        text: null, voice: null, item_id: null, audio_end_ms: null, tool_result: null,
      }) catch (e) { _ => { return null; } };
    }
  };

  // 4. Open the session. This BLOCKS for the whole call — many turns — and
  //    returns the Transcript record when the user closes io. Audio already
  //    streamed out; only the durable item list comes back here.
  //
  //    Catch most-specific-first: the concrete server-pushed decline, then the
  //    realtime capability interface, then the UnknownError escape hatch. Note
  //    the closed-channel Unsupported can never fire here — VoiceChat's client
  //    IS realtime — but a non-realtime client would surface it concretely.
  VoiceChat.live("You are a friendly voice assistant. Keep replies short.", io) catch (e) {
    let s: RealtimeServerError    => { ui.toast(s.message); Transcript { items: [], ended_reason: "error" }; }
    let r: baml.errors.RealtimeError => {
      if (r.is_network_error()) { ui.toast("connection dropped"); } else { ui.toast("voice session failed"); }
      Transcript { items: [], ended_reason: "error" };
    }
    let u: baml.UnknownError      => { ui.toast(u.message.join(": ")); Transcript { items: [], ended_reason: "error" }; }
  }
}


// ============================================================================
// Push-to-talk: same companion, but the app drives turn-taking itself.
// ============================================================================

function VoicePTT(system: string) -> Transcript {
  client: PushToTalk()
  prompt #"{{ system }}"#
}

function run_push_to_talk() -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  let io = baml.audio.open_channel();
  io.on_kind(InKind.OutputAudio, ev => { baml.audio.play(ev.audio); });
  io.on_kind(InKind.OutputText, ev => { ui.append_caption(ev.text); });

  // While the button is HELD, stream audio up. `spawn { }` runs the capture
  // loop concurrently with the .live session below.
  let pump: baml.future.Future<null, baml.errors.Io> = spawn {
    for (let chunk = mic.while_held(); chunk != null; chunk = mic.while_held()) {
      io.send(OutEvent {
        kind: OutKind.AppendAudio, audio: chunk,
        text: null, voice: null, item_id: null, audio_end_ms: null, tool_result: null,
      }) catch (e) { _ => { return null; } };
    }
    // On RELEASE, the app commits the buffer and asks for the response. This is
    // the manual-turn path: the client initiates generation, not the server.
    io.send(OutEvent { kind: OutKind.Commit, audio: null, text: null, voice: null,
                       item_id: null, audio_end_ms: null, tool_result: null }) catch (e) { _ => {} };
    io.send(OutEvent { kind: OutKind.RequestResponse, audio: null, text: null, voice: null,
                       item_id: null, audio_end_ms: null, tool_result: null }) catch (e) { _ => {} };
  };

  VoicePTT.live("Answer only when asked.", io)
}


// ============================================================================
// Barge-in: react to server VAD by cancelling + truncating to what was heard.
// The app keeps a running count of ms actually played and sends Truncate so the
// model's MEMORY reflects what the user heard, not what it generated.
// ============================================================================

function run_with_bargein() -> Transcript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  let io = baml.audio.open_channel();
  let played = PlayedCursor { ms: 0, active_item: null };

  io.on_kind(InKind.OutputAudio, ev => {
    played.ms = played.ms + baml.audio.play(ev.audio);   // play() returns ms enqueued
    played.active_item = ev.item_id;
  });

  // When the SERVER's VAD says the user started talking over the model, cut it
  // off and tell the server how much was actually heard.
  io.on_kind(InKind.SpeechStarted, ev => {
    if (played.active_item != null) {
      io.send(OutEvent { kind: OutKind.Cancel, audio: null, text: null, voice: null,
                         item_id: null, audio_end_ms: null, tool_result: null }) catch (e) { _ => {} };
      io.send(OutEvent {
        kind: OutKind.Truncate, item_id: played.active_item, audio_end_ms: played.ms,
        audio: null, text: null, voice: null, tool_result: null,
      }) catch (e) { _ => {} };
      played.ms = 0;
    }
  });

  VoiceChat.live("You are a friendly voice assistant.", io)
}

class PlayedCursor { ms: int, active_item: string? }


// ============================================================================
// A combinator chain over a realtime client. Fallback forwards .live by
// runtime delegation (proposal §6). NOTE this compiles even though only ONE
// member is realtime; it degrades/errors at call time. A retry of a duplex
// session is a reconnect, NOT a replay — see evaluation.md, the connection IS
// the session.
// ============================================================================

client ResilientVoice() {
  PhoneAssistant().fallback_to(GeminiPhone())     // first realtime member wins .live
}

function VoiceHA(system: string) -> Transcript {
  client: ResilientVoice()
  prompt #"{{ system }}"#
}
// VoiceHA.live("...", io) -> Fallback.run picks the first Realtime member.
// If PhoneAssistant's connect fails, Fallback's .live re-tries Gemini — but any
// audio already streamed over `io` from the failed attempt is gone, because the
// channel state is not part of the provider's retryable value.
