// The Google Gemini (generativelanguage.googleapis.com `generateContent`) provider — a
// third concrete implementation of the capability spine, written entirely in BAML
// (request/response logic in BAML; only leaf primitives — `baml.http.send`,
// `baml.json.*`, `baml.sap.parse`, `baml.env` — are host functions). It mirrors
// `anthropic.baml`/`openai.baml` structurally; the differences are all wire-shape:
// the `x-goog-api-key` header, the `:generateContent` URL suffix, the `contents`
// array with `parts`, the assistant→`"model"` role remap, top-level `systemInstruction`
// hoisting, and the camelCase response envelope (decoded via SAP `@alias`).
//
// Streaming is intentionally NOT implemented: the host SSE accumulator
// (`sys_llm::stream_accumulator`) rejects the `google-ai` provider as NotImplemented
// (its `data:`-chunked `generateContent` shape is not yet parsed), so there is no
// reasonable path to wrap Gemini's stream in `baml.llm.Stream` today.

/// A non-2xx HTTP response from the Gemini API — typed so `catch` arms and the
/// `Failure` axis can triage it: 429 is a rate limit; 429/5xx are retryable; other
/// 4xx are not.
class GeminiHttpError {
    status: int,
    body: string,
}

implements root.errors.Failure for GeminiHttpError {
    function is_retryable(self) -> bool {
        self.status == 429 || self.status >= 500
    }
    function is_effectful(self) -> bool {
        false
    }
    function is_policy_refusal(self) -> bool {
        false
    }
    function is_resumable(self) -> bool {
        false
    }
    function is_unsupported(self) -> bool {
        false
    }
}

implements root.errors.CallError for GeminiHttpError {
    function is_network_error(self) -> bool {
        false
    }
    function is_rate_limit(self) -> bool {
        self.status == 429
    }
    function is_parse_error(self) -> bool {
        false
    }
}

implements root.errors.StreamError for GeminiHttpError {
    function is_network_error(self) -> bool {
        false
    }
}

// --- wire shapes (generateContent) ---
// The response envelope uses camelCase keys; fields are named snake_case with `@alias`
// and decoded via `baml.sap.parse` (only SAP honors `@alias`, NOT `from_json`).
class GeminiPartWire {
    text string?
}

class GeminiContentWire {
    parts GeminiPartWire[]?
    role string?
}

class GeminiCandidateWire {
    content GeminiContentWire?
    finish_reason string? @alias("finishReason")
}

class GeminiUsageWire {
    prompt_token_count int? @alias("promptTokenCount")
    candidates_token_count int? @alias("candidatesTokenCount")
}

class GeminiResponseWire {
    candidates GeminiCandidateWire[]
    usage_metadata GeminiUsageWire? @alias("usageMetadata")
}

/// Lazy `ResponseMeta` over the raw response body text. Accessors are best-effort
/// (`throws never`): a missing/malformed wire shape yields a default rather than an
/// error, so projecting the meta never fails the call.
class GeminiResponseMeta {
    raw_text: string,

    function _wire(self) -> GeminiResponseWire? throws never {
        root.sap.parse<GeminiResponseWire>(self.raw_text) catch (e) {
            _ => null
        }
    }

    implements root.ai.ResponseMeta {
        function finish_reason(self) -> string throws never {
            match (self._wire()) {
                let w: GeminiResponseWire => match (w.candidates.at(0)) {
                    let c: GeminiCandidateWire => c.finish_reason ?? "",
                    _ => "",
                },
                _ => "",
            }
        }

        function usage(self) -> root.ai.Usage throws never {
            match (self._wire()) {
                let w: GeminiResponseWire => match (w.usage_metadata) {
                    let u: GeminiUsageWire => {
                        root.ai.Usage {
                            input_tokens: u.prompt_token_count ?? 0,
                            output_tokens: u.candidates_token_count ?? 0,
                        }
                    },
                    _ => root.ai.Usage { input_tokens: 0, output_tokens: 0 },
                },
                _ => root.ai.Usage { input_tokens: 0, output_tokens: 0 },
            }
        }

        // The generateContent path exposes neither reasoning-token text, logprobs,
        // nor citations in this normalization.
        function reasoning(self) -> string | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "gemini generateContent has no reasoning text on this path" }
        }
        function logprobs(self) -> root.ai.Logprob[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "gemini generateContent has no logprobs on this path" }
        }
        function citations(self) -> root.ai.Citation[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "gemini generateContent has no citations on this path" }
        }
    }
}

/// Google Gemini (`generateContent`) provider. Provider-specific options are plain fields.
class Gemini {
    model: string,
    /// API key. When null, sourced from the `GOOGLE_API_KEY` environment variable
    /// (the ubiquitous Google AI convention) — mirroring the shorthand key resolution.
    api_key: string?,
    /// API base URL (default `https://generativelanguage.googleapis.com/v1beta`).
    base_url: string?,
    /// Extra HTTP headers merged onto the request (proxies, tracing).
    extra_headers: map<string, string>?,
    /// Extra top-level body params merged into the request JSON (e.g. `generationConfig`,
    /// `safetySettings`) — mirrors the legacy `options.request_body` behavior.
    extra_body: map<string, unknown>?,
    /// Append T's output schema to the prompt (default true). Gemini structured output
    /// here relies on schema-append + SAP parsing.
    append_output_schema: bool?,

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        // Body is the raw response body TEXT (read once in `send`), so `parse` and
        // `meta_of` both read the same buffer — `Response.text()` is not idempotent.
        type Body = string

        function build_request<T>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.http.Request throws root.errors.UnknownError {
            // Inject the output schema so the model knows the shape to return.
            // Empty for `T = string` (free-form text), non-empty for a structured `T`.
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://generativelanguage.googleapis.com/v1beta")
                    + "/models/"
                    + self.model
                    + ":generateContent",
                headers: self._request_headers(),
                body: self._request_body(self._messages_for(messages, schema)),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let resp = root.http.send(request) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["gemini send failed"] },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["gemini body read failed"] }
                },
            };
            if (resp.ok()) {
                text
            } else {
                throw GeminiHttpError { status: resp.status_code, body: text }
            }
        }

        // Concatenate the first candidate's text parts, then coerce to `T` via SAP
        // (Schema-Aligned Parsing) — robust to markdown fences and minor JSON deviations;
        // for `T = string` the text passes through unchanged. The camelCase envelope is
        // decoded via SAP too (only SAP honors the `@alias` fields).
        function parse<T>(
            self,
            from: string,
        ) -> T throws root.errors.UnknownError {
            let wire: GeminiResponseWire = root.sap.parse<GeminiResponseWire>(from) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["gemini parse failed"] },
            };
            match (wire.candidates.at(0)) {
                let c: GeminiCandidateWire => {
                    let text = "";
                    match (c.content) {
                        let content: GeminiContentWire => match (content.parts) {
                            let ps: GeminiPartWire[] => {
                                for (let part in ps) {
                                    match (part.text) {
                                        null => {
                                        },
                                        let t: string => {
                                            text = text + t;
                                        },
                                    }
                                }
                            },
                            _ => {
                            },
                        },
                        _ => {
                        },
                    }
                    root.sap.parse<T>(text) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: e, message: ["gemini SAP parse failed"] }
                        },
                    }
                },
                _ => throw root.errors.UnknownError { data: null, message: ["gemini: no candidates"] },
            }
        }

        function meta_of(
            self,
            from: string,
        ) -> root.ai.ResponseMeta throws never {
            GeminiResponseMeta { raw_text: from }
        }
    }

    // ── private helpers ──

    // Resolve the API key: the explicit option, else the `GOOGLE_API_KEY` env var.
    function _api_key(self) -> string throws root.errors.UnknownError {
        match (self.api_key) {
            let k: string => k,
            null => {
                let v = root.env.get("GOOGLE_API_KEY") catch (e) {
                    _ => {
                        throw root.errors.UnknownError {
                            data: e,
                            message: ["gemini: failed to read GOOGLE_API_KEY"],
                        }
                    },
                };
                v ?? ""
            },
        }
    }

    // Encode one message part into a Gemini `Part`. Media crosses as `inline_data`
    // (base64 + mime). Video has no inline form here — the caller errors.
    function _encode_part(self, p: root.ai.MessagePart) -> json throws root.errors.UnknownError {
        // NOTE the match shape: `null => {}` + binding-as-last-arm (house style). A `_`
        // arm after a media binding makes the media runtime type-test fail even for
        // genuine media values (see _plan/baml_gotchas.md), so each field is
        // null-eliminated instead. Object literals live in `return` position to
        // disambiguate from blocks.
        match (p.text) {
            null => {
            },
            let t: string => {
                return { "text": t };
            },
        }
        match (p.image) {
            null => {
            },
            let img: image => {
                return {
                    "inline_data": {
                        "mime_type": img.mime_type() ?? "image/png",
                        "data": img.base64(),
                    },
                };
            },
        }
        match (p.audio) {
            null => {
            },
            let a: audio => {
                return {
                    "inline_data": {
                        "mime_type": a.mime_type() ?? "audio/wav",
                        "data": a.base64(),
                    },
                };
            },
        }
        match (p.pdf) {
            null => {
            },
            let d: pdf => {
                return {
                    "inline_data": {
                        "mime_type": "application/pdf",
                        "data": d.base64(),
                    },
                };
            },
        }
        throw root.errors.UnknownError {
            data: null,
            message: ["gemini: unsupported message part (video has no inline generateContent form)"],
        }
    }

    // Encode one non-system chat message into `{role, parts: [...]}`. The assistant role
    // is remapped to Gemini's `"model"`; `"user"` passes through.
    function _encode_message(self, m: root.ai.ChatMessage) -> json throws root.errors.UnknownError {
        let parts: json[] = [];
        for (let p in m.parts) {
            parts.push(self._encode_part(p));
        }
        let role = if (m.role == "assistant") {
            "model"
        } else {
            m.role
        };
        { "role": role, "parts": parts }
    }

    // Assemble the request body: `contents` (native user/model messages) + hoisted
    // `systemInstruction` + extra_body entries merged at the top level. System-role
    // messages do NOT go in `contents`; their text collects into `systemInstruction`.
    function _request_body(
        self,
        messages: root.ai.ChatMessage[],
    ) -> string throws root.errors.UnknownError {
        let body: map<string, json> = {};
        let system_parts: json[] = [];
        let contents: json[] = [];
        for (let m in messages) {
            if (m.role == "system") {
                for (let p in m.parts) {
                    match (p.text) {
                        null => {
                        },
                        let t: string => {
                            let part: json = { "text": t };
                            system_parts.push(part);
                        },
                    }
                }
            } else {
                contents.push(self._encode_message(m));
            }
        }
        body.set("contents", contents);
        if (system_parts.length() > 0) {
            let si: json = { "parts": system_parts };
            body.set("systemInstruction", si);
        }
        match (self.extra_body) {
            let extra: map<string, unknown> => {
                for (let k in extra.keys()) {
                    // unknown -> json by runtime-type dispatch (to_string<T> is type-driven
                    // and cannot serialize an `unknown`-typed value; to_json<T> can).
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

    // Base headers (x-goog-api-key + content-type) plus any extra_headers.
    function _request_headers(self) -> map<string, string> throws root.errors.UnknownError {
        let headers: map<string, string> = {
            "x-goog-api-key": self._api_key(),
            "content-type": "application/json",
        };
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

    // Append the output schema to the conversation: merged into the last message's
    // trailing text part when possible (keeping the classic "prompt\n\nschema" shape),
    // otherwise as a fresh text part / user message. (Copied from the sibling providers —
    // the schema-append idiom is provider-agnostic.)
    function _append_schema(
        self,
        messages: root.ai.ChatMessage[],
        schema: string,
    ) -> root.ai.ChatMessage[] throws never {
        let n = messages.length();
        if (n == 0) {
            return [root.ai.ChatMessage.user(schema)];
        }
        let out: root.ai.ChatMessage[] = [];
        let i = 0;
        while (i < n - 1) {
            match (messages.at(i)) {
                let m: root.ai.ChatMessage => out.push(m),
                _ => {
                },
            }
            i = i + 1;
        }
        match (messages.at(n - 1)) {
            let last: root.ai.ChatMessage => {
                let parts: root.ai.MessagePart[] = [];
                let np = last.parts.length();
                let j = 0;
                while (j < np - 1) {
                    match (last.parts.at(j)) {
                        let p: root.ai.MessagePart => parts.push(p),
                        _ => {
                        },
                    }
                    j = j + 1;
                }
                let appended = false;
                match (last.parts.at(np - 1)) {
                    let lastp: root.ai.MessagePart => match (lastp.text) {
                        let t: string => {
                            parts.push(root.ai.MessagePart.from_text(t + "\n\n" + schema));
                            appended = true;
                        },
                        _ => parts.push(lastp),
                    },
                    _ => {
                    },
                }
                if (!appended) {
                    parts.push(root.ai.MessagePart.from_text(schema));
                }
                out.push(root.ai.ChatMessage { role: last.role, parts: parts });
            },
            _ => {
            },
        }
        out
    }

    // Messages + optional schema (skipped when append_output_schema is false).
    function _messages_for(
        self,
        messages: root.ai.ChatMessage[],
        schema: string,
    ) -> root.ai.ChatMessage[] throws never {
        if (!(self.append_output_schema ?? true) || schema == "") {
            messages
        } else {
            self._append_schema(messages, schema)
        }
    }
}
