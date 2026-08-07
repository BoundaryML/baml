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
// root.ai.Journal lowering. Anthropic requires strict user/assistant alternation and
// tool_result blocks in a user message immediately after the assistant
// tool_use message, so consecutive tool results group into ONE user message.

// An intermediate lowered message: role plus wire content blocks.
// tool_results marks a user message that is an open tool_result group.
class AnthropicMessage {
    role: string,
    tool_results: bool,
    blocks: map<string, unknown>[],
}

function _anthropic_assistant_blocks(content: root.ai.ContentBlock[]) -> map<string, unknown>[] {
    let out: map<string, unknown>[] = [];
    for (let b in content) {
        match (b) {
            let t: root.ai.Text => {
                let bb: map<string, unknown> = {};
                let _ = bb.set("type", "text");
                let _ = bb.set("text", t.text);
                out.push(bb);
                null
            },
            // Dropped: replaying thinking without its signature is rejected
            // by the API; readable-projection replay is a phase 2 capability.
            let r: root.ai.Reasoning => null,
            let u: root.ai.ToolUse => {
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
    let last: AnthropicMessage? = if (msgs.length() > 0) {
        msgs.at(msgs.length() - 1)
    } else {
        null
    };
    match (last) {
        let m: AnthropicMessage => {
            if (m.tool_results) {
                m.blocks.push(block);
                null
            } else {
                msgs.push(AnthropicMessage { role: "user", tool_results: true, blocks: [block] });
                null
            }
        },
        null => {
            msgs.push(AnthropicMessage { role: "user", tool_results: true, blocks: [block] });
            null
        },
    }
}

// root.ai.RunStarted/root.ai.ToolRequested/root.ai.Usage/root.ai.FinalProduced are not promptable and skip;
// skipped events never close an open tool_result group.
function _anthropic_lower_journal(j: root.ai.Journal) -> AnthropicMessage[] {
    let msgs: AnthropicMessage[] = [];
    for (let e in j.entries()) {
        match (e) {
            let m: root.ai.UserMessage => {
                let block: map<string, unknown> = {};
                let _ = block.set("type", "text");
                let _ = block.set("text", m.content);
                msgs.push(AnthropicMessage { role: "user", tool_results: false, blocks: [block] });
                null
            },
            let m: root.ai.AssistantMessage => {
                msgs.push(
                    AnthropicMessage {
                        role: "assistant",
                        tool_results: false,
                        blocks: _anthropic_assistant_blocks(m.content),
                    },
                );
                null
            },
            let c: root.ai.ToolCompleted => _anthropic_push_tool_result(msgs, c.id, c.output, false),
            let f: root.ai.ToolFailed => _anthropic_push_tool_result(msgs, f.id, f.message, true),
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

// The one internal entry point: render, send, normalize — one model turn.
function invoke(
    c: root.anthropic.AnthropicClient,
    input: root.ai.ModelTurnInput,
) -> root.ai.ModelTurn {
    let instructions = input.prompt.render_text(root.ai.render_output_format(input.output_type));
    let lowered = _anthropic_lower_journal(input.journal);

    let body: map<string, unknown> = {};
    let _ = body.set("model", c.model);
    let _ = body.set("max_tokens", c.max_tokens);
    // The API requires a non-empty message list that STARTS with a user
    // message. A repaired first step opens the journal with an assistant
    // message, so the first-turn rule generalizes (as it did for Gemini):
    // whenever the lowered journal does not begin with a user message, the
    // instructions lead as one, and system is omitted.
    let first_role = if let first: AnthropicMessage = lowered.at(0) {
        first.role
    } else {
        ""
    };
    if (first_role == "user") {
        let _ = body.set("system", instructions);
        let _ = body.set("messages", _anthropic_render_messages(lowered));
        null
    } else {
        let block: map<string, unknown> = {};
        let _ = block.set("type", "text");
        let _ = block.set("text", instructions);
        let msg: map<string, unknown> = {};
        let _ = msg.set("role", "user");
        let _ = msg.set("content", [block]);
        let _ = body.set("messages", [msg].concat(_anthropic_render_messages(lowered)));
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

    let base = c.base_url ?? "https://api.anthropic.com";
    let req = baml.http.Request {
        method: "POST",
        url: `${base}/v1/messages`,
        headers: {
            "x-api-key": c.api_key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
        body: baml.json.stringify(baml.json.to_json(body)),
    };
    let resp = root.ai.send_as<AnthropicResponse>(req, "anthropic");

    let content: root.ai.ContentBlock[] = [];
    for (let b in resp.content) {
        if (b.type == "text") {
            content.push(root.ai.Text { text: b.text ?? "" });
            null
        } else if (b.type == "thinking") {
            content.push(root.ai.Reasoning { summary: b.thinking ?? "" });
            null
        } else if (b.type == "tool_use") {
            let empty_args: map<string, unknown> = {};
            content
                .push(
                    root.ai.ToolUse {
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
        null => root.ai.StopReason.Complete,
        let s: string => {
            if (s == "tool_use") {
                root.ai.StopReason.ToolUse
            } else if (s == "max_tokens") {
                root.ai.StopReason.MaxTokens
            } else if (s == "refusal") {
                root.ai.StopReason.Refused
            } else {
                root.ai.StopReason.Complete
            }
        },
    };

    let usage: root.ai.Usage? = match (resp.usage) {
        let u: AnthropicUsage => {
            root.ai.Usage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                cached_input_tokens: null,
                reasoning_tokens: null,
            }
        },
        null => null,
    };

    root.ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}
