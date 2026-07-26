// A local-process provider backed by the authenticated Claude Code CLI.
//
// Claude Code is an agent harness rather than a raw model endpoint: it owns
// sessions, tools, permissions, and a JSONL event protocol. This adapter uses
// its constrained structured-output mode for typed BAML results and preserves
// the CLI event transcript alongside normalized AgentEvents.
//
class ClaudeCodeCli {
    executable: string,
    model: string,
    cwd: string?,
    timeout_ms: int,
    max_turns: int,
    max_budget_usd: string?,
    permission_mode: string,
    tools: string[],
    allowed_tools: string[],
    safe_mode: bool,
    persist_session: bool,

    function complete_with_events<T>(
        self,
        task: Task<T>,
        on_event: ((json) -> void throws never)? = null,
    ) -> ClaudeCodeCliRun<T> throws baml.errors.UnknownError {
        let schema = baml.schema.json_schema(reflect.type_of<T>()) catch (e) {
            _ => {
                throw baml.errors.UnknownError {
        data: e,
        message: ["Claude Code CLI output type cannot be represented as JSON Schema"],
      }
            },
        };
        let args = self.command_args(task.prompt.text(), baml.json.stringify(schema));
        let process = baml
            .sys
            .start_process(
                self.executable,
                args,
                baml.sys.ProcessOptions {
                    cwd: self.cwd,
                    env: null,
                    timeout_ms: self.timeout_ms,
                    stdin: null,
                },
            ) catch (e) {
            _ => {
                throw baml.errors.UnknownError {
        data: e,
        message: ["failed to start the Claude Code CLI"],
      }
            },
        };
        defer {
            process.close()
        }
        let run: ClaudeCodeCliRun<T> = parse_claude_code_cli_lines<T, baml.errors.Io
                | baml.errors.Timeout>(self, process.stdout, on_event) catch (e) {
            let unknown: baml.errors.UnknownError => throw unknown,
            _ => {
                throw baml.errors.UnknownError {
                data: e,
                message: ["failed while reading Claude Code CLI events"],
            }
            },
        };
        let exit = process.wait() catch (e) {
            _ => {
                throw baml.errors.UnknownError {
                data: e,
                message: ["failed while waiting for the Claude Code CLI"],
            }
            },
        };
        if (!exit.ok()) {
            throw baml.errors.UnknownError {
        data: {
          "exit_code": exit.exit_code,
          "signal": exit.signal,
        },
        message: ["Claude Code CLI exited unsuccessfully"],
      };
        }
        run
    }

    function command_args(self, prompt: string, json_schema: string) -> string[] throws never {
        let args: string[] = [];
        if (self.safe_mode) {
            args.push("--safe-mode");
        }
        args.push("-p");
        args.push("--output-format");
        args.push("stream-json");
        args.push("--verbose");
        args.push("--include-partial-messages");
        args.push("--model");
        args.push(self.model);
        args.push("--max-turns");
        args.push(self.max_turns.to_string());
        match (self.max_budget_usd) {
            let budget: string => {
                args.push("--max-budget-usd");
                args.push(budget);
            },
            null => {},
        }
        args.push("--permission-mode");
        args.push(self.permission_mode);
        args.push("--tools");
        args.push(self.tools.join(","));
        if (self.allowed_tools.length() > 0) {
            args.push("--allowedTools");
            args.push(self.allowed_tools.join(","));
        }
        if (!self.persist_session) {
            args.push("--no-session-persistence");
        }
        args.push("--json-schema");
        args.push(json_schema);
        args.push(prompt);
        args
    }
}

class ClaudeCodeCliRun<T> {
    response: Response<T>,
    events: AgentEvent[],
    raw_events: json[],
    session_id: string,
    event_delivery: string,
}

function claude_code_cli() -> ClaudeCodeCli throws never {
    ClaudeCodeCli {
        executable: "claude",
        model: "haiku",
        cwd: null,
        timeout_ms: 120000,
        max_turns: 1,
        max_budget_usd: null,
        permission_mode: "dontAsk",
        tools: [],
        allowed_tools: [],
        safe_mode: true,
        persist_session: false,
    }
}

function parse_claude_code_cli_output<T>(
    provider: ClaudeCodeCli,
    stdout: string,
) -> ClaudeCodeCliRun<T> throws baml.errors.UnknownError {
    parse_claude_code_cli_lines<T, never>(provider, stdout.lines(), null) catch (e) {
        let unknown: baml.errors.UnknownError => throw unknown,
        _ => {
            throw baml.errors.UnknownError {
            data: e,
            message: ["failed while reading Claude Code CLI fixture events"],
        }
        },
    }
}

function parse_claude_code_cli_lines<T, E>(
    provider: ClaudeCodeCli,
    lines: baml.iter.Iterable<Item = string, Error = E>,
    on_event: ((json) -> void throws never)?,
) -> ClaudeCodeCliRun<T> throws baml.errors.UnknownError | E {
    let raw_events: json[] = [];
    let events: AgentEvent[] = [];
    let terminal: json = null;
    let terminal_found = false;
    let session_id = "";
    let actual_model: string? = null;

    for (let line in lines) {
        let trimmed = line.trim();
        if (trimmed.length() == 0) {
            continue;
        }
        let raw = baml.json.parse(trimmed) catch (e) {
            _ => {
                throw baml.errors.UnknownError {
        data: { "line": trimmed, "parse_error": e },
        message: ["Claude Code CLI emitted invalid JSONL"],
      }
            },
        };
        raw_events.push(raw);
        match (on_event) {
            let callback: ((json) -> void throws never) => {
                callback(raw);
            },
            null => {},
        }

        let kind = baml.json.path_or<string>(raw, ".type", "");
        let event_session_id = baml.json.path_or<string>(raw, ".session_id", "");
        if (event_session_id.length() > 0) {
            session_id = event_session_id;
        }

        if (kind == "system" && baml.json.path_or<string>(raw, ".subtype", "") == "init") {
            actual_model = baml.json.path_or<string?>(raw, ".model", provider.model);
            events
                .push(
                    ModelStartedEvent {
                        id: if (session_id.length() > 0) {
                            session_id
                        } else {
                            null
                        },
                        provider: "claude-code-cli",
                    },
                );
        }

        if (
            kind == "stream_event"
                && baml.json.path_or<string>(raw, ".event.type", "") == "content_block_delta"
        ) {
            let delta_kind = baml.json.path_or<string>(raw, ".event.delta.type", "");
            if (delta_kind == "text_delta") {
                events
                    .push(
                        TextDeltaEvent {
                            id: if (session_id.length() > 0) {
                                session_id
                            } else {
                                null
                            },
                            text: baml.json.path_or<string>(raw, ".event.delta.text", ""),
                            metadata: { "source": "claude_code_stream_json", "native_type": delta_kind },
                        },
                    );
            }
            if (delta_kind == "input_json_delta") {
                events
                    .push(
                        TextDeltaEvent {
                            id: if (session_id.length() > 0) {
                                session_id
                            } else {
                                null
                            },
                            text: baml.json.path_or<string>(raw, ".event.delta.partial_json", ""),
                            metadata: { "source": "claude_code_structured_output", "native_type": delta_kind },
                        },
                    );
            }
        }

        if (kind == "result") {
            terminal = raw;
            terminal_found = true;
        }
    }

    if (!terminal_found) {
        throw baml.errors.UnknownError {
      data: raw_events,
      message: ["Claude Code CLI stream ended without a result event"],
    };
    }
    let result = terminal;
    if (baml.json.path_or<bool>(result, ".is_error", false)) {
        throw baml.errors.UnknownError {
      data: result,
      message: ["Claude Code CLI returned an error result"],
    };
    }

    let result_text = baml.json.path_or<string>(result, ".result", "");
    let structured = baml.json.path<json>(result, ".structured_output") catch (e) {
        _ => {
            baml.json.parse(result_text) catch (parse_error) {
                _ => {
                    throw baml.errors.UnknownError {
        data: parse_error,
        message: ["Claude Code CLI result did not contain structured JSON output"],
      }
                },
            }
        }
    };
    let value: T = baml.json.from_json<T>(structured) catch (e) {
        _ => {
            throw baml.errors.UnknownError {
      data: e,
      message: ["Claude Code CLI structured output did not match the BAML type"],
    }
        },
    };

    let usage = Usage {
        input_tokens: baml.json.path_or<int>(result, ".usage.input_tokens", 0),
        output_tokens: baml.json.path_or<int>(result, ".usage.output_tokens", 0),
        cached_input_tokens: baml.json.path_or<int?>(result, ".usage.cache_read_input_tokens", null),
        reasoning_tokens: null,
        cost_usd: baml.json.path_or<float?>(result, ".total_cost_usd", null),
    };
    events
        .push(
            UsageEvent {
                id: if (session_id.length() > 0) {
                    session_id
                } else {
                    null
                },
                usage: usage,
            },
        );
    events
        .push(
            RunFinishedEvent {
                id: if (session_id.length() > 0) {
                    session_id
                } else {
                    null
                },
                outcome: baml.json.path_or<string>(result, ".terminal_reason", "completed"),
            },
        );

    ClaudeCodeCliRun<T> {
        response: Response<T> {
            value: value,
            meta: Meta {
                provider: "claude-code-cli",
                model: actual_model ?? provider.model,
                request_id: baml.json.path_or<string?>(result, ".uuid", null),
                finish_reason: baml.json.path_or<string?>(result, ".stop_reason", null),
                usage: usage,
                attributes: {
                    "session_id": session_id,
                    "transport": "local_process",
                    "event_delivery": "streaming_jsonl",
                },
                raw: result,
            },
            conversation: null,
        },
        events: events,
        raw_events: raw_events,
        session_id: session_id,
        event_delivery: "streaming_jsonl",
    }
}

implements Provider for ClaudeCodeCli {
  function name(self) -> string throws never { "claude-code-cli" }
  function render_shorthand(self) -> string throws never { "anthropic/" + self.model }
}

implements GenerationProvider for ClaudeCodeCli {
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    self.complete_with_events<T>(task).response
  }
}

implements CompletionProvider for ClaudeCodeCli {
  function complete<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    self.complete_with_events<T>(task).response
  }
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}
