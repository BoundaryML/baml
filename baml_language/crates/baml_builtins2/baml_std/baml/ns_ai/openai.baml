// The OpenAI provider — the first concrete implementation of the capability spine,
// written entirely in BAML (request/response logic in BAML; only leaf primitives —
// `baml.http.send`, `baml.json.*`, `baml.env` — are host functions). This is the
// reference every other provider (and a user's custom provider) follows.

// --- wire shapes (Chat Completions) ---
class OpenAiMessageWire {
    content: string,
}

class OpenAiChoiceWire {
    message: OpenAiMessageWire,
    finish_reason: string?,
}

class OpenAiResponseWire {
    choices: OpenAiChoiceWire[],
}

/// Lazy `ResponseMeta` over the raw HTTP response — each dimension re-reads the body.
class OpenAiResponseMeta {
    raw: root.http.Response,

    implements root.ai.ResponseMeta {
        function finish_reason(
            self,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let wire: OpenAiResponseWire = root.json.from_json<OpenAiResponseWire>(root.json.parse(self.raw.text())) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai meta parse failed"] }
                },
            };
            match (wire.choices.at(0)) {
                let c: OpenAiChoiceWire => c.finish_reason ?? "",
                _ => "",
            }
        }
    }
}

/// OpenAI Chat Completions provider. Provider-specific options are plain fields.
class OpenAi {
    model: string,
    api_key: string,
    base_url: string?,

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        type Body = root.http.Response

        function build_request<T>(
            self,
            prompt: string,
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            // Inject the output schema so the model knows the shape to return.
            // Empty for `T = string` (free-form text), non-empty for a structured `T`.
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            let content: string = if (schema == "") {
                prompt
            } else {
                prompt + "\n\n" + schema
            };
            let body: json = { "model": self.model, "messages": [{ "role": "user", "content": content }] };
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: { "Authorization": "Bearer " + self.api_key, "Content-Type": "application/json" },
                body: root.json.stringify(body),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> root.http.Response throws root.errors.CallError | root.errors.UnknownError {
            let resp = root.http.send(request) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["openai send failed"] },
            };
            if (resp.ok()) {
                resp
            } else {
                let body = resp.text() catch (e) {
                    _ => {
                        ""
                    }
                };
                throw root.errors.UnknownError {
                    data: body,
                    message: ["openai http " + resp.status_code.to_string()],
                }
            }
        }

        // Extracts choices[0].message.content, then coerces it to `T` via SAP
        // (Schema-Aligned Parsing) — robust to markdown fences and minor JSON deviations;
        // for `T = string` the text passes through unchanged.
        function parse<T>(
            self,
            from: root.http.Response,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            let wire: OpenAiResponseWire = root.json.from_json<OpenAiResponseWire>(root.json.parse(from.text())) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["openai parse failed"] },
            };
            match (wire.choices.at(0)) {
                let c: OpenAiChoiceWire => {
                    root.sap.parse<T>(c.message.content) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: e, message: ["openai SAP parse failed"] }
                        },
                    }
                },
                _ => throw root.errors.UnknownError { data: null, message: ["openai: no choices"] },
            }
        }

        function meta_of(
            self,
            from: root.http.Response,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            OpenAiResponseMeta { raw: from }
        }
    }

    implements root.ai.Streaming {
        // Build a streaming Chat Completions request in BAML, open the SSE stream, and
        // wrap it in `baml.llm.Stream`. SSE-delta accumulation + finish-reason validation
        // reuse the existing (leaf) accumulator via a shorthand-built PrimitiveClient; the
        // request itself (endpoint/headers/body, incl. `api_key`/`base_url`) is BAML.
        function stream<TStream, TFinal>(
            self,
            prompt: string,
        ) -> root.llm.Stream<TStream, TFinal> throws root.errors.StreamError | root.errors.UnknownError {
            let schema: string = root.llm.render_output_format(reflect.type_of<TFinal>());
            let content: string = if (schema == "") {
                prompt
            } else {
                prompt + "\n\n" + schema
            };
            let body: json = {
                "model": self.model,
                "messages": [{ "role": "user", "content": content }],
                "stream": true,
            };
            let req = root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: { "Authorization": "Bearer " + self.api_key, "Content-Type": "application/json" },
                body: root.json.stringify(body),
            };
            let sse = root.http.fetch_sse(req) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai fetch_sse failed"] }
                },
            };
            let primitive = root.llm.from_shorthand("openai/" + self.model) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai stream setup failed"] }
                },
            };
            let acc = primitive.new_stream_accumulator() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai accumulator failed"] }
                },
            };
            let cache = root.llm.StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>());
            root.llm.Stream<TStream, TFinal> {
                _client: primitive,
                _acc: acc,
                _sse: sse,
                _cache: cache,
            }
        }
    }
}
