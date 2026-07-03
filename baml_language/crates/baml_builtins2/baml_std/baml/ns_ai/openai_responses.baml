// The OpenAI Responses provider (`/v1/responses`) — a second concrete provider written
// entirely in BAML. Unlike Chat Completions, the Responses API is server-stateful: a call
// can be `store`d and later extended by `previous_response_id`, realizing scenario 20
// (server-stored chains). It implements `Provider` + `HttpProvider` (the ordinary call
// path) and `root.ai.Chain` (the server-stored-chain capability).
//
// The Responses envelope carries `type`-tagged items (`"type":"message"`,
// `"type":"output_text"`). The wire classes use the field name `type` directly — BAML
// allows keyword-named fields, and the builtins codegen escapes them as Rust raw
// identifiers (`r#type`). Decoded via SAP (`root.sap.parse`).

// --- wire shapes (Responses envelope) ---
class OpenAiRespContentWire {
    type: string,
    text: string?,
}

class OpenAiRespItemWire {
    type: string,
    content: OpenAiRespContentWire[]?,
}

class OpenAiRespUsageWire {
    input_tokens: int?,
    output_tokens: int?,
}

class OpenAiRespEnvelopeWire {
    id: string?,
    status: string?,
    output: OpenAiRespItemWire[]?,
    usage: OpenAiRespUsageWire?,
}

/// Decode a raw Responses body into the typed envelope (null if undecodable).
function _openai_responses_envelope(raw_text: string) -> OpenAiRespEnvelopeWire? throws never {
    root.sap.parse<OpenAiRespEnvelopeWire>(raw_text) catch (e) {
        _ => null,
    }
}

/// First `output_text` string in the envelope: the `"type":"message"` output item's
/// `"type":"output_text"` content entry.
function _openai_responses_output_text(envelope: OpenAiRespEnvelopeWire) -> string? throws never {
    let items: OpenAiRespItemWire[] = envelope.output ?? [];
    for (let item in items) {
        if (item.type != "message") {
            continue;
        }
        let contents: OpenAiRespContentWire[] = item.content ?? [];
        for (let c in contents) {
            if (c.type != "output_text") {
                continue;
            }
            match (c.text) {
                null => {
                },
                let t: string => {
                    return t;
                },
            }
        }
    }
    null
}

/// Lazy `ResponseMeta` over the raw Responses body text. Accessors are best-effort
/// (`throws never`): a missing/malformed wire shape yields a default rather than an error.
class OpenAiResponsesMeta {
    raw_text: string,

    implements root.ai.ResponseMeta {
        // The Responses API has a top-level `status` (e.g. "completed"); there is no
        // per-choice finish_reason, so surface the status (or "").
        function finish_reason(self) -> string throws never {
            match (_openai_responses_envelope(self.raw_text)) {
                null => "",
                let envelope: OpenAiRespEnvelopeWire => envelope.status ?? "",
            }
        }

        function usage(self) -> root.ai.Usage throws never {
            let none = root.ai.Usage { input_tokens: 0, output_tokens: 0 };
            match (_openai_responses_envelope(self.raw_text)) {
                null => none,
                let envelope: OpenAiRespEnvelopeWire => match (envelope.usage) {
                    null => none,
                    let u: OpenAiRespUsageWire => {
                        root.ai.Usage {
                            input_tokens: u.input_tokens ?? 0,
                            output_tokens: u.output_tokens ?? 0,
                        }
                    },
                },
            }
        }

        function reasoning(self) -> string | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "openai responses reasoning text not surfaced" }
        }
        function logprobs(self) -> root.ai.Logprob[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "openai responses logprobs not surfaced" }
        }
        function citations(self) -> root.ai.Citation[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "openai responses citations not surfaced" }
        }
    }
}

/// OpenAI Responses provider. Server-stateful: `store` (default true) persists the
/// response so a `ChainHandle` can extend it later via `previous_response_id`.
class OpenAiResponses {
    model: string,
    api_key: string,
    base_url: string?,
    /// Extra HTTP headers merged onto the request (proxies, tracing, org headers).
    extra_headers: map<string, string>?,
    /// Extra top-level body params merged into the request JSON.
    extra_body: map<string, unknown>?,
    /// Append T's output schema to the prompt (default true).
    append_output_schema: bool?,
    /// Whether to server-store responses (default true). Storing is what enables
    /// `previous_response_id` chaining (scenario 20).
    store: bool?,

    // Encode one message part into a Responses `content` item. Only text is supported
    // for now; media parts error (they have a different Responses encoding).
    function _encode_part(self, p: root.ai.MessagePart) -> json throws root.errors.UnknownError {
        match (p.text) {
            null => {
            },
            let t: string => {
                return { "type": "input_text", "text": t };
            },
        }
        throw root.errors.UnknownError {
            data: null,
            message: ["openai-responses: only text message parts are supported"],
        }
    }

    // Encode one chat message into `{ role, content: [...] }`.
    function _encode_message(self, m: root.ai.ChatMessage) -> json throws root.errors.UnknownError {
        let content: json[] = [];
        for (let p in m.parts) {
            content.push(self._encode_part(p));
        }
        { "role": m.role, "content": content }
    }

    // Assemble the Responses request body: model + `input` array + `store`, plus an
    // optional `previous_response_id` and any `extra_body` entries (merged at top level).
    function _request_body(
        self,
        messages: root.ai.ChatMessage[],
        previous_response_id: string?,
        store: bool,
        background: bool,
    ) -> string throws root.errors.UnknownError {
        let body: map<string, json> = {};
        body.set("model", self.model);
        if (background) {
            body.set("background", true);
        }
        let encoded: json[] = [];
        for (let m in messages) {
            encoded.push(self._encode_message(m));
        }
        body.set("input", encoded);
        body.set("store", store);
        match (previous_response_id) {
            null => {
            },
            let id: string => {
                body.set("previous_response_id", id);
            },
        }
        match (self.extra_body) {
            null => {
            },
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
        }
        root.json.stringify(body)
    }

    // Base headers + the caller-owned idempotency key (design D2: a retried submit
    // coalesces server-side where the API honors the header).
    function _headers_with_idempotency(
        self,
        idempotency_key: string,
    ) -> map<string, string> throws never {
        let headers = self._request_headers();
        if (idempotency_key != "") {
            headers.set("Idempotency-Key", idempotency_key);
        }
        headers
    }

    // Base headers + any extra_headers.
    function _request_headers(self) -> map<string, string> throws never {
        let headers: map<string, string> = { "Authorization": "Bearer " + self.api_key, "Content-Type": "application/json" };
        match (self.extra_headers) {
            null => {
            },
            let extra: map<string, string> => {
                for (let k in extra.keys()) {
                    headers.set(k, extra.get(k) ?? "");
                }
            },
        }
        headers
    }

    // Append the output schema onto the last message's trailing text part when possible,
    // otherwise as a fresh text part / user message. Mirrors the OpenAi provider.
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
                null => {
                },
                let m: root.ai.ChatMessage => out.push(m),
            }
            i = i + 1;
        }
        match (messages.at(n - 1)) {
            null => {
            },
            let last: root.ai.ChatMessage => {
                let parts: root.ai.MessagePart[] = [];
                let np = last.parts.length();
                let j = 0;
                while (j < np - 1) {
                    match (last.parts.at(j)) {
                        null => {
                        },
                        let p: root.ai.MessagePart => parts.push(p),
                    }
                    j = j + 1;
                }
                let appended = false;
                match (last.parts.at(np - 1)) {
                    null => {
                    },
                    let lastp: root.ai.MessagePart => match (lastp.text) {
                        null => parts.push(lastp),
                        let t: string => {
                            parts.push(root.ai.MessagePart.from_text(t + "\n\n" + schema));
                            appended = true;
                        },
                    },
                }
                if (!appended) {
                    parts.push(root.ai.MessagePart.from_text(schema));
                }
                out.push(root.ai.ChatMessage { role: last.role, parts: parts });
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

    // POST a prepared body to `/responses`, returning the raw response text (2xx) or
    // normalizing any failure into the `UnknownError` channel (the Chain channel).
    function _post(self, body: string) -> string throws root.errors.UnknownError {
        let req = root.http.Request {
            method: "POST",
            url: (self.base_url ?? "https://api.openai.com/v1") + "/responses",
            headers: self._request_headers(),
            body: body,
        };
        let resp = root.http.send(req) catch (e) {
            _ => {
                throw root.errors.UnknownError { data: e, message: ["openai-responses send failed"] }
            },
        };
        let text = resp.text() catch (e) {
            _ => {
                throw root.errors.UnknownError { data: e, message: ["openai-responses body read failed"] }
            },
        };
        if (resp.ok()) {
            text
        } else {
            throw root.errors.UnknownError {
                data: text,
                message: ["openai-responses http " + resp.status_code.to_string()],
            }
        }
    }

    implements root.ai.Provider {
        // Server-stateful: calls create stored responses and submit creates billed
        // background jobs — a blind re-drive duplicates them (plan D2).
        function is_effectful(self) -> bool throws never {
            true
        }
    }

    implements root.ai.HttpProvider {
        // Body is the raw response TEXT (read once in `send`), shared by `parse`/`meta_of`.
        type Body = string

        function build_request<T>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/responses",
                headers: self._request_headers(),
                body: self
                    ._request_body(
                        self._messages_for(messages, schema),
                        null,
                        self.store ?? true,
                        false,
                    ),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let resp = root.http.send(request) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-responses send failed"] }
                },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-responses body read failed"] }
                },
            };
            if (resp.ok()) {
                text
            } else {
                throw root.errors.UnknownError {
                    data: text,
                    message: ["openai-responses http " + resp.status_code.to_string()],
                }
            }
        }

        function parse<T>(
            self,
            from: string,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            let envelope = match (_openai_responses_envelope(from)) {
                null => {
                    throw root.errors.UnknownError { data: from, message: ["openai-responses parse failed"] }
                },
                let e2: OpenAiRespEnvelopeWire => e2,
            };
            match (_openai_responses_output_text(envelope)) {
                null => {
                    throw root.errors.UnknownError { data: from, message: ["openai-responses: no output_text in response"] }
                },
                let t: string => {
                    root.sap.parse<T>(t) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: t, message: ["openai-responses SAP parse failed"] }
                        },
                    }
                },
            }
        }

        function meta_of(
            self,
            from: string,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            OpenAiResponsesMeta { raw_text: from }
        }
    }

    // Server-stored chains (scenario 20): extend a stored response by id. The Chain
    // channel is `UnknownError` only, so every failure normalizes into it.
    implements root.ai.Chain {
        function extend<T>(
            self,
            prompt: string,
            handle: root.ai.ChainHandle,
        ) -> T throws root.errors.UnknownError {
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            let messages = [root.ai.ChatMessage.user(prompt)];
            let body = self._request_body(self._messages_for(messages, schema), handle.id, true, false);
            let text = self._post(body);
            let envelope = match (_openai_responses_envelope(text)) {
                null => {
                    throw root.errors.UnknownError { data: text, message: ["openai-responses parse failed"] }
                },
                let e2: OpenAiRespEnvelopeWire => e2,
            };
            match (_openai_responses_output_text(envelope)) {
                null => {
                    throw root.errors.UnknownError { data: text, message: ["openai-responses: no output_text in response"] };
                },
                let t: string => {
                    root.sap.parse<T>(t) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: t, message: ["openai-responses SAP parse failed"] }
                        },
                    }
                },
            }
        }
    }

    implements root.ai.Background {
        // Scenario 27 (async background + poll), LIVE: `"background": true` submits a
        // server-side job that runs after the HTTP call returns; `poll` GETs
        // /responses/{id} and yields the typed value once status == "completed".
        // The idempotency key is caller-owned (design D2): sent as a header so a
        // retried submit can coalesce server-side where the API honors it.
        function submit<T>(
            self,
            prompt: string,
            idempotency_key: string,
        ) -> root.ai.Job<T> throws root.errors.UnknownError {
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            let messages = [root.ai.ChatMessage.user(prompt)];
            let body = self._request_body(self._messages_for(messages, schema), null, true, true);
            let req = root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/responses",
                headers: self._headers_with_idempotency(idempotency_key),
                body: body,
            };
            let resp = root.http.send(req) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-responses submit failed"] }
                },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-responses submit body read failed"] }
                },
            };
            if (!resp.ok()) {
                throw root.errors.UnknownError {
                    data: text,
                    message: ["openai-responses submit http " + resp.status_code.to_string()],
                };
            }
            let envelope = match (_openai_responses_envelope(text)) {
                null => {
                    throw root.errors.UnknownError { data: text, message: ["openai-responses submit parse failed"] }
                },
                let e2: OpenAiRespEnvelopeWire => e2,
            };
            root.ai.Job<T> { id: envelope.id ?? "", owner: "openai-responses" }
        }

        function poll<T>(self, job: root.ai.Job<T>) -> T? throws root.errors.UnknownError {
            let req = root.http.Request {
                method: "GET",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/responses/" + job.id,
                headers: self._request_headers(),
                body: "",
            };
            let resp = root.http.send(req) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-responses poll failed"] }
                },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-responses poll body read failed"] }
                },
            };
            if (!resp.ok()) {
                throw root.errors.UnknownError {
                    data: text,
                    message: ["openai-responses poll http " + resp.status_code.to_string()],
                };
            }
            let envelope = match (_openai_responses_envelope(text)) {
                null => {
                    throw root.errors.UnknownError { data: text, message: ["openai-responses poll parse failed"] }
                },
                let e2: OpenAiRespEnvelopeWire => e2,
            };
            if ((envelope.status ?? "") != "completed") {
                return null;
            }
            match (_openai_responses_output_text(envelope)) {
                null => {
                    throw root.errors.UnknownError { data: text, message: ["openai-responses: completed job has no output_text"] }
                },
                let t: string => {
                    root.sap.parse<T>(t) catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: t, message: ["openai-responses job SAP parse failed"] }
                        },
                    }
                },
            }
        }
    }

    // Open a server-stored chain: make a stored call and return a handle carrying the
    // response id. NOT on any interface — an ordinary class method (scenario 20 entry).
    function start_chain(
        self,
        prompt: string,
    ) -> root.ai.ChainHandle throws root.errors.UnknownError {
        let messages = [root.ai.ChatMessage.user(prompt)];
        let body = self._request_body(messages, null, true, false);
        let text = self._post(body);
        let envelope = match (_openai_responses_envelope(text)) {
            null => {
                throw root.errors.UnknownError { data: text, message: ["openai-responses parse failed"] }
            },
            let e2: OpenAiRespEnvelopeWire => e2,
        };
        root.ai.ChainHandle { id: envelope.id ?? "", owner: "openai-responses" }
    }

    // Convenience: extend a chain expecting a plain-string reply.
    function extend_text(
        self,
        prompt: string,
        handle: root.ai.ChainHandle,
    ) -> string throws root.errors.UnknownError {
        let r: string = self.extend<string>(prompt, handle);
        r
    }
}
