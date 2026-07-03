// The Realtime capability + examples (scenarios 22–26). A realtime provider is a marker
// that implements `Realtime` directly — no degenerate `call` to fake (OQ1). The live
// `Channel` is a parameter of `run` (pass-in), handed in at the call, never a provider field.
//
// `OpenAiRealtime.run` is now LIVE: it drives OpenAI's Realtime API over the `baml.ws`
// WebSocket surface (text mode). The channel receives the provider's raw event feed and
// the accumulated text is returned as a `Transcript`. `LiveControl` (barge-in) stays
// stubbed (out of scope).

/// The final record returned when a live session ends.
class Transcript {
    text: string,
}

/// A live, duplex I/O channel: the caller hands it in, the provider drives it.
interface Channel {
    function on(self, handler: (string) -> void) -> null throws never
    function send(self, data: string) -> null throws root.errors.Io
    function close(self) -> null throws never
}

/// A live, duplex interaction. Owns `run`; requires only the `Provider` marker.
interface Realtime requires root.ai.Provider {
    function run(self, prompt: string, io: Channel) -> Transcript
        throws root.errors.RealtimeError | root.errors.UnknownError
}

/// Barge-in / interruption / mid-session mutation over a live session (scenario 23).
interface LiveControl requires Realtime {
    function cancel(self, io: Channel) -> null
        throws root.errors.RealtimeError | root.errors.UnknownError
    function truncate(self, io: Channel, played_ms: int) -> null
        throws root.errors.RealtimeError | root.errors.UnknownError
}

/// Extract a JSON value as a string, or `""` if it is not a string. Mirrors
/// `_openai_json_str` — a `throws never` accessor for one-off event-field reads.
function _realtime_json_str(j: json) -> string throws never {
    match (j) {
        let s: string => s,
        _ => "",
    }
}

/// A realtime voice provider (scenario 22), LIVE over OpenAI's Realtime API. Config-only
/// fields; the channel is a `run` parameter. `run` drives a text-mode exchange over a
/// WebSocket: it sends the prompt, forwards every raw provider event to the channel, and
/// accumulates the streamed text into the returned `Transcript`.
class OpenAiRealtime {
    /// The realtime model id (e.g. "gpt-realtime" or "gpt-realtime-mini"). Optional:
    /// defaults to "gpt-realtime" when omitted.
    model: string?,
    voice: string,
    api_key: string,

    implements root.ai.Provider {}

    implements Realtime {
        function run(
            self,
            prompt: string,
            io: Channel,
        ) -> Transcript throws root.errors.RealtimeError | root.errors.UnknownError {
            // GA Realtime API: `/v1/realtime` with a bare bearer token. (The legacy
            // `OpenAI-Beta: realtime=v1` header now selects the retired beta shape and
            // is rejected — do NOT send it.)
            let ws = root
                .ws
                .connect(
                    "wss://api.openai.com/v1/realtime?model=" + (self.model ?? "gpt-realtime"),
                    { "Authorization": "Bearer " + self.api_key },
                ) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["realtime: connect failed"] }
                },
            };

            // Enqueue the user's message as a conversation item.
            let create_item = root
                .json
                .stringify(
                    {
                        "type": "conversation.item.create",
                        "item": {
                            "type": "message",
                            "role": "user",
                            "content": [{ "type": "input_text", "text": prompt }],
                        },
                    },
                );
            ws.send(create_item) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["realtime: send item failed"] }
                },
            };

            // Ask for a text-only response. The GA Realtime API uses `output_modalities`.
            let response_create = root
                .json
                .stringify(
                    { "type": "response.create", "response": { "output_modalities": ["text"] } },
                );
            ws.send(response_create) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["realtime: response.create failed"] }
                },
            };

            // Pump the event feed: forward every raw event to the channel, accumulate
            // text deltas, and stop when the response is done (or an error arrives).
            let acc = "";
            let done = false;
            while (!done) {
                let evt = ws.next() catch (e) {
                    _ => {
                        throw root.errors.UnknownError { data: e, message: ["realtime: recv failed"] }
                    },
                };
                let raw = match (evt) {
                    let s: string => s,
                    null => {
                        done = true;
                        continue;
                    },
                };
                // v1 semantics: the channel receives the provider's event feed.
                io.send(raw) catch (e) {
                    _ => {
                        throw root.errors.UnknownError { data: e, message: ["realtime: channel send failed"] }
                    },
                };
                let j = root.json.parse(raw) catch (e) {
                    _ => {
                        throw root.errors.UnknownError { data: raw, message: ["realtime: event not JSON"] }
                    },
                };
                let etype = _realtime_json_str(root.json.field(j, "type"));
                // Text deltas: GA emits `response.output_text.delta`; older/beta builds
                // used `response.text.delta`. Field is `delta` in both.
                if (etype == "response.output_text.delta" || etype == "response.text.delta") {
                    acc = acc + _realtime_json_str(root.json.field(j, "delta"));
                }
                if (etype == "error" || etype == "response.failed") {
                    ws.close();
                    throw root.errors.UnknownError { data: raw, message: ["realtime: error event"] };
                }
                if (etype == "response.done" || etype == "response.completed") {
                    done = true;
                }
            }

            ws.close();
            Transcript { text: acc }
        }
    }

    implements LiveControl {
        function cancel(
            self,
            io: Channel,
        ) -> null throws root.errors.RealtimeError | root.errors.UnknownError {
            throw root.errors.Unsupported { message: "realtime transport not yet wired" }
        }
        function truncate(
            self,
            io: Channel,
            played_ms: int,
        ) -> null throws root.errors.RealtimeError | root.errors.UnknownError {
            throw root.errors.Unsupported { message: "realtime transport not yet wired" }
        }
    }
}

// --- usage examples ---

// Scenario 22/26: the `.live` companion — negotiate `Realtime` and hand in the channel.
function example_voice_chat(
    p: root.ai.Provider,
    system: string,
    io: Channel,
) -> Transcript throws root.errors.RealtimeError | root.errors.UnknownError {
    match (p) {
        let r: Realtime => r.run(system, io),
        _ => throw root.errors.Unsupported { message: "client is not realtime" },
    }
}

// Scenario 23: barge-in — negotiate `LiveControl` on top of a live session.
function example_barge_in(
    p: root.ai.Provider,
    io: Channel,
    played_ms: int,
) -> null throws root.errors.RealtimeError | root.errors.UnknownError {
    match (p) {
        let lc: LiveControl => lc.truncate(io, played_ms),
        _ => throw root.errors.Unsupported { message: "client has no live control" },
    }
}
