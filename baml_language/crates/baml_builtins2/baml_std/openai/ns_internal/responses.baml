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

// ai.Journal -> Responses API input items. Only promptable events lower;
// ai.RunStarted, ai.ToolRequested, ai.Usage, and ai.FinalProduced are journal-only.
function openai_lower_journal(j: ai.Journal) -> map<string, unknown>[] {
    let items: map<string, unknown>[] = [];
    for (let e in j.entries()) {
        match (e) {
            let u: ai.UserMessage => {
                let m: map<string, unknown> = {};
                let _ = m.set("role", "user");
                let _ = m.set("content", u.content);
                items.push(m);
                null
            },
            let a: ai.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let t: ai.Text => {
                            let m: map<string, unknown> = {};
                            let _ = m.set("role", "assistant");
                            let _ = m.set("content", t.text);
                            items.push(m);
                            null
                        },
                        let use: ai.ToolUse => {
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
            let c: ai.ToolCompleted => {
                let m: map<string, unknown> = {};
                let _ = m.set("type", "function_call_output");
                let _ = m.set("call_id", c.id);
                let _ = m.set("output", c.output);
                items.push(m);
                null
            },
            let f: ai.ToolFailed => {
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
function openai_render(
    c: root.OpenAiClient,
    input: ai.ModelTurnInput,
) -> baml.http.Request {
    _openai_request(c, input, false)
}

// The shared request builder: identical body for both paths, plus the
// `stream: true` flag on the streaming one.
function _openai_request(
    c: root.OpenAiClient,
    input: ai.ModelTurnInput,
    stream: bool,
) -> baml.http.Request {
    let instructions = input.prompt.render_text(ai.render_output_format(input.output_type));
    let items = openai_lower_journal(input.journal);
    let body: map<string, unknown> = {};
    let _ = body.set("model", c.model);
    let _ = body.set("store", false);
    if (items.length() == 0) {
        // first turn: the instructions stand as the sole user input item
        let first: map<string, unknown> = {};
        let _ = first.set("role", "user");
        let _ = first.set("content", instructions);
        let _ = body.set("input", [first]);
        null
    } else {
        let _ = body.set("instructions", instructions);
        let _ = body.set("input", items);
        null
    };
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
        url: `${c.base_url ?? "https://api.openai.com/v1"}/responses`,
        headers: {
            "authorization": `Bearer ${c.api_key ?? baml.env.get_or_panic("OPENAI_API_KEY")}`,
            "content-type": "application/json",
        },
        body: baml.json.stringify(baml.json.to_json(body)),
    }
}

// One decoded response -> canonical content blocks and a stop reason.
function openai_parse(resp: OaResponse) -> ai.ModelTurn {
    let content: ai.ContentBlock[] = [];
    let has_call = false;
    for (let item in resp.output) {
        if (item.type == "function_call") {
            has_call = true;
            let args: map<string, unknown> = {};
            if let s: string = item.arguments {
                args = baml.json.from_string<map<string, unknown>>(s);
            }
            content.push(
                ai.ToolUse { id: item.call_id ?? "", name: item.name ?? "", args: args },
            );
            null
        } else if (item.type == "message") {
            let parts: OaContentPart[] = item.content ?? [];
            for (let part in parts) {
                if ((part.type ?? "") == "output_text") {
                    content.push(ai.Text { text: part.text ?? "" });
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
                    content.push(ai.Reasoning { summary: txt });
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
        ai.StopReason.ToolUse
    } else if ((resp.status ?? "") == "incomplete") {
        ai.StopReason.MaxTokens
    } else {
        ai.StopReason.Complete
    };
    let usage: ai.Usage? = if let u: OaUsage = resp.usage {
        ai.Usage {
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
    let resp = ai.send_as<OaResponse>(req, "openai");
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
function _openai_decode_batch(batch: string) -> ai.StreamEvent[] {
    let out: ai.StreamEvent[] = [];
    for (let raw in ai.decode_sse_batch(batch)) {
        if let data: string = raw.data {
            let ev: OaStreamEvent? = baml.json.from_string<OaStreamEvent>(data) catch_all (e) {
                _ => null,
            };
            if let event: OaStreamEvent = ev {
                let t = event.type ?? raw.event ?? "";
                if (t == "response.output_text.delta") {
                    if let d: string = event.delta {
                        out.push(ai.TextDelta { text: d });
                        null
                    } else {
                        null
                    };
                    null
                } else if (t == "response.completed" || t == "response.incomplete" || t == "response.failed") {
                    if let r: OaResponse = event.response {
                        let turn = openai_parse(r);
                        out.push(
                            ai.TurnMeta {
                                stop_reason: turn.stop_reason,
                                input_tokens: turn.usage?.input_tokens,
                                output_tokens: turn.usage?.output_tokens,
                            },
                        );
                        null
                    } else {
                        null
                    };
                    out.push(ai.TurnDone {});
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
function invoke_stream(c: root.OpenAiClient, input: ai.ModelTurnInput) -> ai.TurnStream {
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
    ai.TurnStream.from_sse(sse, _openai_decode_batch)
}
