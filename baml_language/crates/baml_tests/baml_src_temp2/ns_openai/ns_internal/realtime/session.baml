class RealtimeSession {
    socket: baml.ws.WsStream?,
    channel: root.ai.realtime.Channel,
    tools: root.ai.tools.Tool[],
    voice: string,
    output_audio: bool,
    keep_open: bool,
    input_transcription_enabled: bool,
    automatic_turn_detection: bool,
    items: root.ai.realtime.LiveEvent[],
    submitted_results: root.ai.tools.ToolResult[],
    is_closed: bool,
    audio_boundary_emitted: bool,
    awaiting_input_transcript: bool,
    last_assistant_item_id: string?,
    assistant_audio_bytes_by_item_id: map<string, int>,
    pending_tool_calls: root.ai.tools.ToolCall[],
    response_finished: bool,
    cancel_requests: int,
    truncate_positions_ms: int[],

    // function cleanup(self) -> void throws never { self.close(); }

    function send_frame(
        self,
        frame: string,
        label: string,
    ) -> null throws baml.errors.UnknownError {
        match (self.socket) {
            let ws: baml.ws.WsStream => {
                ws.send(frame) catch (e) {
                    _ => throw baml.errors.UnknownError.with_message<never>(e, label),
                };
                self.channel.send(frame) catch (e) {
                    _ => null
                };
                null
            },
            null => {
                throw baml.errors.UnknownError.with_message<never>(null, "realtime session is closed")
            },
        }
    }

    function remember(
        self,
        batch: root.ai.realtime.LiveEvent[],
    ) -> root.ai.realtime.LiveEvent[] throws never {
        for (let item in batch) {
            self.items.push(item);
        }
        batch
    }

    function start_response(self) -> null throws baml.errors.UnknownError {
        self.audio_boundary_emitted = false;
        self.pending_tool_calls = [];
        self.response_finished = false;
        self.send_frame(
            _openai_realtime_response_create(self.tools, self.output_audio, self.voice),
            "realtime response.create failed",
        )
    }

    function finish_response_batch(self, batch: root.ai.realtime.LiveEvent[]) -> null throws never {
        if (self.pending_tool_calls.length() > 0) {
            batch.push(root.ai.realtime.LiveToolCalls { calls: self.pending_tool_calls });
            self.pending_tool_calls = [];
        } else if (self.keep_open) {
            batch.push(root.ai.realtime.LiveTurnCompleted {  });
        } else {
            self.close();
            batch.push(root.ai.realtime.LiveClosed {  });
        }
        null
    }

    implements root.ai.realtime.LiveSession {
        // Return the next meaningful normalized event batch. Text/audio deltas
        // yield immediately so callers can log and play them while the response is
        // still in flight. Tool calls yield together at the response boundary.
        function receive(self) -> root.ai.realtime.LiveEvent[] throws baml.errors.UnknownError {
            if (self.is_closed) {
                return [];
            }
            let ws = match (self.socket) {
                let value: baml.ws.WsStream => value,
                null => {
                    self.is_closed = true;
                    return self.remember([root.ai.realtime.LiveClosed {  }]);
                },
            };
            let batch: root.ai.realtime.LiveEvent[] = [];
            let batch_done = false;
            //# Receive one realtime event batch
            while (!batch_done) {
                //## Read the next provider event
                let next = ws.next() catch (e) {
                    _ => {
                        throw baml.errors.UnknownError.with_message<never>(e, "realtime receive failed")
                    },
                };
                //## Normalize the provider event
                match (next) {
                    null => {
                        self.close();
                        batch.push(root.ai.realtime.LiveClosed {  });
                        batch_done = true;
                    },
                    let raw: string => {
                        self.channel.send(raw) catch (e) {
                            _ => null
                        };
                        let event = baml.json.parse(raw) catch (e) {
                            _ => {
                                throw baml.errors.UnknownError.with_message<never>(raw, "realtime event JSON failed")
                            },
                        };
                        let kind = baml.json.path_or(event, ".type", "");
                        // OpenAI owns the response lifecycle. In particular,
                        // server VAD can create a response without a local
                        // request_response() call.
                        if (kind == "response.created") {
                            self.response_finished = false;
                            self.audio_boundary_emitted = false;
                            self.pending_tool_calls = [];
                        }
                        //### Surface text and audio transcripts uniformly
                        if (
                            kind == "response.output_text.delta"
                                || kind == "response.text.delta"
                                || kind == "response.output_audio_transcript.delta"
                                || kind == "response.audio_transcript.delta"
                        ) {
                            batch
                                .push(
                                    root.ai.realtime.TranscriptDelta {
                                        text: baml.json.path_or(event, ".delta", ""),
                                    },
                                );
                            batch_done = true;
                        }
                        //### Surface provider-generated assistant audio for caller playback
                        if (kind == "response.output_audio.delta" || kind == "response.audio.delta") {
                            let encoded = baml.json.path_or(event, ".delta", "");
                            let audio = baml.Uint8Array.from_base64(encoded) catch (e) {
                                _ => {
                                    throw baml.errors.UnknownError.with_message<never>(encoded, "realtime audio delta was not valid base64")
                                },
                            };
                            let item_id = baml.json.path_or(event, ".item_id", self.last_assistant_item_id ?? "");
                            if (item_id != "") {
                                self.assistant_audio_bytes_by_item_id
                                    .set(
                                        item_id,
                                        (self.assistant_audio_bytes_by_item_id.get(item_id) ?? 0)
                                            + audio.length(),
                                    );
                            }
                            batch.push(root.ai.realtime.AssistantAudioDelta { audio: audio });
                            batch_done = true;
                        }
                        //### Input transcription is asynchronous and may follow response.done
                        if (kind == "conversation.item.input_audio_transcription.completed") {
                            self.awaiting_input_transcript = false;
                            batch
                                .push(
                                    root.ai.realtime.UserTranscript {
                                        item_id: baml.json.path_or(event, ".item_id", ""),
                                        text: baml.json.path_or(event, ".transcript", ""),
                                    },
                                );
                            batch_done = true;
                        }
                        if (kind == "conversation.item.input_audio_transcription.failed") {
                            self.awaiting_input_transcript = false;
                            batch
                                .push(
                                    root.ai.realtime.UserTranscriptFailed {
                                        item_id: baml.json.path_or(event, ".item_id", ""),
                                        message: baml
                                            .json
                                            .path_or(
                                                event,
                                                ".error.message",
                                                "input transcription failed",
                                            ),
                                    },
                                );
                            batch_done = true;
                        }
                        //### Provider VAD surfaces speech boundaries while the session stays open
                        if (kind == "input_audio_buffer.speech_started") {
                            self.awaiting_input_transcript = self.input_transcription_enabled;
                            batch
                                .push(
                                    root.ai.realtime.UserSpeechStarted {
                                        item_id: baml.json.path_or(event, ".item_id", ""),
                                        audio_start_ms: baml.json.path_or(event, ".audio_start_ms", 0),
                                    },
                                );
                            // Return immediately so a caller can stop queued speaker output.
                            batch_done = true;
                        }
                        if (kind == "input_audio_buffer.speech_stopped") {
                            batch
                                .push(
                                    root.ai.realtime.UserSpeechStopped {
                                        item_id: baml.json.path_or(event, ".item_id", ""),
                                        audio_end_ms: baml.json.path_or(event, ".audio_end_ms", 0),
                                    },
                                );
                            batch_done = true;
                        }
                        //### Retain the assistant item needed for precise truncation
                        if (kind == "response.output_item.added") {
                            let item_type = baml.json.path_or(event, ".item.type", "");
                            if (item_type == "message") {
                                self.last_assistant_item_id
                                    = baml.json.path_or(event, ".item.id", "");
                                match (self.last_assistant_item_id) {
                                    let item_id: string => {
                                        self.assistant_audio_bytes_by_item_id.set(item_id, 0);
                                    },
                                    null => {},
                                }
                                if (self.output_audio && !self.audio_boundary_emitted) {
                                    self.audio_boundary_emitted = true;
                                    batch
                                        .push(
                                            root.ai.realtime.AssistantAudioStarted {
                                                item_id: self.last_assistant_item_id ?? "",
                                            },
                                        );
                                    batch_done = true;
                                }
                            }
                        }
                        //### Preserve realtime tool-call correlation
                        if (kind == "response.function_call_arguments.done") {
                            let raw_args = baml.json.path_or(event, ".arguments", "{}");
                            let args = baml.json.parse(raw_args) catch (e) {
                                _ => {
                                    throw baml.errors.UnknownError.with_message<never>(raw_args, "realtime tool arguments were not JSON")
                                },
                            };
                            self.pending_tool_calls
                                .push(
                                    root.ai.tools.ToolCall {
                                        id: baml.json.path_or(event, ".call_id", ""),
                                        name: baml.json.path_or(event, ".name", ""),
                                        args: args,
                                    },
                                );
                        }
                        //### End the batch at a response boundary
                        if (kind == "response.done" || kind == "response.completed") {
                            self.response_finished = true;
                            // For an audio turn, wait for the separately-generated ASR event
                            // even when it is delivered after the response boundary.
                            if (!self.awaiting_input_transcript) {
                                self.finish_response_batch(batch);
                                batch_done = true;
                            }
                        }
                        // response.done may have arrived first; the transcript now completes
                        // the same turn. OpenAI does not guarantee their relative order.
                        if (
                            self.response_finished
                                && (
                                    kind
                                        == "conversation.item.input_audio_transcription.completed"
                                        || kind
                                            == "conversation.item.input_audio_transcription.failed"
                                )
                        ) {
                            self.finish_response_batch(batch);
                            batch_done = true;
                        }
                        let error_code = baml.json.path_or(event, ".error.code", "");
                        // response.done and a client response.cancel can cross on the
                        // wire. The server's no-active-response reply confirms the
                        // response is already stopped; local playback still needs its
                        // following conversation.item.truncate.
                        if (kind == "error" && error_code == "response_cancel_not_active") {
                            self.response_finished = true;
                        } else if (kind == "error" || kind == "response.failed") {
                            self.close();
                            throw baml.errors.UnknownError.with_message<never>(raw, "realtime error event");
                        }
                    },
                }
            }
            self.remember(batch)
        }

        function send_text(self, text: string) -> null throws baml.errors.UnknownError {
            self.awaiting_input_transcript = false;
            self.send_frame(
                baml.json
                    .stringify(
                        {
                            "type": "conversation.item.create",
                            "item": {
                                "type": "message",
                                "role": "user",
                                "content": [{ "type": "input_text", "text": text }],
                            },
                        },
                    ),
                "realtime text turn failed",
            );
            self.start_response()
        }

        function input_audio_format(self) -> root.ai.realtime.RealtimeAudioFormat throws never {
            root.openai.realtime_audio_format()
        }

        function output_audio_format(self) -> root.ai.realtime.RealtimeAudioFormat throws never {
            root.openai.realtime_audio_format()
        }

        function send_audio(
            self,
            pcm16_24khz_mono: uint8array,
        ) -> null throws baml.errors.UnknownError {
            if (pcm16_24khz_mono.length() == 0) {
                return null;
            }
            self.send_frame(
                baml.json
                    .stringify(
                        {
                            "type": "input_audio_buffer.append",
                            "audio": pcm16_24khz_mono.to_base64(),
                        },
                    ),
                "realtime input audio append failed",
            )
        }

        function commit_audio(self) -> null throws baml.errors.UnknownError {
            if (self.automatic_turn_detection) {
                throw baml.errors.UnknownError.with_message<never>(null, "server VAD commits audio turns automatically");
            }
            self.awaiting_input_transcript = self.input_transcription_enabled;
            self.send_frame(
                baml.json.stringify({ "type": "input_audio_buffer.commit" }),
                "realtime input audio commit failed",
            );
            self.start_response()
        }

        function send_audio_turn(
            self,
            pcm16_24khz_mono: uint8array,
        ) -> null throws baml.errors.UnknownError {
            if (pcm16_24khz_mono.length() == 0) {
                throw baml.errors.UnknownError.with_message<never>(null, "cannot send an empty realtime audio turn");
            }
            self.send_frame(
                baml.json.stringify({ "type": "input_audio_buffer.clear" }),
                "realtime input audio clear failed",
            );
            let offset = 0;
            let chunk_size = 24_000;
            while (offset < pcm16_24khz_mono.length()) {
                let end = offset + chunk_size;
                if (end > pcm16_24khz_mono.length()) {
                    end = pcm16_24khz_mono.length();
                }
                self.send_audio(pcm16_24khz_mono.slice(offset, end));
                offset = end;
            }
            self.commit_audio()
        }

        function submit_tool_results(
            self,
            results: root.ai.tools.ToolResult[],
        ) -> null throws baml.errors.UnknownError {
            //# Splice correlated tool outputs into the open conversation
            for (let result in results) {
                self.submitted_results.push(result);
                self.send_frame(
                    baml.json
                        .stringify(
                            {
                                "type": "conversation.item.create",
                                "item": {
                                    "type": "function_call_output",
                                    "call_id": result.id,
                                    "output": baml.json.stringify(result.output),
                                },
                            },
                        ),
                    "realtime tool result send failed",
                );
            }
            //## Request the provider's continuation separately
            self.start_response();
            self.remember([root.ai.realtime.LiveToolResults { results: results }]);
            null
        }

        function request_response(self) -> null throws baml.errors.UnknownError {
            self.start_response()
        }

        function cancel_response(self) -> null throws baml.errors.UnknownError {
            if (self.response_finished) {
                return null;
            }
            //# Cancel the active provider response, not the whole session
            self.send_frame(
                baml.json.stringify({ "type": "response.cancel" }),
                "realtime response.cancel failed",
            );
            self.response_finished = true;
            self.cancel_requests = self.cancel_requests + 1;
            null
        }

        function truncate_assistant_audio(
            self,
            played_ms: int,
        ) -> null throws baml.errors.UnknownError {
            let item_id = match (self.last_assistant_item_id) {
                let value: string => value,
                null => {
                    throw baml.errors.UnknownError.with_message<never>(played_ms, "cannot truncate before an assistant audio item starts")
                },
            };
            let received_audio_bytes = self.assistant_audio_bytes_by_item_id.get(item_id) ?? 0;
            let audio_end_ms = _realtime_audio_end_ms(played_ms, received_audio_bytes);
            //# Rewrite provider memory to exactly what the caller played
            self.send_frame(
                baml.json
                    .stringify(
                        {
                            "type": "conversation.item.truncate",
                            "item_id": item_id,
                            "content_index": 0,
                            "audio_end_ms": audio_end_ms,
                        },
                    ),
                "realtime conversation.item.truncate failed",
            );
            self.truncate_positions_ms.push(audio_end_ms);
            null
        }

        function close(self) -> null throws never {
            if (!self.is_closed) {
                match (self.socket) {
                    let ws: baml.ws.WsStream => ws.close(),
                    null => null,
                };
                self.socket = null;
                self.channel.close();
                self.is_closed = true;
            }
            null
        }
    }

    implements root.ai.Resource {}
}
