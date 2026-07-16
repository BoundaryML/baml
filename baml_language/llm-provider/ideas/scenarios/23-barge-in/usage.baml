// =============================================================================
// usage.baml — Barge-in, interruption & mid-session mutation (application code)
//
// End-user code. Shows the ergonomics of running a steerable voice session:
//   - a config-only `client` (the live channel is supplied at the .live call),
//   - a `.live` companion that hands in `io`,
//   - how barge-in, truncate, and mid-session voice-swap read at the call site,
//   - two providers (OpenAIRealtime, GeminiLive) to prove portability and to
//     expose where it leaks (Gemini's truncate is a no-op).
// =============================================================================


// -----------------------------------------------------------------------------
// 1. Clients are config-only. NO channel here (it doesn't exist until you run).
//    Voice/instructions are provider fields (options dissolved into fields).
// -----------------------------------------------------------------------------

client VoiceOpenAI() {
  OpenAIRealtime { voice: "alloy", api_key: env.OPENAI_API_KEY, instructions: "Be concise." }
}

client VoiceGemini() {
  GeminiLive { voice: "Puck", api_key: env.GEMINI_API_KEY, instructions: "Be concise." }
}

// Dynamic selection — a client is a function, so this is just an `if`.
client Voice(backend: string) {
  if (backend == "gemini") { GeminiLive { voice: "Puck", api_key: env.GEMINI_API_KEY, instructions: null } }
  else                     { OpenAIRealtime { voice: "alloy", api_key: env.OPENAI_API_KEY, instructions: null } }
}


// -----------------------------------------------------------------------------
// 2. The realtime function. The `.live(args, io)` companion hands in the channel.
//    The function body is config-only; `io` propagates from Realtime.run's
//    signature onto the companion (OQ#2).
// -----------------------------------------------------------------------------

function VoiceAssistant(system: string) -> Transcript {
  client: VoiceOpenAI()
  prompt #"{{ system }}"#
}
// Run it:  VoiceAssistant.live("You are a terse barista.", io)
//   -> matches OpenAIRealtime as Realtime -> OpenAIRealtime.run(prompt, io)
// The barge-in loop lives INSIDE run() (implement.baml §3): server VAD ->
// cursor.begin_interrupt() -> self.barge_in(io, item, played_ms). The app that
// just wants a working steerable assistant writes ZERO barge-in code.


// -----------------------------------------------------------------------------
// 3. App-DRIVEN barge-in. The default loop inside run() handles server-VAD
//    barge-in for you. But some apps want to drive it themselves — e.g. push-to-
//    talk, where the UI (not server VAD) decides the user is interrupting. Then
//    the app needs the LiveControl methods directly. It must `match` the
//    provider for LiveControl, because a `.live` companion only guarantees
//    Realtime, not steerability (capability is a runtime promise).
// -----------------------------------------------------------------------------

// A push-to-talk handler the app wires to its own "user pressed talk" button.
// `cursor` is the app's PlaybackCursor (it owns playback, so it owns the cursor).
function PushToTalkInterrupt(p: Provider, io: Channel, cursor: PlaybackCursor)
    -> null throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
  match (p) {
    let lc: LiveControl => {
      match (cursor.item_id) {
        let id: string => {
          // Same atomic move the auto-loop uses, but triggered by the UI.
          lc.barge_in(io, id, cursor.played_ms);
        },
        null => {},  // nothing playing; ignore the press
      }
    },
    // No honest fallback: a provider that isn't steerable cannot be barged into.
    // Unsupported implements every capability-error interface (incl. LiveControlError),
    // so it is legal on this channel and the caller catches it by its concrete type.
    _ => { throw baml.errors.Unsupported { message: "client does not support live barge-in" }; },
  }
}


// -----------------------------------------------------------------------------
// 4. Mid-session mutation — swap the voice on the fly. Reads as one call. Again
//    a runtime LiveControl match, because `client` returned the existential.
// -----------------------------------------------------------------------------

function SwapVoice(p: Provider, io: Channel, new_voice: string) -> null
    throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
  match (p) {
    let lc: LiveControl => {
      lc.update_session(io, baml.json.stringify({ "voice": new_voice }));
    },
    _ => { throw baml.errors.Unsupported { message: "client cannot be mutated live" }; },
  }
}

// Switch the model to Spanish partway through (instructions patch).
function SwitchToSpanish(p: Provider, io: Channel) -> null
    throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
  match (p) {
    let lc: LiveControl => {
      lc.update_session(io, baml.json.stringify({ "instructions": "From now on, respond only in Spanish." }));
    },
    _ => { throw baml.errors.Unsupported { message: "client cannot be mutated live" }; },
  }
}


// -----------------------------------------------------------------------------
// 5. Cancel vs close — the two distinct operations the background warns about.
//    They are DIFFERENT methods on DIFFERENT interfaces, so you can't conflate
//    them by accident: cancel() is LiveControl (stop one response, session lives);
//    close() is Channel (end the whole connection). The type system keeps them
//    apart — this is the one place the proposal genuinely helps here.
// -----------------------------------------------------------------------------

function StopThisResponse(p: Provider, io: Channel) -> null
    throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
  match (p) {
    let lc: LiveControl => { lc.cancel(io); },   // session stays open; a new turn can follow
    _ => { throw baml.errors.Unsupported { message: "not steerable" }; },
  }
}

function EndTheCall(io: Channel) -> null {
  io.close();   // ends the whole session. No LiveControl needed — it's on Channel.
}


// -----------------------------------------------------------------------------
// 6. Portability check across the two backends. The SAME app code calls
//    barge_in / update_session. What changes is what the backend DOES — and the
//    one place it leaks (Gemini truncate no-op) is invisible at this call site,
//    which is exactly the danger (see evaluation part c).
// -----------------------------------------------------------------------------

// Generic "run a steerable voice turn with manual barge-in wiring" that works
// against either provider via the existential. Proves the call site is portable.
function RunSteerable(p: Provider, io: Channel) -> null
    throws baml.ExtendUnknownError<baml.errors.LiveControlError> {
  match (p) {
    let lc: LiveControl => {
      // Open with whatever VAD/voice the provider configured; then, for demo,
      // mutate the voice mid-call. Identical lines for OpenAI and Gemini.
      lc.update_session(io, baml.json.stringify({ "voice": "shimmer" }));
      // The actual barge-in is driven inside run()'s loop; here we only show that
      // the control surface is uniform across providers at the call site.
    },
    _ => { throw baml.errors.Unsupported { message: "provider not steerable" }; },
  }
}

// Wire it up against both:
//   let openai = VoiceOpenAI();   RunSteerable(openai, io_a);   // ms-truncate barge-in
//   let gemini = VoiceGemini();   RunSteerable(gemini, io_b);   // server-discard barge-in
// Same code. Different memory semantics underneath. The proposal makes the
// ENCODING portable; it does not, and cannot, make the SEMANTICS identical.


// -----------------------------------------------------------------------------
// Host helpers referenced (declared in implement.baml's host-boundary list):
//   baml.json.stringify  (build a native map<string, json> literal -> serialize)
//   Channel.{send,close}, PlaybackCursor, LiveControl, Transcript
// -----------------------------------------------------------------------------
