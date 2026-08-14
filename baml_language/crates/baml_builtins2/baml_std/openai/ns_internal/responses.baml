// root.OpenAiClient — one model turn against the OpenAI Responses API.
// Phase 1: SAP output mode (schema text in the prompt), native function
// calling. Mirrors _planv2/pages/02_guides/03_clients/05_the_built_in_clients.md.

// Envelope classes for the response body. from_string ignores unknown
// fields, so these type only what the parse reads.
class OaContentPart {
    type: string?,
    text: string?,
}

class OaOutputItem {
    type: string,
    call_id: string?,
    name: string?,
    arguments: string?,
    content: OaContentPart[]?,
    summary: OaContentPart[]?,
}

class OaUsage {
    input_tokens: int,
    output_tokens: int,
}

class OaResponse {
    status: string?,
    output: OaOutputItem[],
    usage: OaUsage?,
}

// A rendered structural prompt becomes Responses input messages. Role-less
// content uses the ordinary user role; `tool` is not a valid
// free-standing Responses message role, so authored tool context is user
// content (actual tool outputs come from the journal path below).
function openai_lower_prompt(prompt: ai.Prompt) -> map<string, unknown>[] {
    let items: map<string, unknown>[] = [];
    for (let message in prompt.messages()) {
        let role = if (message.role == "") {
            "user"
        } else if (message.role == "tool") {
            "user"
        } else {
            message.role
        };
        let item: map<string, unknown> = {};
        let _ = item.set("role", role);
        let _ = item.set("content", message.content);
        items.push(item);
    }
    items
}

// ai.Journal -> Responses API input items. Only promptable events lower;
// ai.RunStarted, ai.ToolRequested, ai.Usage, and ai.FinalProduced are journal-only.
function openai_lower_journal(j: ai.Journal) -> map<string, unknown>[] {
    let items: map<string, unknown>[] = [];
    for (let e in j.entries()) {
        match (e) {
            let u: ai.events.UserMessage => {
                let m: map<string, unknown> = {};
                let _ = m.set("role", "user");
                let _ = m.set("content", u.content);
                items.push(m);
                null
            },
            let a: ai.events.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let t: ai.content.Text => {
                            let m: map<string, unknown> = {};
                            let _ = m.set("role", "assistant");
                            let _ = m.set("content", t.text);
                            items.push(m);
                            null
                        },
                        let use: ai.content.ToolUse => {
                            let m: map<string, unknown> = {};
                            let _ = m.set("type", "function_call");
                            let _ = m.set("call_id", use.id);
                            let _ = m.set("name", use.name);
                            let _ = m.set("arguments", baml.json.stringify(baml.json.to_json(use.args)));
                            items.push(m);
                            null
                        },
                        // ai.Reasoning lowers to nothing in this phase.
                        _ => null,
                    };
                }
                null
            },
            let c: ai.events.ToolCompleted => {
                let m: map<string, unknown> = {};
                let _ = m.set("type", "function_call_output");
                let _ = m.set("call_id", c.id);
                let _ = m.set("output", c.output);
                items.push(m);
                null
            },
            let f: ai.events.ToolFailed => {
                let err: map<string, unknown> = {};
                let _ = err.set("error", f.message);
                let m: map<string, unknown> = {};
                let _ = m.set("type", "function_call_output");
                let _ = m.set("call_id", f.id);
                let _ = m.set("output", baml.json.stringify(baml.json.to_json(err)));
                items.push(m);
                null
            },
            _ => null,
        };
    }
    items
}

// ai.ModelTurnInput -> one Responses API request.
function openai_render(c: root.OpenAiClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _openai_request(c, input, false)
}

// The shared request builder: identical body for both paths, plus the
// `stream: true` flag on the streaming one.
function _openai_request(
    c: root.OpenAiClient,
    input: ai.ModelTurnInput,
    stream: bool,
) -> baml.http.Request {
    let prompt = input.prompt(ai.wire.render_output_format(input.output_type));
    let items = openai_lower_prompt(prompt);
    for (let item in openai_lower_journal(input.journal)) {
        items.push(item);
    }
    let body: map<string, unknown> = {};
    let _ = body.set("model", c.model);
    let _ = body.set("store", false);
    let _ = body.set("input", items);
    if (!input.toolbox.is_empty()) {
        let tools: map<string, unknown>[] = [];
        for (let t in input.toolbox.list()) {
            let entry: map<string, unknown> = {};
            let _ = entry.set("type", "function");
            let _ = entry.set("name", t.name);
            let _ = entry.set("description", t.description);
            let _ = entry.set("parameters", t.input_schema);
            let _ = entry.set("strict", false);
            tools.push(entry);
        }
        let _ = body.set("tools", tools);
        let _ = body.set("tool_choice", "auto");
        null
    } else {
        null
    };
    if (stream) {
        let _ = body.set("stream", true);
        null
    } else {
        null
    };
    baml.http.Request {
        method: "POST",
        url: `${c.resolved_base_url() ?? "https://api.openai.com/v1"}/responses`,
        headers: { "authorization": `Bearer ${c.resolved_api_key()}`, "content-type": "application/json" },
        body: baml.json.stringify(baml.json.to_json(body)),
    }
}

// One decoded response -> canonical content blocks and a stop reason.
function openai_parse(resp: OaResponse) -> ai.ModelTurn {
    let content: ai.content.Block[] = [];
    let has_call = false;
    for (let item in resp.output) {
        if (item.type == "function_call") {
            has_call = true;
            let args: map<string, unknown> = {};
            if let s: string = item.arguments {
                args = baml.json.from_string<map<string, unknown>>(s);
            }
            content.push(
                ai.content.ToolUse { id: item.call_id ?? "", name: item.name ?? "", args: args },
            );
            null
        } else if (item.type == "message") {
            let parts: OaContentPart[] = item.content ?? [];
            for (let part in parts) {
                if ((part.type ?? "") == "output_text") {
                    content.push(ai.content.Text { text: part.text ?? "" });
                    null
                } else {
                    null
                };
            }
            null
        } else if (item.type == "reasoning") {
            let parts: OaContentPart[] = item.summary ?? [];
            for (let part in parts) {
                if let txt: string = part.text {
                    content.push(ai.content.Reasoning { summary: txt });
                    null
                } else {
                    null
                };
            }
            null
        } else {
            null
        };
    }
    let stop = if (has_call) {
        ai.content.StopReason.ToolUse
    } else if ((resp.status ?? "") == "incomplete") {
        ai.content.StopReason.MaxTokens
    } else {
        ai.content.StopReason.Complete
    };
    let usage: ai.events.Usage? = if let u: OaUsage = resp.usage {
        ai.events.Usage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cached_input_tokens: null,
            reasoning_tokens: null,
        }
    } else {
        null
    };
    ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}

// The one internal entry point: render, send, normalize — one model turn.
function invoke(c: root.OpenAiClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = openai_render(c, input);
    let resp = ai.wire.send_as<OaResponse>(req, "openai");
    openai_parse(resp)
}

// ---------------------------------------------------------------------------
// Streaming (Responses API SSE).
//
// Wire events observed: `response.output_text.delta` carries the text delta
// in `delta`; the terminal event (`response.completed` / `response.incomplete`
// / `response.failed`) carries the full response object in `response`, which
// yields the stop reason and usage through the same openai_parse as the
// non-streaming path.

// The envelope of one SSE `data:` payload; only what the decoder reads.
class OaStreamEvent {
    type: string?,
    delta: string?,
    response: OaResponse?,
}

// One SseStream batch -> provider-agnostic stream events.
function _openai_decode_batch(batch: string) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    for (let raw in ai.stream.decode_sse_batch(batch)) {
        if let data: string = raw.data {
            let ev: OaStreamEvent? = baml.json.from_string<OaStreamEvent>(data) catch_all (e) {
                _ => null,
            };
            if let event: OaStreamEvent = ev {
                let t = event.type ?? raw.event ?? "";
                if (t == "response.output_text.delta") {
                    if let d: string = event.delta {
                        out.push(ai.stream.TextDelta { text: d });
                        null
                    } else {
                        null
                    };
                    null
                } else if (t == "response.completed" || t == "response.incomplete" || t == "response.failed") {
                    if let r: OaResponse = event.response {
                        let turn = openai_parse(r);
                        out.push(
                            ai.stream.TurnMeta {
                                stop_reason: turn.stop_reason,
                                input_tokens: turn.usage?.input_tokens,
                                output_tokens: turn.usage?.output_tokens,
                            },
                        );
                        null
                    } else {
                        null
                    };
                    out.push(ai.stream.TurnDone {  });
                    null
                } else {
                    null
                };
                null
            } else {
                null
            };
            null
        } else {
            null
        };
    }
    out
}

// The streaming entry point: the same request as `invoke` with
// `stream: true`, wrapped in an ai.TurnStream over the SSE connection.
function invoke_stream(c: root.OpenAiClient, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
    let req = _openai_request(c, input, true);
    let sse = baml.http.fetch_sse(req) catch_all (e) {
        _ => {
            throw ai.errors.NetworkFailure {
                provider: "openai",
                detail: `transport failed before a response arrived: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    ai.stream.TurnStream.from_sse(sse, _openai_decode_batch)
}
