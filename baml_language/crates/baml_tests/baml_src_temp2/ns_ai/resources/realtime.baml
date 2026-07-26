interface Channel {
  function send(self, frame: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
}

class RecordingChannel {
    frames: string[],
    implements Channel {
        function send(self, frame: string) -> null throws never {
            self.frames.push(frame);
            null
        }
        function close(self) -> null throws never {
            null
        }
    }
}

interface LiveEvent {
  function kind(self) -> string throws never
}

class TranscriptDelta {
    text: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "transcript_delta"
        }
    }
}

class UserTranscript {
    item_id: string,
    text: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_transcript"
        }
    }
}

class UserTranscriptFailed {
    item_id: string,
    message: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_transcript_failed"
        }
    }
}

class UserSpeechStarted {
    item_id: string,
    audio_start_ms: int,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_speech_started"
        }
    }
}

class UserSpeechStopped {
    item_id: string,
    audio_end_ms: int,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "user_speech_stopped"
        }
    }
}

class AssistantAudioStarted {
    item_id: string,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "assistant_audio_started"
        }
    }
}

class AssistantAudioDelta {
    audio: uint8array,
    implements LiveEvent {
        function kind(self) -> string throws never {
            "assistant_audio_delta"
        }
    }
}

class LiveToolCalls {
    calls: ToolCall[],
    implements LiveEvent {
        function kind(self) -> string throws never {
            "tool_calls"
        }
    }
}

class LiveToolResults {
    results: ToolResult[],
    implements LiveEvent {
        function kind(self) -> string throws never {
            "tool_results"
        }
    }
}

class LiveTurnCompleted {
    implements LiveEvent {
        function kind(self) -> string throws never {
            "turn_completed"
        }
    }
}

class LiveClosed {
    implements LiveEvent {
        function kind(self) -> string throws never {
            "closed"
        }
    }
}

interface LiveSession requires Resource {
  function receive(self) -> LiveEvent[] throws baml.errors.UnknownError
  function send_text(self, text: string) -> null throws baml.errors.UnknownError
  // Append one microphone frame without ending the user's turn. This is the
  // normal path for continuous input with provider-side VAD.
  function send_audio(self, pcm16_24khz_mono: uint8array) -> null
    throws baml.errors.UnknownError
  // End a manually-delimited audio turn. Do not call this when server VAD is
  // configured: the provider commits speech at its detected boundary.
  function commit_audio(self) -> null throws baml.errors.UnknownError
  // OpenAI Realtime's `audio/pcm` wire format: signed PCM16, mono, 24 kHz.
  // Convenience for bounded clips; equivalent to clear + send_audio + commit.
  function send_audio_turn(self, pcm16_24khz_mono: uint8array) -> null
    throws baml.errors.UnknownError
  function submit_tool_results(self, results: ToolResult[]) -> null throws baml.errors.UnknownError
  // Start a response for the committed conversation state. This is used with
  // server VAD when create_response=false so a runner can own turn policy.
  function request_response(self) -> null throws baml.errors.UnknownError
  function cancel_response(self) -> null throws baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError
  function close(self) -> null throws never
}

// Runner-owned policy wrapper for ordinary function-backed task tools. The
// provider session remains responsible for wire correlation and transcript
// mutation; this wrapper only dispatches calls and submits their results.
class AutomaticToolLiveSession {
    inner: LiveSession,
    tools: Tool[],

    implements LiveSession {
        function receive(self) -> LiveEvent[] throws baml.errors.UnknownError {
            let events = self.inner.receive();
            let calls: ToolCall[] = [];
            for (let event in events) {
                match (event) {
                    let requested: LiveToolCalls => {
                        for (let call in requested.calls) {
                            calls.push(call);
                        }
                    },
                    _ => {},
                }
            }
            if (calls.length() == 0) {
                return events;
            }
            //# Execute the task's concrete tools, preserving provider call IDs
            let results = calls.map((call: ToolCall) -> ToolResult {
                match (
                    self.tools.find((tool: Tool) -> bool {
                        tool.name == call.name
                    })
                ) {
                    let tool: Tool => invoke_tool(tool, call),
                    null => ToolResult.error(call, "tool is not registered on this task"),
                }
            });
            //## Add correlated results to the provider transcript and continue
            self.inner.submit_tool_results(results);
            events.push(LiveToolResults { results: results });
            events
        }

        function send_text(self, text: string) -> null throws baml.errors.UnknownError {
            self.inner.send_text(text)
        }

        function send_audio(
            self,
            pcm16_24khz_mono: uint8array,
        ) -> null throws baml.errors.UnknownError {
            self.inner.send_audio(pcm16_24khz_mono)
        }

        function commit_audio(self) -> null throws baml.errors.UnknownError {
            self.inner.commit_audio()
        }

        function send_audio_turn(
            self,
            pcm16_24khz_mono: uint8array,
        ) -> null throws baml.errors.UnknownError {
            self.inner.send_audio_turn(pcm16_24khz_mono)
        }

        function submit_tool_results(
            self,
            results: ToolResult[],
        ) -> null throws baml.errors.UnknownError {
            self.inner.submit_tool_results(results)
        }

        function request_response(self) -> null throws baml.errors.UnknownError {
            self.inner.request_response()
        }

        function cancel_response(self) -> null throws baml.errors.UnknownError {
            self.inner.cancel_response()
        }

        function truncate_assistant_audio(
            self,
            played_ms: int,
        ) -> null throws baml.errors.UnknownError {
            self.inner.truncate_assistant_audio(played_ms)
        }

        function close(self) -> null throws never {
            self.inner.close()
        }
    }
    implements Resource {}
}

// Application-owned audio I/O. next_input() yields short PCM frames from the
// microphone; play_output() queues provider audio without blocking reception.
// stop_output() drops queued audio and reports how many milliseconds the caller
// actually heard from the current assistant item.
interface RealtimeAudioDevice requires Resource {
  function next_input(self) -> uint8array? throws baml.errors.UnknownError
  function play_output(self, pcm16_24khz_mono: uint8array) -> null
    throws baml.errors.UnknownError
  // The current assistant response has finished producing audio. A device may
  // now flush or begin playing the chunks queued by play_output().
  function finish_output(self) -> null throws baml.errors.UnknownError
  function stop_output(self) -> int throws baml.errors.UnknownError
  function close(self) -> null throws never
}

// Deterministic audio device for orchestration tests and examples.
class BufferedRealtimeAudioDevice {
    input_chunks: uint8array[],
    next_input_index: int,
    output_chunks: uint8array[],
    played_ms: int,
    is_closed: bool,

    implements RealtimeAudioDevice {
        function next_input(self) -> uint8array? throws never {
            if (self.next_input_index >= self.input_chunks.length()) {
                return null;
            }
            let chunk = self.input_chunks[self.next_input_index];
            self.next_input_index = self.next_input_index + 1;
            chunk
        }
        function play_output(self, pcm16_24khz_mono: uint8array) -> null throws never {
            self.output_chunks.push(pcm16_24khz_mono);
            null
        }
        function finish_output(self) -> null throws never {
            null
        }
        function stop_output(self) -> int throws never {
            let heard = self.played_ms;
            self.output_chunks = [];
            heard
        }
        function close(self) -> null throws never {
            self.is_closed = true;
            null
        }
    }
    implements Resource {}
}

interface RealtimeProvider requires Provider {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> LiveSession
    throws baml.errors.UnknownError
}

// A deterministic LiveSession used by unit tests. Control frames are recorded through
// the same Channel contract as a real provider so ordering can be asserted.
class BufferedLiveSession {
    items: LiveEvent[],
    channel: Channel,
    is_closed: bool,
    control_frames: string[],

    // function cleanup(self) -> void throws never { self.close(); }

    implements LiveSession {
        function receive(self) -> LiveEvent[] throws never {
            self.items
        }
        function send_text(self, text: string) -> null throws never {
            let frame = baml.json.stringify({ "type": "conversation.item.create", "text": text });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function send_audio(self, pcm16_24khz_mono: uint8array) -> null throws never {
            let frame = baml
                .json
                .stringify(
                    { "type": "input_audio_buffer.append", "bytes": pcm16_24khz_mono.length() },
                );
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function commit_audio(self) -> null throws never {
            let frame = baml.json.stringify({ "type": "input_audio_buffer.commit" });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function send_audio_turn(self, pcm16_24khz_mono: uint8array) -> null throws never {
            let frame = baml
                .json
                .stringify(
                    { "type": "input_audio_buffer.commit", "bytes": pcm16_24khz_mono.length() },
                );
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function submit_tool_results(self, results: ToolResult[]) -> null throws never {
            let frame = baml.json.stringify({ "type": "tool.results", "count": results.length() });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function request_response(self) -> null throws never {
            let frame = baml.json.stringify({ "type": "response.create" });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function cancel_response(self) -> null throws never {
            let frame = baml.json.stringify({ "type": "response.cancel" });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function truncate_assistant_audio(self, played_ms: int) -> null throws never {
            let frame = baml.json.stringify({ "type": "conversation.item.truncate", "audio_end_ms": played_ms });
            self.control_frames.push(frame);
            self.channel.send(frame) catch (e) {
                _ => null
            };
            null
        }
        function close(self) -> null throws never {
            if (!self.is_closed) {
                self.channel.close();
                self.is_closed = true;
            }
            null
        }
    }
    implements Resource {}
}

class FakeRealtimeProvider {
    transcript: string,
    implements Provider {
        function name(self) -> string throws never {
            "fake-realtime"
        }
        function render_shorthand(self) -> string throws never {
            "openai/gpt-5.6-luna"
        }
    }
    implements RealtimeProvider {
        function open_live<T>(self, task: Task<T>, channel: Channel) -> LiveSession throws never {
            channel.send(baml.json.stringify({ "type": "session.created" })) catch (e) {
                _ => null
            };
            BufferedLiveSession {
                items: [TranscriptDelta { text: self.transcript }, LiveClosed {  }],
                channel: channel,
                is_closed: false,
                control_frames: [],
            }
        }
    }
}

class OpenAiRealtime {
    model: string,
    voice: string,
    api_key: string,
    // null/false is text-only. true requests audio plus its provider transcript.
    output_audio: bool?,
    // Realtime session instructions, separate from the task's first user turn.
    instructions: string?,
    // Optional asynchronous input ASR. For example: gpt-4o-mini-transcribe.
    input_transcription_model: string?,
    // null means caller-delimited turns. A value enables provider-side speech
    // detection; its policy flags decide whether responses and interruption
    // remain automatic or are owned by the runner.
    server_vad: RealtimeServerVad?,
    // Keep the socket open after an ordinary response so callers can send turns.
    keep_open: bool?,
    implements Provider {
        function name(self) -> string throws never {
            "openai-realtime"
        }
        function render_shorthand(self) -> string throws never {
            "openai/" + self.model
        }
    }
}

class RealtimeServerVad {
    threshold: float?,
    prefix_padding_ms: int?,
    silence_duration_ms: int?,
    // These default to OpenAI's ordinary hands-free conversation behavior.
    // Set interrupt_response=false when a runner owns a delayed barge-in policy.
    create_response: bool?,
    interrupt_response: bool?,
}

function openai_realtime_turn_detection(vad: RealtimeServerVad?) -> json throws never {
    match (vad) {
        let config: RealtimeServerVad => {
            {
                "type": "server_vad",
                "threshold": config.threshold ?? 0.5,
                "prefix_padding_ms": config.prefix_padding_ms ?? 300,
                "silence_duration_ms": config.silence_duration_ms ?? 500,
                "create_response": config.create_response ?? true,
                "interrupt_response": config.interrupt_response ?? true,
            }
        },
        null => null,
    }
}

function openai_realtime_tool_schemas(tools: Tool[]) -> json[] throws never {
    tools.map((tool: Tool) -> json {
        {
            "type": "function",
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

function openai_realtime_response_create(
    tools: Tool[],
    output_audio: bool,
    voice: string,
) -> string throws never {
    let schemas = openai_realtime_tool_schemas(tools);
    if (output_audio) {
        if (schemas.length() > 0) {
            baml.json
                .stringify(
                    {
                        "type": "response.create",
                        "response": {
                            "output_modalities": ["audio"],
                            "audio": { "output": { "voice": voice } },
                            "tools": schemas,
                            "tool_choice": "auto",
                        },
                    },
                )
        } else {
            baml.json
                .stringify(
                    {
                        "type": "response.create",
                        "response": {
                            "output_modalities": ["audio"],
                            "audio": { "output": { "voice": voice } },
                        },
                    },
                )
        }
    } else {
        if (schemas.length() > 0) {
            baml.json
                .stringify(
                    {
                        "type": "response.create",
                        "response": { "output_modalities": ["text"], "tools": schemas, "tool_choice": "auto" },
                    },
                )
        } else {
            baml.json
                .stringify(
                    { "type": "response.create", "response": { "output_modalities": ["text"] } },
                )
        }
    }
}

function openai_realtime_session_update(
    instructions: string,
    voice: string,
    transcription_model: string?,
    server_vad: RealtimeServerVad?,
) -> string throws never {
    let turn_detection = openai_realtime_turn_detection(server_vad);
    match (transcription_model) {
        let model: string => {
            return baml
                .json
                .stringify(
                    {
                        "type": "session.update",
                        "session": {
                            "type": "realtime",
                            "instructions": instructions,
                            "audio": {
                                "input": {
                                    "format": { "type": "audio/pcm", "rate": 24_000 },
                                    "noise_reduction": { "type": "near_field" },
                                    "transcription": { "model": model },
                                    "turn_detection": turn_detection,
                                },
                                "output": {
                                    "format": { "type": "audio/pcm", "rate": 24_000 },
                                    "voice": voice,
                                },
                            },
                        },
                    },
                );
        },
        null => {
            return baml
                .json
                .stringify(
                    {
                        "type": "session.update",
                        "session": {
                            "type": "realtime",
                            "instructions": instructions,
                            "audio": {
                                "input": {
                                    "format": { "type": "audio/pcm", "rate": 24_000 },
                                    "noise_reduction": { "type": "near_field" },
                                    "turn_detection": turn_detection,
                                },
                                "output": {
                                    "format": { "type": "audio/pcm", "rate": 24_000 },
                                    "voice": voice,
                                },
                            },
                        },
                    },
                );
        },
    }
}

// The WebSocket remains inside this Resource. receive(), controls, and tool
// submission therefore all address the same provider session until it closes.
class OpenAiRealtimeSession {
    socket: baml.ws.WsStream?,
    channel: Channel,
    tools: Tool[],
    voice: string,
    output_audio: bool,
    keep_open: bool,
    input_transcription_enabled: bool,
    automatic_turn_detection: bool,
    items: LiveEvent[],
    submitted_results: ToolResult[],
    is_closed: bool,
    audio_boundary_emitted: bool,
    awaiting_input_transcript: bool,
    last_assistant_item_id: string?,
    pending_tool_calls: ToolCall[],
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
                    _ => throw baml.errors.UnknownError { data: e, message: [label] },
                };
                self.channel.send(frame) catch (e) {
                    _ => null
                };
                null
            },
            null => {
                throw baml.errors.UnknownError {
        data: null,
        message: ["realtime session is closed"],
      }
            },
        }
    }

    function remember(self, batch: LiveEvent[]) -> LiveEvent[] throws never {
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
            openai_realtime_response_create(self.tools, self.output_audio, self.voice),
            "realtime response.create failed",
        )
    }

    function finish_response_batch(self, batch: LiveEvent[]) -> null throws never {
        if (self.pending_tool_calls.length() > 0) {
            batch.push(LiveToolCalls { calls: self.pending_tool_calls });
            self.pending_tool_calls = [];
        } else if (self.keep_open) {
            batch.push(LiveTurnCompleted {  });
        } else {
            self.close();
            batch.push(LiveClosed {  });
        }
        null
    }

    implements LiveSession {
        // Return the next meaningful normalized event batch. Text/audio deltas
        // yield immediately so callers can log and play them while the response is
        // still in flight. Tool calls yield together at the response boundary.
        function receive(self) -> LiveEvent[] throws baml.errors.UnknownError {
            if (self.is_closed) {
                return [];
            }
            let ws = match (self.socket) {
                let value: baml.ws.WsStream => value,
                null => {
                    self.is_closed = true;
                    return self.remember([LiveClosed {  }]);
                },
            };
            let batch: LiveEvent[] = [];
            let batch_done = false;
            //# Receive one realtime event batch
            while (!batch_done) {
                //## Read the next provider event
                let next = ws.next() catch (e) {
                    _ => {
                        throw baml.errors.UnknownError {
            data: e,
            message: ["realtime receive failed"],
          }
                    },
                };
                //## Normalize the provider event
                match (next) {
                    null => {
                        self.close();
                        batch.push(LiveClosed {  });
                        batch_done = true;
                    },
                    let raw: string => {
                        self.channel.send(raw) catch (e) {
                            _ => null
                        };
                        let event = baml.json.parse(raw) catch (e) {
                            _ => {
                                throw baml.errors.UnknownError {
                data: raw,
                message: ["realtime event JSON failed"],
              }
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
                                    TranscriptDelta { text: baml.json.path_or(event, ".delta", "") },
                                );
                            batch_done = true;
                        }
                        //### Surface provider-generated assistant audio for caller playback
                        if (kind == "response.output_audio.delta" || kind == "response.audio.delta") {
                            let encoded = baml.json.path_or(event, ".delta", "");
                            let audio = baml.Uint8Array.from_base64(encoded) catch (e) {
                                _ => {
                                    throw baml.errors.UnknownError {
                  data: encoded,
                  message: ["realtime audio delta was not valid base64"],
                }
                                },
                            };
                            batch.push(AssistantAudioDelta { audio: audio });
                            batch_done = true;
                        }
                        //### Input transcription is asynchronous and may follow response.done
                        if (kind == "conversation.item.input_audio_transcription.completed") {
                            self.awaiting_input_transcript = false;
                            batch
                                .push(
                                    UserTranscript {
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
                                    UserTranscriptFailed {
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
                                    UserSpeechStarted {
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
                                    UserSpeechStopped {
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
                                if (self.output_audio && !self.audio_boundary_emitted) {
                                    self.audio_boundary_emitted = true;
                                    batch
                                        .push(
                                            AssistantAudioStarted {
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
                                    throw baml.errors.UnknownError {
                  data: raw_args,
                  message: ["realtime tool arguments were not JSON"],
                }
                                },
                            };
                            self.pending_tool_calls
                                .push(
                                    ToolCall {
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
                            throw baml.errors.UnknownError {
                data: raw,
                message: ["realtime error event"],
              };
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
                throw baml.errors.UnknownError {
          data: null,
          message: ["server VAD commits audio turns automatically"],
        };
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
                throw baml.errors.UnknownError {
          data: null,
          message: ["cannot send an empty realtime audio turn"],
        };
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
            results: ToolResult[],
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
            self.remember([LiveToolResults { results: results }]);
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
                    throw baml.errors.UnknownError {
          data: played_ms,
          message: ["cannot truncate before an assistant audio item starts"],
        }
                },
            };
            //# Rewrite provider memory to exactly what the caller played
            self.send_frame(
                baml.json
                    .stringify(
                        {
                            "type": "conversation.item.truncate",
                            "item_id": item_id,
                            "content_index": 0,
                            "audio_end_ms": played_ms,
                        },
                    ),
                "realtime conversation.item.truncate failed",
            );
            self.truncate_positions_ms.push(played_ms);
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

    implements Resource {}
}

implements RealtimeProvider for OpenAiRealtime {
  function open_live<T>(self, task: Task<T>, channel: Channel) -> LiveSession
      throws baml.errors.UnknownError {
    //# Open and retain the realtime WebSocket
    let ws = baml.ws.connect(
      "wss://api.openai.com/v1/realtime?model=" + self.model,
      { "Authorization": "Bearer " + self.api_key },
    ) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["realtime connect failed"],
      },
    };
    //## Configure the persistent voice session before adding conversation items
    let session_update = openai_realtime_session_update(
      self.instructions ?? "",
      self.voice,
      self.input_transcription_model,
      self.server_vad,
    );
    ws.send(session_update) catch (e) {
      _ => {
        ws.close();
        throw baml.errors.UnknownError {
          data: e,
          message: ["realtime session.update failed"],
        };
      },
    };
    channel.send(session_update) catch (e) { _ => null };
    let initial_item = baml.json.stringify({
      "type": "conversation.item.create",
      "item": {
        "type": "message",
        "role": "user",
        "content": [{
          "type": "input_text",
          "text": root.ai.prompt_text_content(task.prompt),
        }],
      },
    });
    //## Add the caller-owned conversation item
    ws.send(initial_item) catch (e) {
      _ => {
        ws.close();
        throw baml.errors.UnknownError {
          data: e,
          message: ["realtime initial item failed"],
        };
      },
    };
    channel.send(initial_item) catch (e) { _ => null };
    let output_audio = self.output_audio ?? false;
    let response_create = openai_realtime_response_create(task.tools, output_audio, self.voice);
    //## Start inference without draining or closing the resource
    ws.send(response_create) catch (e) {
      _ => {
        ws.close();
        throw baml.errors.UnknownError {
          data: e,
          message: ["realtime response.create failed"],
        };
      },
    };
    channel.send(response_create) catch (e) { _ => null };
    OpenAiRealtimeSession {
      socket: ws,
      channel: channel,
      tools: task.tools,
      voice: self.voice,
      output_audio: output_audio,
      keep_open: self.keep_open ?? false,
      input_transcription_enabled: self.input_transcription_model != null,
      automatic_turn_detection: self.server_vad != null,
      items: [],
      submitted_results: [],
      is_closed: false,
      audio_boundary_emitted: false,
      awaiting_input_transcript: false,
      last_assistant_item_id: null,
      pending_tool_calls: [],
      response_finished: false,
      cancel_requests: 0,
      truncate_positions_ms: [],
    }
  }
}

function collect_live_text(live_session: LiveSession) -> string throws baml.errors.UnknownError {
    let text = "";
    let done = false;
    //# Drain event batches until the live session closes
    while (!done) {
        let batch = live_session.receive();
        if (batch.length() == 0) {
            done = true;
        }
        for (let event in batch) {
            match (event) {
                let delta: TranscriptDelta => {
                    text = text + delta.text;
                },
                let closed: LiveClosed => {
                    done = true;
                },
                _ => {},
            }
        }
    }
    text
}

class LiveTurn {
    events: LiveEvent[],
    assistant_transcript: string,
    user_transcript: string?,
    assistant_audio: uint8array[],
    tool_calls: ToolCall[],
    tool_results: ToolResult[],
    is_closed: bool,
}

// Collect one response while preserving its transcript, audio, and tool calls.
function collect_live_turn(live_session: LiveSession) -> LiveTurn throws baml.errors.UnknownError {
    let events: LiveEvent[] = [];
    let assistant_transcript = "";
    let user_transcript: string? = null;
    let assistant_audio: uint8array[] = [];
    let tool_calls: ToolCall[] = [];
    let tool_results: ToolResult[] = [];
    let is_closed = false;
    let done = false;
    while (!done) {
        let batch = live_session.receive();
        if (batch.length() == 0) {
            done = true;
        }
        for (let event in batch) {
            events.push(event);
            match (event) {
                let delta: TranscriptDelta => {
                    assistant_transcript = assistant_transcript + delta.text;
                },
                let transcript: UserTranscript => {
                    user_transcript = transcript.text;
                },
                let audio: AssistantAudioDelta => {
                    assistant_audio.push(audio.audio);
                },
                let calls: LiveToolCalls => {
                    for (let call in calls.calls) {
                        tool_calls.push(call);
                    }
                    done = true;
                },
                let results: LiveToolResults => {
                    for (let result in results.results) {
                        tool_results.push(result);
                    }
                    // AutomaticToolLiveSession has already submitted these results and
                    // requested the model's continuation, so keep collecting this turn.
                    done = false;
                },
                let completed: LiveTurnCompleted => {
                    done = true;
                },
                let closed: LiveClosed => {
                    is_closed = true;
                    done = true;
                },
                _ => {},
            }
        }
    }
    LiveTurn {
        events: events,
        assistant_transcript: assistant_transcript,
        user_transcript: user_transcript,
        assistant_audio: assistant_audio,
        tool_calls: tool_calls,
        tool_results: tool_results,
        is_closed: is_closed,
    }
}
