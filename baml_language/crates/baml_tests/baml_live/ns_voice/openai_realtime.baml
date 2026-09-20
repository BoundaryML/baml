// The OpenAI Realtime API over WebSocket (wss://api.openai.com/v1/realtime).
// https://developers.openai.com/api/docs/guides/realtime
//
// Realtime is turn based. Server-side voice activity detection commits the
// user's turn and starts a response, and the speech-to-speech model calls the
// function tools itself. Output audio arrives faster than it plays, so an
// interruption has two client-side halves: discard the audio queued locally,
// and truncate the assistant item to what the user actually heard.

class OpenAIRealtime {
    model: string,
    voice: string,
    /// Transcribes the user's audio for captions and the journal. Null turns
    /// user transcripts off; the model hears the audio either way.
    transcription_model: string?,
    /// An ISO-639-1 hint such as "en". Null lets the transcriber guess, and a
    /// short word can be guessed wrong.
    transcription_language: string?,
    /// `server_vad` or `semantic_vad`.
    turn_detection: string,
    api_key: ai.Credential?,
    base_url: ai.Credential?,
    connect_timeout_ms: int,

    function new(
        model: string = "gpt-realtime",
        voice: string = "marin",
        transcription_model: string? = "gpt-4o-mini-transcribe",
        transcription_language: string? = null,
        turn_detection: string = "server_vad",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        connect_timeout_ms: int = 30000,
    ) -> OpenAIRealtime {
        OpenAIRealtime {
            model: model,
            voice: voice,
            transcription_model: transcription_model,
            transcription_language: transcription_language,
            turn_detection: turn_detection,
            api_key: api_key,
            base_url: base_url,
            connect_timeout_ms: connect_timeout_ms,
        }
    }

    /// The `session.update` message for `config`.
    function update_message(self, config: SessionConfig) -> baml.json.json {
        let tools: baml.json.json[] = [];
        for (let t in config.tools) {
            tools.push(
                {
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": _schema_json(t.input_schema),
                },
            );
        }
        let input: map<string, baml.json.json> = {
            "format": { "type": "audio/pcm", "rate": 24000 },
            "turn_detection": { "type": self.turn_detection, "create_response": true, "interrupt_response": true },
        };
        if let stt: string = self.transcription_model {
            let transcription: map<string, baml.json.json> = { "model": stt };
            if let language: string = self.transcription_language {
                let _ = transcription.set("language", language);
            }
            let _ = input.set("transcription", transcription);
        }
        {
            "type": "session.update",
            "session": {
                "type": "realtime",
                "instructions": config.instructions,
                "output_modalities": ["audio"],
                "audio": {
                    "input": input,
                    "output": { "format": { "type": "audio/pcm", "rate": 24000 }, "voice": self.voice },
                },
                "tools": tools,
                "tool_choice": "auto",
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
                `${root}/realtime?model=${self.model}`,
                { "Authorization": `Bearer ${key}` },
                timeout = baml.time.Duration.from_milliseconds(self.connect_timeout_ms),
            );
            // The server opens with `session.created`; the session is ready
            // once it has acknowledged this configuration.
            ws.send(baml.json.stringify(self.update_message(config)));
            let ready = false;
            while (!ready) {
                match (ws.next()) {
                    let text: string => {
                        let ev = baml.json.parse(text) catch_all (e) {
                            _ => null,
                        };
                        let kind = baml.json.path_or<string>(ev, ".type", "");
                        if (kind == "session.updated") {
                            ready = true;
                        } else if (kind == "error") {
                            throw ai.errors.InvalidRequest {
                                provider: "openai-realtime",
                                status_code: null,
                                detail: baml.json.path_or<string>(ev, ".error.message", text),
                                raw_body: text,
                            }
                        }
                    },
                    let binary: uint8array => {},
                    let closed: baml.ws.CloseEvent => {
                        throw baml.errors.Io {
                            message: `the Realtime API closed the socket before the session was configured: ${closed.code} ${closed.reason}`,
                        }
                    },
                }
            }
            OpenAIRealtimeSession {
                ws: ws,
                _instructions: config.instructions,
                _item_id: null,
                _response_active: false,
                _create_pending: false,
                _closing: false,
            }
        }
    }
}

class OpenAIRealtimeSession {
    ws: baml.ws.WebSocket,
    _instructions: string,
    // The assistant item whose audio is playing, for truncation.
    _item_id: string?,
    // Server VAD starts responses by itself, and only one may be active.
    // A `response.create` that would collide waits for `response.done`.
    _response_active: bool,
    _create_pending: bool,
    _closing: bool,

    function _send(self, message: baml.json.json) -> void throws baml.errors.Io {
        self.ws.send(baml.json.stringify(message));
    }

    /// (internal) Start a response now, or as soon as the active one is done.
    function _create_response(self) -> void throws baml.errors.Io {
        if (self._response_active) {
            self._create_pending = true;
        } else {
            self._response_active = true;
            self._send({ "type": "response.create" });
        }
    }

    /// (internal) `response.done`: the function calls, the token usage, the boundary.
    function _response_done(self, ev: baml.json.json) -> VoiceEvent[] {
        let out: VoiceEvent[] = [];
        let calls: ai.content.ToolUse[] = [];
        for (let item in baml.json.path_or<baml.json.json[]>(ev, ".response.output", [])) {
            if (baml.json.path_or<string>(item, ".type", "") == "function_call") {
                calls.push(
                    ai.content.ToolUse {
                        id: baml.json.path_or<string>(item, ".call_id", ""),
                        name: baml.json.path_or<string>(item, ".name", ""),
                        args: _tool_args(baml.json.path_or<string>(item, ".arguments", "")),
                    },
                );
            }
        }
        let input = baml.json.path_or<int>(ev, ".response.usage.input_tokens", 0);
        let output = baml.json.path_or<int>(ev, ".response.usage.output_tokens", 0);
        if (input + output > 0) {
            out.push(
                ai.events.Usage {
                    input_tokens: input,
                    output_tokens: output,
                    cached_input_tokens: baml.json.path_or<int?>(
                        ev,
                        ".response.usage.input_token_details.cached_tokens",
                        null,
                    ),
                    reasoning_tokens: null,
                },
            );
        }
        if (calls.length() > 0) {
            out.push(ToolCalls { calls: calls });
        }
        out.push(TurnDone {  });
        out
    }

    implements VoiceSession {
        function send_audio(self, pcm: uint8array) -> void {
            self._send({ "type": "input_audio_buffer.append", "audio": pcm.to_base64() });
        }

        function next_events(self) -> VoiceEvent[] {
            match (self.ws.next()) {
                let text: string => {
                    let ev = baml.json.parse(text) catch_all (e) {
                        _ => null,
                    };
                    let kind = baml.json.path_or<string>(ev, ".type", "");
                    match (kind) {
                        "response.output_audio.delta" => {
                            let pcm = baml.Uint8Array.from_base64(baml.json.path_or<string>(ev, ".delta", "")) catch_all (e) {
                                _ => b"",
                            };
                            self._item_id = baml.json.path_or<string?>(ev, ".item_id", null);
                            [AssistantAudio { pcm: pcm, item_id: self._item_id }]
                        },
                        "response.output_audio_transcript.delta" => {
                            [AssistantSaid { delta: baml.json.path_or<string>(ev, ".delta", "") }]
                        },
                        "input_audio_buffer.speech_started" => [UserSpeechStarted {  }],
                        "response.created" => {
                            self._response_active = true;
                            []
                        },
                        // The finished transcript of one committed stretch of
                        // speech. Transcription runs beside the conversation,
                        // so it can arrive after the assistant began to answer.
                        "conversation.item.input_audio_transcription.completed" => {
                            [
                                UserSaid {
                                    delta: baml.json.path_or<string>(ev, ".transcript", "") + " ",
                                },
                            ]
                        },
                        "response.done" => {
                            self._response_active = false;
                            let out = self._response_done(ev);
                            if (self._create_pending) {
                                self._create_pending = false;
                                self._create_response();
                            }
                            out
                        },
                        "error" => {
                            [
                                SessionError {
                                    code: baml.json.path_or<string?>(ev, ".error.code", null),
                                    message: baml.json.path_or<string>(ev, ".error.message", text),
                                },
                            ]
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
                        "type": "conversation.item.create",
                        "item": { "type": "function_call_output", "call_id": r.id, "output": r.output },
                    },
                );
            }
            self._create_response();
        }

        function say(self, text: string) -> void {
            self._send(
                {
                    "type": "response.create",
                    // Per-response instructions replace the session's, so
                    // the session's are repeated here.
                    "response": { "instructions": `${self._instructions}\n\nRight now: ${text}` },
                },
            );
        }

        function interrupted(self, played_ms: int) -> void {
            // `interrupt_response` already cancelled generation. Truncating
            // removes the unheard audio from the model's context.
            if let item: string = self._item_id {
                self._send(
                    {
                        "type": "conversation.item.truncate",
                        "item_id": item,
                        "content_index": 0,
                        "audio_end_ms": played_ms,
                    },
                );
                self._item_id = null;
            }
        }

        function close(self) -> void {
            if (self._closing) {
                return;
            }
            self._closing = true;
            self.ws.close(1000, "") catch_all (e) {
                _ => {}
            };
        }
    }
}
