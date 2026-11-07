// The provider-neutral contract between a voice runner and a realtime
// speech-to-speech API. A provider opens a session from a SessionConfig; the
// session carries PCM audio in both directions and reports what happened as
// VoiceEvents. `OpenAILive` and `OpenAIRealtime` are the two implementations.
//
// All audio on this boundary is raw PCM16, 24 kHz, mono, little-endian.

/// What a session is opened with. The runner derives every field from the
/// `ai.FunctionSpec` it is executing.
class SessionConfig {
    /// The system prompt of the voice model.
    instructions: string,
    /// The function tools the model may call. The runner executes them.
    tools: ai.tools.Tool[],
}

/// The assistant produced audio. `item_id` identifies the utterance on
/// providers that support truncation and is null elsewhere.
class AssistantAudio {
    pcm: uint8array,
    item_id: string?,
}

/// A fragment of the assistant's speech transcript, in delivery order.
class AssistantSaid {
    delta: string,
}

/// A fragment of the user's speech transcript, in delivery order.
class UserSaid {
    delta: string,
}

/// The provider detected the start of user speech. Emitted only by providers
/// whose output arrives ahead of real time, where the client has to discard
/// the audio it queued locally. A full-duplex provider never emits it.
class UserSpeechStarted {
}

/// The model called tools. Every call needs a result, delivered together
/// through `VoiceSession.send_tool_results`.
class ToolCalls {
    calls: ai.content.ToolUse[],
}

/// A delegated backend model answered the voice model in text. The voice
/// model decides what to say about it. Emitted only by providers that
/// delegate.
class BackendReply {
    text: string,
}

/// The assistant finished a response. Providers without response boundaries
/// never emit it.
class TurnDone {
}

/// The provider rejected a command or reported a fault. The session stays
/// open unless a `Closed` follows.
class SessionError {
    code: string?,
    message: string,
}

/// The session ended. No event follows this one.
class Closed {
    reason: string,
}

type VoiceEvent = AssistantAudio
    | AssistantSaid
    | UserSaid
    | UserSpeechStarted
    | ToolCalls
    | BackendReply
    | ai.events.Usage
    | TurnDone
    | SessionError
    | Closed;

/// The result of one tool call, as text the model reads.
class ToolResult {
    id: string,
    output: string,
}

interface VoiceSession {
    /// Append microphone audio. Call it continuously, silence included.
    function send_audio(self, pcm: uint8array) -> void throws baml.errors.Io;

    /// Block until the provider reports something, then return it. One wire
    /// message can produce several events or none.
    function next_events(self) -> VoiceEvent[] throws baml.errors.Io;

    /// Deliver the results of one `ToolCalls` event and let the model continue.
    function send_tool_results(self, results: ToolResult[]) -> void throws baml.errors.Io;

    /// Have the assistant say `text` now, in its own words.
    function say(self, text: string) -> void throws baml.errors.Io;

    /// The user interrupted after hearing `played_ms` of the current
    /// utterance. Providers that manage interruption themselves ignore it.
    function interrupted(self, played_ms: int) -> void throws baml.errors.Io;

    /// Begin closing. `next_events` reports `Closed` when the session ends.
    function close(self) -> void throws never;
}

interface VoiceProvider {
    function id(self) -> string throws never;

    /// Open a session and return once the provider is ready for audio.
    function connect(
        self,
        config: SessionConfig,
    ) -> VoiceSession throws baml.errors.Io | baml.errors.Timeout | ai.errors.InvalidRequest;
}

/// The provider a `client: "openai/<model>"` line selects, or null when the
/// model is not a realtime voice model.
function provider_for(client_id: string) -> VoiceProvider? {
    let model = client_id.split("/").at(1) ?? "";
    if (model.starts_with("gpt-live")) {
        let live: VoiceProvider = OpenAILive.new(model = model);
        live
    } else if (model.starts_with("gpt-realtime")) {
        let realtime: VoiceProvider = OpenAIRealtime.new(model = model);
        realtime
    } else {
        null
    }
}

/// (internal) The `wss://…/v1` root for a resolved OpenAI base URL.
function _ws_root(base_url: string?) -> string {
    let http = base_url ?? "https://api.openai.com/v1";
    let trimmed = if (http.ends_with("/")) {
        http.slice(0, http.length() - 1)
    } else {
        http
    };
    if (trimmed.starts_with("https://")) {
        "wss://" + trimmed.slice(8, trimmed.length())
    } else if (trimmed.starts_with("http://")) {
        "ws://" + trimmed.slice(7, trimmed.length())
    } else {
        trimmed
    }
}

/// (internal) `ai.wire.resolve_credential`, with an unreadable environment reported as
/// a configuration error.
function _credential(
    cred: ai.Credential?,
    env_fallback: string?,
) -> string? throws ai.errors.InvalidRequest {
    ai.wire.resolve_credential(cred, env_fallback) catch_all (e) {
        _ => {
            throw ai.errors.InvalidRequest {
                provider: "openai",
                status_code: null,
                detail: `could not resolve a credential: ${e.to_string()}`,
                raw_body: null,
            }
        },
    }
}

/// (internal) The request-time OpenAI key, resolved the way `openai.*` clients do.
function _openai_key(api_key: ai.Credential?) -> string throws ai.errors.InvalidRequest {
    if let k: string = _credential(api_key, "OPENAI_API_KEY") {
        return k;
    }
    throw ai.errors.InvalidRequest {
        provider: "openai",
        status_code: null,
        detail: "openai needs an API key: pass api_key (a literal or env.NAME), or set OPENAI_API_KEY",
        raw_body: null,
    }
}

/// (internal) Decode a function call's JSON argument string.
function _tool_args(raw: string) -> map<string, unknown> {
    if (raw.trim() == "") {
        return {};
    }
    baml.json.from_string<map<string, unknown>>(raw) catch_all (e) {
        _ => {
            let empty: map<string, unknown> = {};
            empty
        },
    }
}

/// (internal) A tool's argument schema as JSON, or an open object when it does not serialize.
function _schema_json(schema: map<string, unknown>) -> baml.json.json throws never {
    baml.json.to_json(schema) catch_all (e) {
        _ => {
            let open: baml.json.json = { "type": "object", "properties": {} };
            open
        },
    }
}
