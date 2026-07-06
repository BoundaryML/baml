// The Anthropic (Messages API) provider — a second concrete implementation of the
// capability spine, written entirely in BAML (request/response logic in BAML; only
// leaf primitives — `baml.http.send`, `baml.json.*`, `baml.env`, and the streaming
// accumulator — are host functions). It mirrors `openai.baml` structurally so the two
// read side-by-side; the differences are all wire-shape: `x-api-key`/`anthropic-version`
// headers, the required `max_tokens`, top-level `system` hoisting, and the
// `content: [{type:"text",...}]` block array.

/// A non-2xx HTTP response from the Anthropic API — typed so `catch` arms and the
/// `Failure` axis can triage it: 429 is a rate limit; 429/5xx (and 529 "overloaded")
/// are retryable; other 4xx are not.
class AnthropicHttpError {
    status: int,
    body: string,
}

implements root.errors.Failure for AnthropicHttpError {
    function is_retryable(self) -> bool {
        // Anthropic uses 529 ("overloaded") in addition to the usual 5xx; it is already
        // covered by `status >= 500` but spelled out for clarity.
        self.status == 429 || self.status >= 500 || self.status == 529
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

implements root.errors.CallError for AnthropicHttpError {
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

implements root.errors.StreamError for AnthropicHttpError {
    function is_network_error(self) -> bool {
        false
    }
}

// --- wire shapes (Messages API) ---
class AnthropicContentBlockWire {
    type: string?,
    text: string?,
}

class AnthropicUsageWire {
    input_tokens: int?,
    output_tokens: int?,
}

class AnthropicResponseWire {
    content: AnthropicContentBlockWire[],
    stop_reason: string?,
    usage: AnthropicUsageWire?,
}

/// Lazy `ResponseMeta` over the raw response body text. Accessors are best-effort
/// (`throws never`): a missing/malformed wire shape yields a default rather than an
/// error, so projecting the meta never fails the call.
class AnthropicResponseMeta {
    raw_text: string,

    function _wire(self) -> AnthropicResponseWire? throws never {
        root.json.from_json<AnthropicResponseWire>(root.json.parse(self.raw_text)) catch (e) {
            _ => null
        }
    }

    implements root.ai.ResponseMeta {
        function finish_reason(self) -> string throws never {
            match (self._wire()) {
                let w: AnthropicResponseWire => w.stop_reason ?? "",
                _ => "",
            }
        }

        function usage(self) -> root.ai.Usage throws never {
            match (self._wire()) {
                let w: AnthropicResponseWire => match (w.usage) {
                    let u: AnthropicUsageWire => {
                        root.ai.Usage {
                            input_tokens: u.input_tokens ?? 0,
                            output_tokens: u.output_tokens ?? 0,
                        }
                    },
                    _ => root.ai.Usage { input_tokens: 0, output_tokens: 0 },
                },
                _ => root.ai.Usage { input_tokens: 0, output_tokens: 0 },
            }
        }

        // The Messages API (non-extended-thinking) exposes neither reasoning-token text,
        // logprobs, nor citations on this path.
        function reasoning(self) -> string | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "anthropic messages api has no reasoning text on this path" }
        }
        function logprobs(self) -> root.ai.Logprob[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "anthropic messages api has no logprobs" }
        }
        function citations(self) -> root.ai.Citation[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "anthropic messages api has no citations on this path" }
        }
    }
}

/// Anthropic Messages API provider. Provider-specific options are plain fields.
class Anthropic {
    model: string,
    /// API key. When null, sourced from the `ANTHROPIC_API_KEY` environment variable
    /// (the ubiquitous Anthropic convention) — mirroring the shorthand key resolution.
    api_key: string?,
    base_url: string?,
    /// The Messages API REQUIRES `max_tokens`; defaults to 4096 when unset.
    max_tokens: int?,
    /// Value of the `anthropic-version` header (defaults to the stable `2023-06-01`).
    anthropic_version: string?,
    /// Extra HTTP headers merged onto the request (proxies, tracing, beta headers).
    extra_headers: map<string, string>?,
    /// Extra top-level body params merged into the request JSON (temperature,
    /// top_p, stop_sequences, …).
    extra_body: map<string, unknown>?,
    /// Append T's output schema to the prompt (default true). Anthropic has no
    /// response_format, so structured output relies on schema-append + SAP parsing.
    append_output_schema: bool?,

    // Resolve the API key: the explicit option, else the `ANTHROPIC_API_KEY` env var.
    function _api_key(self) -> string throws root.errors.UnknownError {
        match (self.api_key) {
            let k: string => k,
            null => {
                let v = root.env.get("ANTHROPIC_API_KEY") catch (e) {
                    _ => {
                        throw root.errors.UnknownError {
                            data: e,
                            message: ["anthropic: failed to read ANTHROPIC_API_KEY"],
                        }
                    },
                };
                v ?? ""
            },
        }
    }

    // Encode one message part into the Messages API content-block shape.
    // Audio/video have no Messages API representation — the caller errors.
    function _encode_part(self, p: root.ai.MessagePart) -> json throws root.errors.UnknownError {
        // NOTE the match shape: `null => {}` + binding-as-last-arm (house style). A `_`
        // arm after a media binding makes the media runtime type-test fail even for
        // genuine media values (see _plan/baml_gotchas.md), so each field is
        // null-eliminated instead.
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
                // Prefer a URL source when the media carries one; otherwise inline base64.
                // (Object literals live in `return` position to disambiguate from blocks.)
                match (img.url()) {
                    null => {
                        return {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": img.mime_type() ?? "image/png",
                                "data": img.base64(),
                            },
                        };
                    },
                    let u: string => {
                        return { "type": "image", "source": { "type": "url", "url": u } };
                    },
                }
            },
        }
        match (p.pdf) {
            null => {
            },
            let d: pdf => {
                return {
                    "type": "document",
                    "source": {
                        "type": "base64",
                        "media_type": "application/pdf",
                        "data": d.base64(),
                    },
                };
            },
        }
        throw root.errors.UnknownError {
            data: null,
            message: ["anthropic: unsupported message part (audio/video have no Messages API form)"],
        }
    }

    // Encode one non-system chat message into `{role, content: [blocks…]}`. Anthropic
    // always accepts the content-block array form (a plain string is only sugar), so we
    // emit the array uniformly.
    function _encode_message(self, m: root.ai.ChatMessage) -> json throws root.errors.UnknownError {
        let parts: json[] = [];
        for (let p in m.parts) {
            parts.push(self._encode_part(p));
        }
        { "role": m.role, "content": parts }
    }

    // Assemble the request body: model + required max_tokens + hoisted `system` +
    // native (user/assistant) messages + extra_body entries (merged at the top level).
    // System-role messages do NOT go in `messages`; their text collects into the
    // top-level `system` param.
    function _request_body(
        self,
        messages: root.ai.ChatMessage[],
        stream: bool,
    ) -> string throws root.errors.UnknownError {
        let body: map<string, json> = {};
        body.set("model", self.model);
        body.set("max_tokens", self.max_tokens ?? 4096);
        let system_texts: string[] = [];
        let encoded: json[] = [];
        for (let m in messages) {
            if (m.role == "system") {
                for (let p in m.parts) {
                    match (p.text) {
                        null => {
                        },
                        let t: string => {
                            system_texts.push(t);
                        },
                    }
                }
            } else {
                encoded.push(self._encode_message(m));
            }
        }
        if (system_texts.length() > 0) {
            body.set("system", system_texts.join("\n\n"));
        }
        body.set("messages", encoded);
        if (stream) {
            body.set("stream", true);
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

    // Base headers (x-api-key + anthropic-version + content-type) plus any extra_headers.
    function _request_headers(self) -> map<string, string> throws root.errors.UnknownError {
        let headers: map<string, string> = {
            "x-api-key": self._api_key(),
            "anthropic-version": self.anthropic_version ?? "2023-06-01",
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
    // otherwise as a fresh text part / user message. (Copied from the OpenAI provider —
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

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        // Body is the raw response body TEXT (read once in `send`), so `parse` and
        // `meta_of` both read the same buffer — `Response.text()` is not idempotent.
        type Body = string

        function build_request<T>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            // Inject the output schema so the model knows the shape to return.
            // Empty for `T = string` (free-form text), non-empty for a structured `T`.
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.anthropic.com") + "/v1/messages",
                headers: self._request_headers(),
                body: self._request_body(self._messages_for(messages, schema), false),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let resp = root.http.send(request) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["anthropic send failed"] },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["anthropic body read failed"] }
                },
            };
            if (resp.ok()) {
                text
            } else {
                throw AnthropicHttpError { status: resp.status_code, body: text }
            }
        }

        // Concatenate the response's text content blocks, then coerce to `T` via SAP
        // (Schema-Aligned Parsing) — robust to markdown fences and minor JSON deviations;
        // for `T = string` the text passes through unchanged.
        function parse<T>(
            self,
            from: string,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            let wire: AnthropicResponseWire = root.json.from_json<AnthropicResponseWire>(root.json.parse(from)) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["anthropic parse failed"] },
            };
            let text = "";
            for (let b in wire.content) {
                match (b.text) {
                    null => {
                    },
                    let t: string => {
                        text = text + t;
                    },
                }
            }
            root.sap.parse<T>(text) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["anthropic SAP parse failed"] }
                },
            }
        }

        function meta_of(
            self,
            from: string,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            AnthropicResponseMeta { raw_text: from }
        }
    }

    implements root.ai.Streaming {
        // Build a streaming Messages request in BAML, open the SSE stream, and wrap it in
        // `baml.llm.Stream`. Anthropic's SSE (`content_block_delta`/`text_delta`, terminal
        // `message_stop`) is understood by the accumulator keyed on the `anthropic/…`
        // shorthand; the request itself (endpoint/headers/body) is BAML.
        function stream_messages<TStream, TFinal>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.llm.Stream<TStream, TFinal> throws root.errors.StreamError | root.errors.UnknownError {
            let schema: string = root.llm.render_output_format(reflect.type_of<TFinal>());
            let req = root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.anthropic.com") + "/v1/messages",
                headers: self._request_headers(),
                body: self._request_body(self._messages_for(messages, schema), true),
            };
            let sse = root.http.fetch_sse(req) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["anthropic fetch_sse failed"] }
                },
            };
            let primitive = root.llm.from_shorthand("anthropic/" + self.model) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["anthropic stream setup failed"] }
                },
            };
            let acc = primitive.new_stream_accumulator() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["anthropic accumulator failed"] }
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
