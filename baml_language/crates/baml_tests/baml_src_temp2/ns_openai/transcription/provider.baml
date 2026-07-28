// A finite-audio adapter over OpenAI's Chat Completions audio-input shape. The
// current guide surface intentionally accepts one bounded audio item per call;
// realtime/multi-turn input belongs to ai.RealtimeProvider instead.
class AudioTranscription {
    inner: Chat,
    received_chunks: int,

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "openai-audio-transcription"
        }

        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }

    implements root.ai.TranscriptionProvider {
        function transcribe(
            self,
            stream: root.ai.AudioStream,
            options: root.ai.TranscriptionOptions,
        ) -> root.ai.ResponseWithMetadata<string> throws baml.errors.UnknownError {
            let chunks = stream.collect();
            if (chunks.length() != 1) {
                throw baml.errors.UnknownError {
          data: chunks.length(),
          message: ["bounded transcription currently requires exactly one audio chunk"],
        };
            }
            self.received_chunks = chunks.length();
            let chunk = chunks[0];
            let language = options.language ?? "auto-detect";
            let hint = options.prompt ?? "none";
            let key = match (self.inner.api_key) {
                let value: string => value,
                null => {
                    throw baml.errors.UnknownError {
            data: null,
            message: ["OpenAI transcription requires api_key"],
          }
                },
            };
            let request_body: map<string, json> = {
                "model": self.inner.model,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": `Transcribe the spoken words exactly. Language: ${language}. Context hint: ${hint}. Return only the transcript.`,
                            },
                            {
                                "type": "input_audio",
                                "input_audio": { "data": chunk.base64(), "format": "wav" },
                            },
                        ],
                    },
                ],
            };
            log.info(
                baml.json
                    .stringify(
                        {
                            "event": "audio.transcription.request",
                            "provider": "openai",
                            "model": self.inner.model,
                            "chunks": self.received_chunks,
                        },
                    ),
            );
            let http_response = baml
                .http
                .send(
                    baml.http.Request {
                        method: "POST",
                        url: (self.inner.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                        headers: { "Authorization": "Bearer " + key, "Content-Type": "application/json" },
                        body: baml.json.stringify(request_body),
                    },
                ) catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
            data: e,
            message: ["OpenAI transcription send failed"],
          }
                },
            };
            let body = http_response.text() catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
            data: e,
            message: ["OpenAI transcription body read failed"],
          }
                },
            };
            log.info(
                baml.json
                    .stringify(
                        {
                            "event": "audio.transcription.response",
                            "provider": "openai",
                            "model": self.inner.model,
                            "status_code": http_response.status_code,
                            "response_bytes": body.length(),
                        },
                    ),
            );
            if (!http_response.ok()) {
                throw baml.errors.UnknownError {
          data: body,
          message: ["OpenAI transcription http " + http_response.status_code.to_string()],
        };
            }
            let raw: json = baml.json.parse(body) catch (e) {
                _ => {
                    throw baml.errors.UnknownError {
            data: e,
            message: ["OpenAI transcription JSON parse failed"],
          }
                },
            };
            let transcript = baml.json.path_or<string>(raw, ".choices[0].message.content", "");
            if (transcript.length() == 0) {
                throw baml.errors.UnknownError {
          data: raw,
          message: ["OpenAI transcription returned no text"],
        };
            }
            root.ai.ResponseWithMetadata<string> {
                value: transcript,
                meta: root.ai.ResponseMetadata {
                    provider: self.name(),
                    model: self.inner.model,
                    request_id: baml.json.path_or<string?>(raw, ".id", null),
                    finish_reason: baml.json.path_or<string?>(raw, ".choices[0].finish_reason", null),
                    usage: root.ai.Usage {
                        input_tokens: baml.json.path_or(raw, ".usage.prompt_tokens", 0),
                        output_tokens: baml.json.path_or(raw, ".usage.completion_tokens", 0),
                        cached_input_tokens: baml
                            .json
                            .path_or<int?>(raw, ".usage.prompt_tokens_details.cached_tokens", null),
                        reasoning_tokens: baml
                            .json
                            .path_or<int?>(
                                raw,
                                ".usage.completion_tokens_details.reasoning_tokens",
                                null,
                            ),
                        cost_usd: null,
                    },
                    attributes: {
                        "capability": "bounded_transcription",
                        "chunks": self.received_chunks,
                        "sample_rate_hz": stream.sample_rate_hz,
                        "channels": stream.channels,
                    },
                    raw: raw,
                },
                conversation: null,
            }
        }
    }
}
