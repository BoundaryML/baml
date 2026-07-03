// The OpenAI provider — the first concrete implementation of the capability spine,
// written entirely in BAML (request/response logic in BAML; only leaf primitives —
// `baml.http.send`, `baml.json.*`, `baml.env` — are host functions). This is the
// reference every other provider (and a user's custom provider) follows.

// --- wire shapes (Chat Completions) ---
class OpenAiMessageWire {
    content: string,
}

class OpenAiLogprobTokenWire {
    token: string,
    logprob: float,
}

class OpenAiLogprobsWire {
    content: OpenAiLogprobTokenWire[]?,
}

class OpenAiChoiceWire {
    message: OpenAiMessageWire,
    finish_reason: string?,
    logprobs: OpenAiLogprobsWire?,
}

class OpenAiUsageWire {
    prompt_tokens: int,
    completion_tokens: int,
}

class OpenAiResponseWire {
    choices: OpenAiChoiceWire[],
    usage: OpenAiUsageWire?,
}

/// Lazy `ResponseMeta` over the raw response body text. Accessors are best-effort
/// (`throws never`): a missing/malformed wire shape yields a default rather than an
/// error, so projecting the meta never fails the call.
class OpenAiResponseMeta {
    raw_text: string,

    function _wire(self) -> OpenAiResponseWire? throws never {
        root.json.from_json<OpenAiResponseWire>(root.json.parse(self.raw_text)) catch (e) {
            _ => null
        }
    }

    implements root.ai.ResponseMeta {
        function finish_reason(self) -> string throws never {
            match (self._wire()) {
                let w: OpenAiResponseWire => match (w.choices.at(0)) {
                    let c: OpenAiChoiceWire => c.finish_reason ?? "",
                    _ => "",
                },
                _ => "",
            }
        }

        function usage(self) -> root.ai.Usage throws never {
            match (self._wire()) {
                let w: OpenAiResponseWire => match (w.usage) {
                    let u: OpenAiUsageWire => {
                        root.ai.Usage {
                            input_tokens: u.prompt_tokens,
                            output_tokens: u.completion_tokens,
                        }
                    },
                    _ => root.ai.Usage { input_tokens: 0, output_tokens: 0 },
                },
                _ => root.ai.Usage { input_tokens: 0, output_tokens: 0 },
            }
        }

        // Chat Completions does not expose reasoning-token text or citations.
        function reasoning(self) -> string | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "openai chat completions has no reasoning text" }
        }
        function citations(self) -> root.ai.Citation[] | root.ai.Unavailable throws never {
            root.ai.Unavailable { reason: "openai chat completions has no citations" }
        }

        // Logprobs are present only if the request set `logprobs: true`.
        function logprobs(self) -> root.ai.Logprob[] | root.ai.Unavailable throws never {
            let w = match (self._wire()) {
                let x: OpenAiResponseWire => x,
                _ => {
                    return root.ai.Unavailable { reason: "unparseable response" };
                },
            };
            let c = match (w.choices.at(0)) {
                let ch: OpenAiChoiceWire => ch,
                _ => {
                    return root.ai.Unavailable { reason: "no choices" };
                },
            };
            let lp = match (c.logprobs) {
                let l: OpenAiLogprobsWire => l,
                _ => {
                    return root.ai.Unavailable { reason: "logprobs not requested" };
                },
            };
            match (lp.content) {
                let toks: OpenAiLogprobTokenWire[] => {
                    let out: root.ai.Logprob[] = [];
                    for (let t in toks) {
                        out.push(root.ai.Logprob { token: t.token, logprob: t.logprob });
                    }
                    out
                },
                _ => root.ai.Unavailable { reason: "logprobs not requested" },
            }
        }
    }
}

/// OpenAI Chat Completions provider. Provider-specific options are plain fields.
class OpenAi {
    model: string,
    api_key: string,
    base_url: string?,
    /// Extra HTTP headers merged onto the request (proxies, tracing, org headers).
    extra_headers: map<string, string>?,
    /// Extra top-level body params merged into the request JSON (temperature,
    /// max_tokens, …) — mirrors the legacy `options.request_body` behavior.
    extra_body: map<string, unknown>?,
    /// Append T's output schema to the prompt (default true). The orchestrator
    /// delegation sets false: a rendered template already carries (or deliberately
    /// omits) `${ctx.output_format}`, and appending again would double-inject.
    append_output_schema: bool?,

    // Assemble the request body: model + messages + extra_body entries (legacy
    // request_body semantics: user entries are merged at the top level).
    function _request_body(self, content: string, stream: bool) -> string throws never {
        let body: map<string, json> = {};
        body.set("model", self.model);
        let messages: json = [{ "role": "user", "content": content }];
        body.set("messages", messages);
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

    // Prompt + optional schema suffix (skipped when append_output_schema is false).
    function _content_for(self, prompt: string, schema: string) -> string throws never {
        if (!(self.append_output_schema ?? true) || schema == "") {
            prompt
        } else {
            prompt + "\n\n" + schema
        }
    }

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        // Body is the raw response body TEXT (read once in `send`), so `parse` and
        // `meta_of` both read the same buffer — `Response.text()` is not idempotent.
        type Body = string

        function build_request<T>(
            self,
            prompt: string,
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            // Inject the output schema so the model knows the shape to return.
            // Empty for `T = string` (free-form text), non-empty for a structured `T`.
            let schema: string = root.llm.render_output_format(reflect.type_of<T>());
            let content: string = self._content_for(prompt, schema);
            root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: self._request_headers(),
                body: self._request_body(content, false),
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            let resp = root.http.send(request) catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["openai send failed"] },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai body read failed"] }
                },
            };
            if (resp.ok()) {
                text
            } else {
                throw root.errors.UnknownError {
                    data: text,
                    message: ["openai http " + resp.status_code.to_string()],
                }
            }
        }

        // Extracts choices[0].message.content, then coerces it to `T` via SAP
        // (Schema-Aligned Parsing) — robust to markdown fences and minor JSON deviations;
        // for `T = string` the text passes through unchanged.
        function parse<T>(
            self,
            from: string,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            let wire: OpenAiResponseWire = root.json.from_json<OpenAiResponseWire>(root.json.parse(from)) catch (e) {
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
            from: string,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            OpenAiResponseMeta { raw_text: from }
        }
    }

    implements root.ai.Capabilities {
        function structured_output(self) -> root.ai.Support throws never {
            root.ai.Support.Yes
        }
        function image_input(self) -> root.ai.Support throws never {
            root.ai.Support.Yes
        }
        function parallel_tools(self) -> root.ai.Support throws never {
            root.ai.Support.Yes
        }
        function max_input_tokens(self) -> int throws never {
            128000
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
            let content: string = self._content_for(prompt, schema);
            let req = root.http.Request {
                method: "POST",
                url: (self.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: self._request_headers(),
                body: self._request_body(content, true),
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

    implements root.ai.Tools {
        type Transcript = OpenAiToolTranscript

        function begin<T>(
            self,
            prompt: string,
            tools: root.ai.Tool[],
        ) -> OpenAiToolTranscript throws root.errors.ToolError | root.errors.UnknownError {
            let tools_arr: json[] = [];
            for (let t in tools) {
                let schema: json = root.json.parse(t.parameters) catch (e) {
                    _ => {
                        throw root.errors.UnknownError { data: e, message: ["tool schema not valid JSON: " + t.name] }
                    },
                };
                tools_arr
                    .push(
                        {
                            "type": "function",
                            "function": { "name": t.name, "description": t.description, "parameters": schema },
                        },
                    );
            }
            OpenAiToolTranscript {
                model: self.model,
                api_key: self.api_key,
                base_url: self.base_url,
                tools_json: root.json.stringify(tools_arr),
                messages: [{ "role": "user", "content": prompt }],
            }
        }

        function step<T>(
            self,
            t: OpenAiToolTranscript,
        ) -> T | root.ai.ToolCalls throws root.errors.ToolError | root.errors.UnknownError {
            let tools_val: json = root.json.parse(t.tools_json) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["tools_json invalid"] };
                },
            };
            let body: json = { "model": t.model, "messages": t.messages, "tools": tools_val, "tool_choice": "auto" };
            let req = root.http.Request {
                method: "POST",
                url: (t.base_url ?? "https://api.openai.com/v1") + "/chat/completions",
                headers: { "Authorization": "Bearer " + t.api_key, "Content-Type": "application/json" },
                body: root.json.stringify(body),
            };
            let resp = root.http.send(req) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["tools send failed"] };
                },
            };
            let text = resp.text() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["tools body read failed"] };
                },
            };
            let parsed: OpenAiToolRespWire = root.json.from_json<OpenAiToolRespWire>(root.json.parse(text)) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: text, message: ["tools parse failed"] };
                },
            };
            let msg = match (parsed.choices.at(0)) {
                let c: OpenAiToolChoiceWire => c.message,
                _ => {
                    throw root.errors.UnknownError { data: text, message: ["tools: no choices"] };
                },
            };
            match (msg.tool_calls) {
                let tcs: json[] => {
                    // Preserve the assistant tool-call turn in the transcript.
                    t.messages.push({ "role": "assistant", "tool_calls": tcs });
                    let calls: root.ai.ToolCall[] = [];
                    for (let tc in tcs) {
                        let fn = root.json.field(tc, "function");
                        calls
                            .push(
                                root.ai.ToolCall {
                                    id: _openai_json_str(root.json.field(tc, "id")),
                                    name: _openai_json_str(root.json.field(fn, "name")),
                                    args: _openai_json_str(root.json.field(fn, "arguments")),
                                },
                            );
                    }
                    root.ai.ToolCalls { calls: calls }
                },
                _ => {
                    // No tool calls: the content is the final value, coerced via SAP.
                    root.sap.parse<T>(msg.content ?? "") catch (e) {
                        _ => {
                            throw root.errors.UnknownError { data: msg.content, message: ["tools final parse failed"] };
                        },
                    }
                },
            }
        }

        function submit(
            self,
            t: OpenAiToolTranscript,
            results: root.ai.ToolResult[],
        ) -> OpenAiToolTranscript throws root.errors.ToolError | root.errors.UnknownError {
            for (let r in results) {
                t.messages.push({ "role": "tool", "tool_call_id": r.id, "content": r.output });
            }
            t
        }
    }
}

// --- OpenAI tool-calling wire + transcript ---
class OpenAiToolMsgWire {
    content: string?,
    tool_calls: json[]?,
}

class OpenAiToolChoiceWire {
    message: OpenAiToolMsgWire,
}

class OpenAiToolRespWire {
    choices: OpenAiToolChoiceWire[],
}

/// Provider-owned transcript: the running OpenAI messages array + the encoded tools.
class OpenAiToolTranscript {
    model: string,
    api_key: string,
    base_url: string?,
    tools_json: string,
    messages: json[],
}

function _openai_json_str(j: json) -> string throws never {
    match (j) {
        let s: string => s,
        _ => "",
    }
}
