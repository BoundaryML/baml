// Claude Code CLI internals: prompt assembly, the outcome envelope, the
// process run, and result normalization.

// The journal as labeled text. Claude Code's -p mode takes one prompt, so
// the conversation folds into it; the CLI's own session state is unused.
function transcript_text(j: ai.Journal) -> string {
    let lines: string[] = [];
    for (let e in j.entries()) {
        match (e) {
            let u: ai.events.UserMessage => {
                lines.push(`user: ${u.content}`);
            },
            let a: ai.events.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let t: ai.content.Text => {
                            lines.push(`assistant: ${t.text}`);
                        },
                        let c: ai.content.ToolUse => {
                            lines.push(
                                `assistant tool_call: {"id": "${c.id}", "name": "${c.name}", "args": ${baml.json.stringify(baml.json.to_json(c.args))}}`,
                            );
                        },
                        _ => {
                            null
                        },
                    };
                }
            },
            let done: ai.events.ToolCompleted => {
                lines.push(`tool result id=${done.id} is_error=false: ${done.output}`);
            },
            let failed: ai.events.ToolFailed => {
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
    let final_root = baml.json.from_json<map<string, unknown>>(baml.json.schema(output_type));
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
function tool_protocol(tb: ai.tools.Toolbox) -> string {
    let catalog: string[] = [];
    for (let t in tb.list()) {
        catalog.push(
            `{"name": "${t.name}", "description": "${t.description}", "parameters": ${baml.json.stringify(baml.json.to_json(t.input_schema))}}`,
        );
    }
    `\n\nThe output contract is one object with an outcome field. Set outcome
to either the final result or a tool request. The tools in the catalog
below are NOT installed in your environment — never attempt to invoke
them directly; any such attempt fails. The ONLY way to run one is to
finish your reply with outcome set to
{"calls": [{"id": "<unique id>", "name": "<exact tool name>", "args": {...}}]};
the caller executes the calls and re-invokes you with their results in
the conversation. If the Conversation so far section shows no tool
results yet, your outcome MUST be a calls request — never invent data a
tool can provide. After tool results appear in the conversation, use
them; the Conversation so far section is authoritative. A tool result
with is_error=false means that call already ran — never repeat the same
successful tool call. Request only tools that are still necessary, or
return the final result.

Application tool catalog (request via outcome.calls only):
[${catalog.join(", ")}]`
}

// Consume the live JSONL stream: log every event as it arrives, return the
// terminal result event.
function _read_result(p: baml.sys.Process) -> json {
    let result: json = null;
    for (let line in p.stdout.lines()) {
        let trimmed = line.trim();
        if (trimmed.length() > 0) {
            let raw = baml.json.parse(trimmed) catch_all (e) {
                _ => {
                    throw ai.errors.ParseFailed { provider: "claude-code", raw_output: trimmed }
                },
            };
            _log_cc_event(raw);
            if (baml.json.path_or<string>(raw, ".type", "") == "result") {
                result = raw;
            }
        } else {
            null
        };
    }
    if (result == null) {
        throw ai.errors.ParseFailed {
            provider: "claude-code",
            raw_output: "the Claude Code CLI stream ended without a result event",
        }
    }
    result
}

function _preview(text: string) -> string {
    if (text.length() > 120) {
        `${text.slice(0, 120)}...`
    } else {
        text
    }
}

// One log line per Claude Code JSONL event: the harness's own transcript.
function _log_cc_event(raw: json) -> void {
    let kind = baml.json.path_or<string>(raw, ".type", "?");
    match (kind) {
        "system" => {
            let subtype = baml.json.path_or<string>(raw, ".subtype", "?");
            if (subtype == "thinking_tokens") {
                // a progress counter, never content — the thinking text arrives
                // later inside the assistant message's thinking block
                let estimated = baml.json.path_or<int>(raw, ".estimated_tokens", 0);
                log.debug(`cc event: thinking ~${estimated} tokens`);
            } else {
                let model = baml.json.path_or<string>(raw, ".model", "?");
                log.info(`cc event: system/${subtype} model=${model}`);
            };
        },
        "assistant" | "user" => {
            let parts: string[] = [];
            let blocks = baml.json.path_or<json[]>(raw, ".message.content", []);
            for (let b in blocks) {
                let bt = baml.json.path_or<string>(b, ".type", "?");
                match (bt) {
                    "text" => {
                        parts.push(`text: ${_preview(baml.json.path_or<string>(b, ".text", ""))}`)
                    },
                    "thinking" => {
                        parts.push(
                            `thinking: ${_preview(baml.json.path_or<string>(b, ".thinking", ""))}`,
                        )
                    },
                    "tool_use" => {
                        parts.push(`tool_use: ${baml.json.path_or<string>(b, ".name", "?")}`)
                    },
                    "tool_result" => parts.push("tool_result"),
                    _ => parts.push(bt),
                };
            }
            log.info(`cc event: ${kind} [${parts.join(" | ")}]`);
        },
        "result" => {
            // logged separately with cost and session
        },
        _ => log.info(`cc event: ${kind}`),
    };
}

// The client's mcp_servers as one --mcp-config JSON string, or null when no
// server is attached. The map is read here, at render time, so a server
// added between turns rides the next turn's process.
function mcp_config_json(c: root.ClaudeCodeClient) -> string? {
    if (c.mcp_servers.length() == 0) {
        null
    } else {
        let wrapper: map<string, unknown> = {};
        let _ = wrapper.set("mcpServers", c.mcp_servers);
        baml.json.stringify(baml.json.to_json(wrapper))
    }
}

// One mcp__<name> entry per attached server: the --allowedTools value that
// permits every tool the server provides.
function allowed_mcp_tools(c: root.ClaudeCodeClient) -> string {
    c.mcp_servers.keys()
        .map((n: string) -> {
            `mcp__${n}`
        })
        .join(",")
}

// One model turn = one CLI run: assemble the prompt, exec, normalize.
function invoke(c: root.ClaudeCodeClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    // the contract travels via --json-schema, so the prompt's schema slot
    // renders empty
    // The CLI accepts one string rather than a chat-message array. Keep the
    // structural prompt until this transport boundary, then use its readable
    // role-headed projection.
    let instructions = input.prompt("").text();
    let has_tools = !input.toolbox.is_empty();
    let schema = if (has_tools) {
        baml.json.to_json(envelope_schema(input.output_type))
    } else {
        baml.json.schema(input.output_type)
    };
    let prompt_text = if (has_tools) {
        `${instructions}${transcript_text(input.journal)}${tool_protocol(input.toolbox)}`
    } else {
        `${instructions}${transcript_text(input.journal)}`
    };

    log.info(
        `claude-code invoke: model=${c.model} prompt_chars=${prompt_text.length()} baml_tools=${input.toolbox.list().length()}`,
    );
    let args: string[] = [];
    args.push("-p");
    args.push("--output-format");
    args.push("stream-json");
    args.push("--verbose");
    args.push("--model");
    args.push(c.model);
    args.push("--permission-mode");
    args.push(c.permission_mode);
    args.push("--no-session-persistence");
    args.push("--tools");
    args.push(c.harness_tools.join(","));
    // --tools governs the built-in set only; MCP servers attach separately.
    // --strict-mcp-config keeps the set deterministic: only the client's
    // servers, never inherited user or project MCP configuration. -p mode
    // cannot answer a permission prompt, so attaching a server implies
    // allowing its tools; the equals form matters, because --allowedTools
    // is variadic and would otherwise swallow the trailing prompt argument.
    if let mcp: string = mcp_config_json(c) {
        args.push("--mcp-config");
        args.push(mcp);
        args.push("--strict-mcp-config");
        args.push(`--allowedTools=${allowed_mcp_tools(c)}`);
    }
    args.push("--json-schema");
    args.push(baml.json.stringify(schema));
    args.push(prompt_text);

    let p = baml.sys.start_process(
        c.executable,
        args,
        baml.sys.ProcessOptions { cwd: c.cwd, env: null, timeout_ms: c.timeout_ms, stdin: null },
    ) catch_all (e) {
        _ => {
            throw ai.errors.NetworkFailure {
                provider: "claude-code",
                detail: `the Claude Code CLI could not start: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    defer {
        p.close()
    }
    // the CLI reads stdin when the pipe is open; deliver EOF immediately
    let _ = {
        p.stdin.close();
        null
    } catch_all (e) {
        _ => null,
    };
    // events stream LIVE: each JSONL line logs as it arrives, and the last
    // relevant line is the result. Stderr inherits the parent process.
    let result = _read_result(p);
    let exit = p.wait() catch_all (e) {
        _ => {
            throw ai.errors.NetworkFailure {
                provider: "claude-code",
                detail: `the Claude Code CLI did not terminate cleanly: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    if (!exit.ok()) {
        throw ai.errors.InvalidRequest {
            provider: "claude-code",
            status_code: null,
            detail: `the Claude Code CLI exited with code ${exit.exit_code}`,
            raw_body: null, // stderr inherits the parent and is not captured
        }
    }

    if (baml.json.path_or<bool>(result, ".is_error", false)) {
        throw ai.errors.Refused {
            provider: "claude-code",
            reason: baml.json.path_or<string>(result, ".subtype", "error"),
            raw_body: baml.json.stringify(result),
        }
    }
    log.info(
        `claude-code result: subtype=${baml.json.path_or<string>(result, ".subtype", "?")} cost_usd=${baml.json.path_or<float>(result, ".total_cost_usd", 0.0)} session=${baml.json.path_or<string>(result, ".session_id", "?")}`,
    );
    let structured = baml.json.path<json>(result, ".structured_output") catch_all (e) {
        _ => {
            baml.json.parse(baml.json.path_or<string>(result, ".result", "")) catch_all (inner) {
                _ => {
                    throw ai.errors.ParseFailed {
                        provider: "claude-code",
                        raw_output: baml.json.stringify(result),
                    }
                },
            }
        }
    };

    let usage = ai.events.Usage {
        input_tokens: baml.json.path_or<int>(result, ".usage.input_tokens", 0),
        output_tokens: baml.json.path_or<int>(result, ".usage.output_tokens", 0),
        cached_input_tokens: baml.json.path_or<int?>(result, ".usage.cache_read_input_tokens", null),
        reasoning_tokens: null,
    };

    let content: ai.content.Block[] = [];
    let stop: ai.content.StopReason = ai.content.StopReason.Complete;
    if (has_tools) {
        let outcome = baml.json.path_or<json>(result, ".structured_output.outcome", structured);
        let calls = baml.json.path_or<json[]?>(outcome, ".calls", null);
        if let call_list: json[] = calls {
            let i = 0;
            for (let raw_call in call_list) {
                let id = baml.json.path_or<string>(
                    raw_call,
                    ".id",
                    `cc_call_${input.journal.entries().length()}_${i}`,
                );
                let name = baml.json.path_or<string>(raw_call, ".name", "");
                let call_args = baml.json.from_json<map<string, unknown>>(
                    baml.json.path_or<json>(raw_call, ".args", {}),
                ) catch_all (e) {
                    _ => {
                        let empty: map<string, unknown> = {};
                        empty
                    },
                };
                content.push(ai.content.ToolUse { id: id, name: name, args: call_args });
                i = i + 1;
            }
            stop = ai.content.StopReason.ToolUse;
        } else {
            content.push(ai.content.Text { text: baml.json.stringify(outcome) });
        }
    } else {
        content.push(ai.content.Text { text: baml.json.stringify(structured) });
    }
    ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}
