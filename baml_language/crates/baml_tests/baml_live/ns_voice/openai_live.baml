// GPT-Live over its primary WebSocket (wss://api.openai.com/v1/live/sessions).
// https://developers.openai.com/api/docs/guides/live
//
// GPT-Live is full duplex. The client streams microphone audio continuously,
// the server streams output audio continuously (digital silence while the
// model is quiet), and the model decides by itself when to speak and when to
// yield. There is no turn detection to configure, no response to create, and
// no audio to truncate on interruption.
//
// The voice model has no tools. It delegates work to a Responses backend
// model, which owns the function tools. The backend's streaming events arrive
// wrapped in `response.event`; this session collects the finished function
// calls of one backend response and reports them as one `ToolCalls`.

/// The preamble of the backend model's instructions. The task prompt follows it.
function live_backend_preamble() -> string {
    "You are the backend of an assistant in a live voice conversation. You receive the conversation so far; speech recognition can introduce mistakes, unfinished phrases, and later corrections, so rely on the latest context. Use your tools to do what the conversation calls for. Then answer briefly with the facts, the current status, and the next step, for the voice assistant to say aloud. State only what your tools returned: an action happened only if you called its tool in this conversation and the tool succeeded, and codes, names, and numbers come from tool results, never from you. If something still has to be done, do it now with a tool instead of describing it. When the user has said goodbye, end the conversation."
}

class OpenAILive {
    model: string,
    voice: string,
    /// The Responses model GPT-Live delegates reasoning and tool calls to.
    backend_model: string,
    backend_reasoning_effort: string?,
    // A literal, `env.NAME`, or null to read OPENAI_API_KEY at connect time.
    api_key: ai.Credential?,
    base_url: ai.Credential?,
    connect_timeout_ms: int,

    function new(
        model: string = "gpt-live-1",
        voice: string = "marin",
        backend_model: string = "gpt-5.6-luna",
        backend_reasoning_effort: string? = "low",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        connect_timeout_ms: int = 30000,
    ) -> OpenAILive {
        OpenAILive {
            model: model,
            voice: voice,
            backend_model: backend_model,
            backend_reasoning_effort: backend_reasoning_effort,
            api_key: api_key,
            base_url: base_url,
            connect_timeout_ms: connect_timeout_ms,
        }
    }

    /// The `session.start` message for `config`.
    function start_message(self, config: SessionConfig) -> baml.json.json {
        let tools: baml.json.json[] = [];
        for (let t in config.tools) {
            tools.push(
                {
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": _schema_json(t.input_schema),
                    "strict": false,
                },
            );
        }
        // The voice model has no tools, so it is told what its backend can do
        // and when to hand off.
        let abilities = "";
        for (let t in config.tools) {
            abilities = abilities + `\n- ${t.name}: ${t.description}`;
        }
        let voice_instructions = config.instructions
            + "\n\nYou cannot take actions yourself. Your backend can, with these tools:"
            + abilities
            + "\nHand the request to your backend whenever the conversation calls for one of them, and tell the user only what the backend reported back. Never make up the result of an action. When the user says goodbye or has nothing else, say goodbye and hand off to your backend one last time so that it ends the conversation.";
        let backend: map<string, baml.json.json> = {
            "model": self.backend_model,
            "instructions": live_backend_preamble() + "\n\n" + config.instructions,
            "tools": tools,
            "tool_choice": "auto",
            // One call at a time keeps the result/continue handshake simple.
            "parallel_tool_calls": false,
        };
        if let effort: string = self.backend_reasoning_effort {
            let _ = backend.set("reasoning", { "effort": effort });
        }
        {
            "type": "session.start",
            "session": {
                "model": self.model,
                "instructions": voice_instructions,
                "audio": {
                    "format": { "type": "audio/pcm", "rate": 24000 },
                    "output": { "voice": self.voice },
                },
                "delegation": { "type": "responses", "responses": backend },
            },
        }
    }

    implements VoiceProvider {
        function id(self) -> string {
            `openai/${self.model}`
        }

        function connect(self, config: SessionConfig) -> VoiceSession {
            let key = _openai_key(self.api_key);
            let root = _ws_root(_credential(self.base_url, null));
            let ws = baml.ws.connect(
                root + "/live/sessions",
                { "Authorization": `Bearer ${key}` },
                timeout = baml.time.Duration.from_milliseconds(self.connect_timeout_ms),
            );
            ws.send(baml.json.stringify(self.start_message(config)));
            // `session.started` must arrive before any audio or command.
            let ready = false;
            while (!ready) {
                match (ws.next()) {
                    let text: string => {
                        let ev = baml.json.parse(text) catch_all (e) {
                            _ => null,
                        };
                        let kind = baml.json.path_or<string>(ev, ".type", "");
                        if (kind == "session.started") {
                            ready = true;
                        } else if (kind == "error") {
                            throw ai.errors.InvalidRequest {
                                provider: "openai-live",
                                status_code: null,
                                detail: baml.json.path_or<string>(ev, ".error.message", text),
                                raw_body: text,
                            }
                        }
                    },
                    let binary: uint8array => {},
                    let closed: baml.ws.CloseEvent => {
                        throw baml.errors.Io {
                            message: `GPT-Live closed the socket before the session started: ${closed.code} ${closed.reason}`,
                        }
                    },
                }
            }
            OpenAILiveSession { ws: ws, _calls: [], _seq: 0, _closing: false }
        }
    }
}

class OpenAILiveSession {
    ws: baml.ws.WebSocket,
    // Finished function calls of the backend response in flight.
    _calls: ai.content.ToolUse[],
    _seq: int,
    _closing: bool,

    function _send(self, message: baml.json.json) -> void throws baml.errors.Io {
        self.ws.send(baml.json.stringify(message));
    }

    function _event_id(self, prefix: string) -> string {
        self._seq = self._seq + 1;
        `${prefix}_${self._seq}`
    }

    /// (internal) One nested Responses streaming event.
    function _backend_event(self, ev: baml.json.json) -> VoiceEvent[] {
        let kind = baml.json.path_or<string>(ev, ".type", "");
        let out: VoiceEvent[] = [];
        if (kind == "response.output_item.done") {
            // `response.completed` carries `output: []`, so the finished items
            // are the only place a call's id, name, and arguments appear.
            let item_type = baml.json.path_or<string>(ev, ".item.type", "");
            if (item_type == "message") {
                let text = "";
                for (let part in baml.json.path_or<baml.json.json[]>(ev, ".item.content", [])) {
                    text = text + baml.json.path_or<string>(part, ".text", "");
                }
                if (text.trim() != "") {
                    out.push(BackendReply { text: text.trim() });
                }
            }
            if (item_type == "function_call") {
                self._calls.push(
                    ai.content.ToolUse {
                        id: baml.json.path_or<string>(ev, ".item.call_id", ""),
                        name: baml.json.path_or<string>(ev, ".item.name", ""),
                        args: _tool_args(baml.json.path_or<string>(ev, ".item.arguments", "")),
                    },
                );
            }
        } else if (kind == "response.completed") {
            let input = baml.json.path_or<int>(ev, ".response.usage.input_tokens", 0);
            let output = baml.json.path_or<int>(ev, ".response.usage.output_tokens", 0);
            if (input + output > 0) {
                out.push(
                    ai.events.Usage {
                        input_tokens: input,
                        output_tokens: output,
                        cached_input_tokens: null,
                        reasoning_tokens: null,
                    },
                );
            }
            if (self._calls.length() > 0) {
                out.push(ToolCalls { calls: self._calls });
                self._calls = [];
            }
        } else if (kind == "response.failed" || kind == "response.incomplete" || kind == "error") {
            self._calls = [];
            out.push(
                SessionError {
                    code: kind,
                    message: baml.json.path_or<string>(
                        ev,
                        ".response.error.message",
                        baml.json.path_or<string>(
                            ev,
                            ".message",
                            "the backend response did not complete",
                        ),
                    ),
                },
            );
        }
        out
    }

    implements VoiceSession {
        function send_audio(self, pcm: uint8array) -> void {
            self._send({ "type": "session.input_audio.append", "audio": pcm.to_base64() });
        }

        function next_events(self) -> VoiceEvent[] {
            match (self.ws.next()) {
                let text: string => {
                    let ev = baml.json.parse(text) catch_all (e) {
                        _ => null,
                    };
                    let kind = baml.json.path_or<string>(ev, ".type", "");
                    match (kind) {
                        "session.output_audio.delta" => {
                            let pcm = baml.Uint8Array.from_base64(baml.json.path_or<string>(ev, ".delta", "")) catch_all (e) {
                                _ => b"",
                            };
                            [AssistantAudio { pcm: pcm, item_id: null }]
                        },
                        "session.output_transcript.delta" => {
                            [AssistantSaid { delta: baml.json.path_or<string>(ev, ".delta", "") }]
                        },
                        "session.input_transcript.delta" => {
                            [UserSaid { delta: baml.json.path_or<string>(ev, ".delta", "") }]
                        },
                        "response.event" => self._backend_event(baml.json.field(ev, "event")),
                        "error" => {
                            [
                                SessionError {
                                    code: baml.json.path_or<string?>(ev, ".error.code", null),
                                    message: baml.json.path_or<string>(ev, ".error.message", text),
                                },
                            ]
                        },
                        "session.closed" => {
                            [Closed { reason: baml.json.path_or<string>(ev, ".reason", "closed") }]
                        },
                        _ => [],
                    }
                },
                let binary: uint8array => [],
                let closed: baml.ws.CloseEvent => {
                    [Closed { reason: `socket closed (${closed.code}) ${closed.reason}`.trim() }]
                },
            }
        }

        function send_tool_results(self, results: ToolResult[]) -> void {
            for (let r in results) {
                self._send(
                    {
                        "type": "response.item.create",
                        "event_id": self._event_id("tool_result"),
                        "item": { "type": "function_call_output", "call_id": r.id, "output": r.output },
                    },
                );
            }
            // Results alone do not resume the backend response.
            self._send({ "type": "response.create", "event_id": self._event_id("continue") });
        }

        function say(self, text: string) -> void {
            self._send(
                {
                    "type": "session.commentary.append",
                    "event_id": self._event_id("say"),
                    "delegation_id": null,
                    "content": text,
                },
            );
        }

        function interrupted(self, played_ms: int) -> void {
            // GPT-Live hears the user while it speaks and yields by itself.
        }

        function close(self) -> void {
            if (self._closing) {
                return;
            }
            self._closing = true;
            self._send({ "type": "session.close", "event_id": self._event_id("close") }) catch_all (e) {
                _ => {
                    self.ws.close(1000, "") catch_all (e2) {
                        _ => {}
                    };
                },
            };
        }
    }
}
