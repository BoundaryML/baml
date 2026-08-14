// The live Anthropic client: one Messages API call per turn, SAP output
// mode (the schema rides in the prompt text; tools are native).
// Mirrors _planv2/pages/02_guides/03_clients/05_the_built_in_clients.md.

// ---------------------------------------------------------------------------
// Response envelope. from_string ignores unknown fields; only what the
// client reads is typed, and per-block fields are optional because block
// kinds carry different fields.

class AnthropicContentBlock {
    type: string,
    text: string?,
    thinking: string?,
    id: string?,
    name: string?,
    input: map<string, unknown>?,
}

class AnthropicUsage {
    input_tokens: int,
    output_tokens: int,
}

class AnthropicResponse {
    content: AnthropicContentBlock[],
    stop_reason: string?,
    usage: AnthropicUsage?,
}

// ---------------------------------------------------------------------------
// ai.Journal lowering. Anthropic requires strict user/assistant alternation and
// tool_result blocks in a user message immediately after the assistant
// tool_use message, so consecutive tool results group into ONE user message.

// An intermediate lowered message: role plus wire content blocks.
// tool_results marks a user message that is an open tool_result group.
class AnthropicMessage {
    role: string,
    tool_results: bool,
    blocks: map<string, unknown>[],
}

// Anthropic carries system content outside the alternating messages list.
// Keep that split provider-local while preserving the authored prompt roles.
class AnthropicPrompt {
    system: map<string, unknown>[],
    messages: AnthropicMessage[],
}

function _anthropic_lower_prompt(prompt: ai.Prompt) -> AnthropicPrompt {
    let system: map<string, unknown>[] = [];
    let messages: AnthropicMessage[] = [];
    for (let message in prompt.messages()) {
        let block: map<string, unknown> = {};
        let _ = block.set("type", "text");
        let _ = block.set("text", message.content);
        if (message.role == "system") {
            system.push(block);
            null
        } else {
            let role = if (message.role == "assistant") {
                "assistant"
            } else {
                "user"
            };
            messages.push(AnthropicMessage { role: role, tool_results: false, blocks: [block] });
            null
        };
    }
    AnthropicPrompt { system: system, messages: messages }
}

function _anthropic_assistant_blocks(content: ai.content.Block[]) -> map<string, unknown>[] {
    let out: map<string, unknown>[] = [];
    for (let b in content) {
        match (b) {
            let t: ai.content.Text => {
                let bb: map<string, unknown> = {};
                let _ = bb.set("type", "text");
                let _ = bb.set("text", t.text);
                out.push(bb);
                null
            },
            // Dropped: replaying thinking without its signature is rejected
            // by the API; readable-projection replay is a phase 2 capability.
            let r: ai.content.Reasoning => null,
            let u: ai.content.ToolUse => {
                let bb: map<string, unknown> = {};
                let _ = bb.set("type", "tool_use");
                let _ = bb.set("id", u.id);
                let _ = bb.set("name", u.name);
                let _ = bb.set("input", u.args);
                out.push(bb);
                null
            },
        };
    }
    out
}

// Append one tool_result block, extending the trailing group when open.
function _anthropic_push_tool_result(
    msgs: AnthropicMessage[],
    id: string,
    content: string,
    is_error: bool,
) -> null {
    let block: map<string, unknown> = {};
    let _ = block.set("type", "tool_result");
    let _ = block.set("tool_use_id", id);
    let _ = block.set("content", content);
    let _ = block.set("is_error", is_error);
    // .at returns null on an empty list, covering the no-messages case.
    if let m: AnthropicMessage = msgs.at(msgs.length() - 1) {
        if (m.tool_results) {
            m.blocks.push(block);
            null
        } else {
            msgs.push(AnthropicMessage { role: "user", tool_results: true, blocks: [block] });
            null
        }
    } else {
        msgs.push(AnthropicMessage { role: "user", tool_results: true, blocks: [block] });
        null
    }
}

// ai.RunStarted/ai.ToolRequested/ai.Usage/ai.FinalProduced are not promptable and skip;
// skipped events never close an open tool_result group.
function _anthropic_lower_journal(j: ai.Journal) -> AnthropicMessage[] {
    let msgs: AnthropicMessage[] = [];
    for (let e in j.entries()) {
        match (e) {
            let m: ai.events.UserMessage => {
                let block: map<string, unknown> = {};
                let _ = block.set("type", "text");
                let _ = block.set("text", m.content);
                msgs.push(AnthropicMessage { role: "user", tool_results: false, blocks: [block] });
                null
            },
            let m: ai.events.AssistantMessage => {
                msgs.push(
                    AnthropicMessage {
                        role: "assistant",
                        tool_results: false,
                        blocks: _anthropic_assistant_blocks(m.content),
                    },
                );
                null
            },
            let c: ai.events.ToolCompleted => {
                _anthropic_push_tool_result(msgs, c.id, c.output, false)
            },
            let f: ai.events.ToolFailed => _anthropic_push_tool_result(msgs, f.id, f.message, true),
            _ => null,
        };
    }
    msgs
}

function _anthropic_render_messages(msgs: AnthropicMessage[]) -> map<string, unknown>[] {
    let out: map<string, unknown>[] = [];
    for (let m in msgs) {
        let mm: map<string, unknown> = {};
        let _ = mm.set("role", m.role);
        let _ = mm.set("content", m.blocks);
        out.push(mm);
    }
    out
}

// ---------------------------------------------------------------------------
// The client.

// The shared request builder: identical body for both paths, plus the
// `stream: true` flag on the streaming one.
function _anthropic_request(
    c: root.AnthropicClient,
    input: ai.ModelTurnInput,
    stream: bool,
) -> baml.http.Request {
    let prompt = input.prompt(ai.wire.render_output_format(input.output_type));
    let prompt_parts = _anthropic_lower_prompt(prompt);
    let system = prompt_parts.system;
    let lowered = prompt_parts.messages;
    for (let message in _anthropic_lower_journal(input.journal)) {
        lowered.push(message);
    }

    let body: map<string, unknown> = {};
    let _ = body.set("model", c.model);
    let _ = body.set("max_tokens", c.max_tokens);
    // The API requires a non-empty message list that starts with a user
    // message. Normal role-less prompts already lower to user. For the unusual
    // system-only/assistant-first shape, move the system blocks into a leading
    // user message, matching the old first-turn fallback without flattening
    // an otherwise well-formed multi-message prompt.
    let first_role = lowered.at(0)?.role ?? "";
    if (first_role != "user" && system.length() > 0) {
        let first = AnthropicMessage { role: "user", tool_results: false, blocks: system };
        let _ = body.set("messages", _anthropic_render_messages([first].concat(lowered)));
        null
    } else {
        if (system.length() > 0) {
            let _ = body.set("system", system);
            null
        } else {
            null
        };
        let _ = body.set("messages", _anthropic_render_messages(lowered));
        null
    };
    if (!input.toolbox.is_empty()) {
        let tools: map<string, unknown>[] = [];
        for (let t in input.toolbox.list()) {
            let tw: map<string, unknown> = {};
            let _ = tw.set("name", t.name);
            let _ = tw.set("description", t.description);
            let _ = tw.set("input_schema", t.input_schema);
            tools.push(tw);
        }
        let tc: map<string, unknown> = {};
        let _ = tc.set("type", "auto");
        let _ = body.set("tools", tools);
        let _ = body.set("tool_choice", tc);
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

    let base = c.base_url ?? "https://api.anthropic.com";
    baml.http.Request {
        method: "POST",
        url: `${base}/v1/messages`,
        headers: {
            "x-api-key": c.api_key ?? baml.env.get_or_panic("ANTHROPIC_API_KEY"),
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
        body: baml.json.stringify(baml.json.to_json(body)),
    }
}

// One wire stop_reason string -> the canonical stop reason.
function _anthropic_stop_reason(s: string) -> ai.content.StopReason {
    if (s == "tool_use") {
        ai.content.StopReason.ToolUse
    } else if (s == "max_tokens") {
        ai.content.StopReason.MaxTokens
    } else if (s == "refusal") {
        ai.content.StopReason.Refused
    } else {
        ai.content.StopReason.Complete
    }
}

// The one internal entry point: render, send, normalize — one model turn.
function invoke(c: root.AnthropicClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = _anthropic_request(c, input, false);
    let resp = ai.wire.send_as<AnthropicResponse>(req, "anthropic");

    let content: ai.content.Block[] = [];
    for (let b in resp.content) {
        if (b.type == "text") {
            content.push(ai.content.Text { text: b.text ?? "" });
            null
        } else if (b.type == "thinking") {
            content.push(ai.content.Reasoning { summary: b.thinking ?? "" });
            null
        } else if (b.type == "tool_use") {
            let empty_args: map<string, unknown> = {};
            content.push(
                ai.content.ToolUse {
                    id: b.id ?? "",
                    name: b.name ?? "",
                    args: b.input ?? empty_args,
                },
            );
            null
        } else {
            null
        };
    }

    let stop = match (resp.stop_reason) {
        null => ai.content.StopReason.Complete,
        let s: string => _anthropic_stop_reason(s),
    };

    let usage: ai.events.Usage? = match (resp.usage) {
        let u: AnthropicUsage => {
            ai.events.Usage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                cached_input_tokens: null,
                reasoning_tokens: null,
            }
        },
        null => null,
    };

    ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}

// ---------------------------------------------------------------------------
// Streaming (Messages API SSE).
//
// Wire events observed: `message_start` carries `message.usage.input_tokens`;
// `content_block_delta` carries text deltas as `delta.type == "text_delta"`
// with the piece in `delta.text`; `message_delta` carries
// `delta.stop_reason` and cumulative `usage.output_tokens`; `message_stop`
// ends the turn.

// Streaming usage objects carry only some fields per event, so every field
// is optional (unlike the non-streaming AnthropicUsage).
class AnthStreamUsage {
    input_tokens: int?,
    output_tokens: int?,
}

class AnthStreamDelta {
    type: string?,
    text: string?,
    stop_reason: string?,
}

class AnthStreamMessage {
    usage: AnthStreamUsage?,
}

// The envelope of one SSE `data:` payload; only what the decoder reads.
class AnthStreamEvent {
    type: string?,
    delta: AnthStreamDelta?,
    message: AnthStreamMessage?,
    usage: AnthStreamUsage?,
}

// One SseStream batch -> provider-agnostic stream events.
function _anthropic_decode_batch(batch: string) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    for (let raw in ai.stream.decode_sse_batch(batch)) {
        if let data: string = raw.data {
            let ev: AnthStreamEvent? = baml.json.from_string<AnthStreamEvent>(data) catch_all (e) {
                _ => null,
            };
            if let event: AnthStreamEvent = ev {
                let t = event.type ?? raw.event ?? "";
                if (t == "content_block_delta") {
                    if let d: AnthStreamDelta = event.delta {
                        if ((d.type ?? "") == "text_delta") {
                            if let text: string = d.text {
                                out.push(ai.stream.TextDelta { text: text });
                                null
                            } else {
                                null
                            }
                        } else {
                            null
                        }
                    } else {
                        null
                    };
                    null
                } else if (t == "message_start") {
                    out.push(
                        ai.stream.TurnMeta {
                            stop_reason: null,
                            input_tokens: event.message?.usage?.input_tokens,
                            output_tokens: event.message?.usage?.output_tokens,
                        },
                    );
                    null
                } else if (t == "message_delta") {
                    let stop: ai.content.StopReason? = match (event.delta?.stop_reason) {
                        null => null,
                        let s: string => _anthropic_stop_reason(s),
                    };
                    out.push(
                        ai.stream.TurnMeta {
                            stop_reason: stop,
                            input_tokens: event.usage?.input_tokens,
                            output_tokens: event.usage?.output_tokens,
                        },
                    );
                    null
                } else if (t == "message_stop") {
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
function invoke_stream(c: root.AnthropicClient, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
    let req = _anthropic_request(c, input, true);
    let sse = baml.http.fetch_sse(req) catch_all (e) {
        _ => {
            throw ai.errors.NetworkFailure {
                provider: "anthropic",
                detail: `transport failed before a response arrived: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    ai.stream.TurnStream.from_sse(sse, _anthropic_decode_batch)
}
