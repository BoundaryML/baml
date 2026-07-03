// OpenAI Chat Completions with STRICT structured outputs. Same wire surface as
// `baml.ai.OpenAi`, but the request carries `response_format: json_schema` with
// `"strict": true` and a JSON Schema lowered from the call's return type `T`
// (via `baml.schema.json_schema`, OpenAI strict-mode rules). The model is then
// constrained to emit exactly that shape — no prose schema is appended to the
// prompt. For `T = string` no `response_format` is sent (free-form text).
//
// Wire/response-meta shapes and the typed `OpenAiHttpError` are reused from
// `openai.baml` (same `root.ai` namespace) rather than re-declared.

/// OpenAI Chat Completions provider that constrains output with strict JSON Schema.
class OpenAiStrict {
    model: string,
    api_key: string,
    base_url: string?,
    /// Extra HTTP headers merged onto the request (proxies, tracing, org headers).
    extra_headers: map<string, string>?,
    /// Extra top-level body params merged into the request JSON (temperature, …).
    extra_body: map<string, unknown>?,

    // Encode one message part into the Chat Completions content-part shape.
    // (Mirrors OpenAi._encode_part; each field null-eliminated so the media
    // runtime type-test isn't broken by a trailing `_` arm.)
    function _encode_part(self, p: root.ai.MessagePart) -> json throws root.errors.UnknownError {
        match (p.text) {
            null => {
            },
            let t: string => {
                return { "type": "text", "text": t };
            },
        }
        match (p.image) {
            null => {
            },
            let img: image => {
                let url = match (img.url()) {
                    null => "data:" + (img.mime_type() ?? "image/png") + ";base64," + img.base64(),
                    let u: string => u,
                };
                return { "type": "image_url", "image_url": { "url": url } };
            },
        }
        match (p.audio) {
            null => {
            },
            let a: audio => {
                let mime = a.mime_type() ?? "audio/wav";
                let fmt = if (mime.includes("mp3") || mime.includes("mpeg")) {
                    "mp3"
                } else {
                    "wav"
                };
                return { "type": "input_audio", "input_audio": { "data": a.base64(), "format": fmt } };
            },
        }
        match (p.pdf) {
            null => {
            },
            let d: pdf => {
                return {
                    "type": "file",
                    "file": {
                        "filename": "document.pdf",
                        "file_data": "data:application/pdf;base64," + d.base64(),
                    },
                };
            },
        }
        throw root.errors.UnknownError {
            data: null,
            message: ["openai strict: unsupported message part (video has no chat-completions form)"],
        }
    }

    // Encode one chat message. A single text part stays a plain-string `content`;
    // anything else becomes a parts array.
    function _encode_message(self, m: root.ai.ChatMessage) -> json throws root.errors.UnknownError {
        if (m.parts.length() == 1) {
            match (m.parts.at(0)) {
                let only: root.ai.MessagePart => match (only.text) {
                    let t: string => {
                        return { "role": m.role, "content": t };
                    },
                    _ => {
                    },
                },
                _ => {
                },
            }
        }
        let parts: json[] = [];
        for (let p in m.parts) {
            parts.push(self._encode_part(p));
        }
        { "role": m.role, "content": parts }
    }

    // Base headers + any extra_headers.
    function _request_headers(self) -> map<string, string> throws never {
        let headers: map<string, string> = { "Authorization": "Bearer " + self.api_key, "Content-Type": "application/json" };
        match (self.extra_headers) {
            let extra: map<string, string> => {
                for (let k in extra.keys()) {
                    headers.set(k, extra.get(k) ?? "");
                }
            },
            _ => {
            },
        }
        headers
    }

    // Assemble the request body: model + native messages + (for a structured `T`)
    // a strict `response_format.json_schema` + any extra_body entries. When `T`
    // lowers to something without object `properties` (notably `string`), no
    // response_format is sent so the call stays free-form text.
    function _request_body<T>(
        self,
        messages: root.ai.ChatMessage[],
    ) -> string throws root.errors.UnknownError {
        let body: map<string, json> = {};
        body.set("model", self.model);
        let encoded: json[] = [];
        for (let m in messages) {
            encoded.push(self._encode_message(m));
        }
        body.set("messages", encoded);
        // Strict structured output: a JSON Schema lowered from the return type.
        let schema_str: string = root.schema.json_schema(reflect.type_of<T>(), true) catch (e) {
            let u: root.errors.Unsupported => "",
        };
        if (schema_str != "" && schema_str.includes("\"properties\"")) {
            let schema_json: json = root.json.parse(schema_str) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai strict: schema parse failed"] }
                },
            };
            body.set(
                "response_format",
                {
                    "type": "json_schema",
                    "json_schema": { "name": "output", "strict": true, "schema": schema_json },
                },
            );
        }
        match (self.extra_body) {
            let extra: map<string, unknown> => {
                for (let k in extra.keys()) {
                    let v: json = root.json.to_json<unknown>(extra.get(k)) catch (e) {
                        _ => {
                            continue;
                        }
                    };
                    body.set(k, v);
                }
            },
            _ => {
            },
        }
        root.json.stringify(body)
    }

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        // Raw response body TEXT read once in `send`; `parse` and `meta_of` share it.
        type Body = string

        function build_request<T>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: self._request_headers(),
                body: self._request_body<T>(messages),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let resp = root.http.send(request) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai strict send failed"] }
                },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai strict body read failed"] }
                },
            };
            if (resp.ok()) {
                text
            } else {
                throw OpenAiHttpError { status: resp.status_code, body: text }
            }
        }

        // Strict mode returns exact JSON; SAP still handles the `T = string`
        // passthrough and any residual fencing.
        function parse<T>(
            self,
            from: string,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            let wire: OpenAiResponseWire = root.json.from_json<OpenAiResponseWire>(root.json.parse(from)) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai strict parse failed"] }
                },
            };
            match (wire.choices.at(0)) {
                let c: OpenAiChoiceWire => {
                    root.sap.parse<T>(c.message.content) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: e, message: ["openai strict SAP parse failed"] }
                        },
                    }
                },
                _ => {
                    throw root.errors.UnknownError { data: null, message: ["openai strict: no choices"] }
                },
            }
        }

        function meta_of(
            self,
            from: string,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            OpenAiResponseMeta { raw_text: from }
        }
    }
}
