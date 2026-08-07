// Claude Code CLI internals: prompt assembly, the outcome envelope, the
// process run, and result normalization.

// The journal as labeled text. Claude Code's -p mode takes one prompt, so
// the conversation folds into it; the CLI's own session state is unused.
function transcript_text(j: root.ai.Journal) -> string {
    let lines: string[] = [];
    for (let e in j.entries()) {
        match (e) {
            let u: root.ai.UserMessage => {
                lines.push(`user: ${u.content}`);
            },
            let a: root.ai.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let t: root.ai.Text => {
                            lines.push(`assistant: ${t.text}`);
                        },
                        let c: root.ai.ToolUse => {
                            lines
                                .push(
                                    `assistant tool_call: {"id": "${c.id}", "name": "${c.name}", "args": ${baml.json.stringify(baml.json.to_json(c.args))}}`,
                                );
                        },
                        _ => {
                            null
                        },
                    };
                }
            },
            let done: root.ai.ToolCompleted => {
                lines.push(`tool result id=${done.id} is_error=false: ${done.output}`);
            },
            let failed: root.ai.ToolFailed => {
                lines.push(`tool result id=${failed.id} is_error=true: ${failed.message}`);
            },
            _ => {
                null
            },
        };
    }
    if (lines.length() == 0) {
        ""
    } else {
        `\n\nConversation so far:\n${lines.join("\n")}`
    }
}

// {outcome: anyOf [T-schema, calls-schema]} — the constrained output that
// lets one CLI run return either the final result or BAML tool calls. T's
// $defs lift to the envelope root so its $refs stay valid.
function envelope_schema(output_type: type) -> map<string, unknown> {
    let final_root = baml
        .json
        .from_json<map<string, unknown>>(baml.schema.json_schema(output_type));
    let definitions = final_root.delete("$defs");
    let call_props: map<string, unknown> = {};
    let _ = call_props.set("id", _type_schema("string"));
    let _ = call_props.set("name", _type_schema("string"));
    let _ = call_props.set("args", _type_schema("object"));
    let call_item: map<string, unknown> = {};
    let _ = call_item.set("type", "object");
    let _ = call_item.set("properties", call_props);
    let _ = call_item.set("required", ["id", "name", "args"]);
    let _ = call_item.set("additionalProperties", false);
    let calls_array: map<string, unknown> = {};
    let _ = calls_array.set("type", "array");
    let _ = calls_array.set("items", call_item);
    let calls_props: map<string, unknown> = {};
    let _ = calls_props.set("calls", calls_array);
    let calls_schema: map<string, unknown> = {};
    let _ = calls_schema.set("type", "object");
    let _ = calls_schema.set("properties", calls_props);
    let _ = calls_schema.set("required", ["calls"]);
    let _ = calls_schema.set("additionalProperties", false);
    let outcome: map<string, unknown> = {};
    let _ = outcome.set("anyOf", [baml.json.to_json(final_root), baml.json.to_json(calls_schema)]);
    let props: map<string, unknown> = {};
    let _ = props.set("outcome", outcome);
    let envelope: map<string, unknown> = {};
    let _ = envelope.set("type", "object");
    let _ = envelope.set("properties", props);
    let _ = envelope.set("required", ["outcome"]);
    let _ = envelope.set("additionalProperties", false);
    if (definitions != null) {
        let _ = envelope.set("$defs", definitions);
    }
    envelope
}

function _type_schema(t: string) -> map<string, unknown> {
    let m: map<string, unknown> = {};
    let _ = m.set("type", t);
    m
}

// The envelope protocol, appended to the prompt when BAML tools are active.
// The guidance about never repeating a successful call is load-bearing:
// each invoke is a fresh CLI process, and the conversation text is the only
// memory it has.
function tool_protocol(tb: root.ai.Toolbox) -> string {
    let catalog: string[] = [];
    for (let t in tb.list()) {
        catalog
            .push(
                `{"name": "${t.name}", "description": "${t.description}", "parameters": ${baml.json.stringify(baml.json.to_json(t.input_schema))}}`,
            );
    }
    `\n\nThe output contract is one object with an outcome field. Set outcome
to either the final result or a tool request. To call application tools,
set outcome to {"calls": [{"id": "<unique id>", "name": "<exact tool name>", "args": {...}}]}.
If the Conversation so far section shows no tool results yet, your outcome
MUST be a calls request — never invent data a tool can provide. After tool
results appear in the conversation, use them; the Conversation so far
section is authoritative. A tool result with is_error=false means that
call already ran — never repeat the same successful tool call. Request
only tools that are still necessary, or return the final result.

Available tools:
[${catalog.join(", ")}]`
}

// One model turn = one CLI run: assemble the prompt, exec, normalize.
function invoke(c: root.claude_code.ClaudeCodeClient, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
    // the contract travels via --json-schema, so the prompt's schema slot
    // renders empty
    let instructions = input.prompt.render_text("");
    let has_tools = !input.toolbox.is_empty();
    let schema = if (has_tools) {
        baml.json.to_json(envelope_schema(input.output_type))
    } else {
        baml.schema.json_schema(input.output_type)
    };
    let prompt_text = if (has_tools) {
        `${instructions}${transcript_text(input.journal)}${tool_protocol(input.toolbox)}`
    } else {
        `${instructions}${transcript_text(input.journal)}`
    };

    let args: string[] = [];
    args.push("-p");
    args.push("--output-format");
    args.push("json");
    args.push("--model");
    args.push(c.model);
    args.push("--permission-mode");
    args.push(c.permission_mode);
    args.push("--no-session-persistence");
    args.push("--tools");
    args.push(c.harness_tools.join(","));
    args.push("--json-schema");
    args.push(baml.json.stringify(schema));
    args.push(prompt_text);

    let out = baml
        .sys
        .exec(
            c.executable,
            args,
            baml.sys.ProcessOptions { cwd: c.cwd, env: null, timeout_ms: c.timeout_ms, stdin: null },
        ) catch_all (e) {
        _ => {
            throw root.ai.errors.NetworkFailure {
                provider: "claude-code",
                detail: `the Claude Code CLI did not complete: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    let stdout = out.stdout.to_string();
    if (!out.ok()) {
        throw root.ai.errors.InvalidRequest {
            provider: "claude-code",
            status_code: null,
            detail: `the Claude Code CLI exited with code ${out.exit_code}`,
            raw_body: out.stderr.to_string(),
        }
    }
    let result = baml.json.parse(stdout) catch_all (e) {
        _ => throw root.ai.errors.ParseFailed { provider: "claude-code", raw_output: stdout },
    };
    if (baml.json.path_or<bool>(result, ".is_error", false)) {
        throw root.ai.errors.Refused {
            provider: "claude-code",
            reason: baml.json.path_or<string>(result, ".subtype", "error"),
            raw_body: stdout,
        }
    }
    let structured = baml.json.path<json>(result, ".structured_output") catch_all (e) {
        _ => {
            baml.json.parse(baml.json.path_or<string>(result, ".result", "")) catch_all (inner) {
                _ => throw root.ai.errors.ParseFailed { provider: "claude-code", raw_output: stdout },
            }
        }
    };

    let usage = root
        .ai
        .Usage {
        input_tokens: baml.json.path_or<int>(result, ".usage.input_tokens", 0),
        output_tokens: baml.json.path_or<int>(result, ".usage.output_tokens", 0),
        cached_input_tokens: baml.json.path_or<int?>(result, ".usage.cache_read_input_tokens", null),
        reasoning_tokens: null,
    };

    let content: root.ai.ContentBlock[] = [];
    let stop: root.ai.StopReason = root.ai.StopReason.Complete;
    if (has_tools) {
        let outcome = baml.json.path_or<json>(result, ".structured_output.outcome", structured);
        let calls = baml.json.path_or<json[]?>(outcome, ".calls", null);
        if let call_list: json[] = calls {
            let i = 0;
            for (let raw_call in call_list) {
                let id = baml.json.path_or<string>(raw_call, ".id", `cc_call_${input.journal.entries().length()}_${i}`);
                let name = baml.json.path_or<string>(raw_call, ".name", "");
                let call_args = baml
                    .json
                    .from_json<map<string, unknown>>(baml.json.path_or<json>(raw_call, ".args", {})) catch_all (e) {
                    _ => {
                        let empty: map<string, unknown> = {};
                        empty
                    },
                };
                content.push(root.ai.ToolUse { id: id, name: name, args: call_args });
                i = i + 1;
            }
            stop = root.ai.StopReason.ToolUse;
        } else {
            content.push(root.ai.Text { text: baml.json.stringify(outcome) });
        }
    } else {
        content.push(root.ai.Text { text: baml.json.stringify(structured) });
    }
    root.ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}
