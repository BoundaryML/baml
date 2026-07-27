class Realtime {
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
    implements root.ai.Provider {
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

// OpenAI Realtime currently exchanges signed little-endian PCM16, mono,
// at 24 kHz. The provider package owns this requirement; portable ai runners
// only compare the session and device declarations.
function realtime_audio_format() -> root.ai.RealtimeAudioFormat throws never {
    root.ai.RealtimeAudioFormat { encoding: "pcm_s16le", sample_rate_hz: 24_000, channels: 1 }
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

function openai_realtime_tool_schemas(tools: root.ai.Tool[]) -> json[] throws never {
    tools.map((tool: root.ai.Tool) -> json {
        {
            "type": "function",
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

function openai_realtime_response_create(
    tools: root.ai.Tool[],
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
    let audio_format = realtime_audio_format();
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
                                    "format": { "type": "audio/pcm", "rate": audio_format.sample_rate_hz },
                                    "noise_reduction": { "type": "near_field" },
                                    "transcription": { "model": model },
                                    "turn_detection": turn_detection,
                                },
                                "output": {
                                    "format": { "type": "audio/pcm", "rate": audio_format.sample_rate_hz },
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
                                    "format": { "type": "audio/pcm", "rate": audio_format.sample_rate_hz },
                                    "noise_reduction": { "type": "near_field" },
                                    "turn_detection": turn_detection,
                                },
                                "output": {
                                    "format": { "type": "audio/pcm", "rate": audio_format.sample_rate_hz },
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
