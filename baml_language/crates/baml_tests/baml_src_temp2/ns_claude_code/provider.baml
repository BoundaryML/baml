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
    max_application_tool_steps: int,
    max_budget_usd: string?,
    permission_mode: string,
    tools: string[],
    allowed_tools: string[],
    safe_mode: bool,
    persist_session: bool,
    harness_sessions: ClaudeCodeHarnessSession[],

    function complete_with_events<T>(
        self,
        task: root.ai.Task<T>,
        on_event: ((root.ai.observe.AgentEvent) -> null throws never)? = null,
        output_schema: ClaudeCodeJsonSchema? = null,
    ) -> ClaudeCodeCliRun<T> throws baml.errors.UnknownError {
        let schema = match (output_schema) {
            let explicit: ClaudeCodeJsonSchema => explicit.value,
            null => {
                baml.schema.json_schema(reflect.type_of<T>()) catch (e) {
                    _ => {
                        throw baml.errors.UnknownError.with_message<never>(e, "Claude Code CLI output type cannot be represented as JSON Schema")
                    },
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
                throw baml.errors.UnknownError.with_message<never>(e, "failed to start the Claude Code CLI")
            },
        };
        defer {
            process.close()
        }
        let run: ClaudeCodeCliRun<T> = _parse_claude_code_cli_lines<T, baml.errors.Io
                | baml.errors.Timeout>(self, process.stdout, on_event) catch (e) {
            let unknown: baml.errors.UnknownError => throw unknown,
            _ => {
                throw baml.errors.UnknownError.with_message<never>(e, "failed while reading Claude Code CLI events")
            },
        };
        let exit = process.wait() catch (e) {
            _ => {
                throw baml.errors.UnknownError.with_message<never>(e, "failed while waiting for the Claude Code CLI")
            },
        };
        if (!exit.ok()) {
            throw baml.errors.UnknownError.with_message<never>(
        { "exit_code": exit.exit_code, "signal": exit.signal },
        "Claude Code CLI exited unsuccessfully",
      );
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
    response: root.ai.ResponseWithMetadata<T>,
    events: root.ai.observe.AgentEvent[],
    raw_events: json[],
    session_id: string,
    event_delivery: string,
}

class ClaudeCodeHarnessSession {
    session_id: string,
    options: root.ai.harness.HarnessOptions,
    history: root.ai.MessageHistory,
    is_stopped: bool,
    steering: string[],

    implements root.ai.harness.HarnessSession {
        function id(self) -> string throws never { self.session_id }
        function messages(self) -> root.ai.Messages throws never { self.history }
        function stopped(self) -> bool throws never { self.is_stopped }
    }
    implements root.ai.Resource {}
}

class ClaudeCodeJsonSchema {
    value: json,
}

function claude_code_cli() -> ClaudeCodeCli throws never {
    ClaudeCodeCli {
        executable: "claude",
        model: "haiku",
        cwd: null,
        timeout_ms: 120000,
        max_turns: 1,
        max_application_tool_steps: 4,
        max_budget_usd: null,
        permission_mode: "dontAsk",
        tools: [],
        allowed_tools: [],
        safe_mode: true,
        persist_session: false,
        harness_sessions: [],
    }
}

function parse_claude_code_cli_output<T>(
    provider: ClaudeCodeCli,
    stdout: string,
) -> ClaudeCodeCliRun<T> throws baml.errors.UnknownError {
    _parse_claude_code_cli_lines<T, never>(provider, stdout.lines(), null) catch (e) {
        let unknown: baml.errors.UnknownError => throw unknown,
    }
}

function _parse_claude_code_cli_lines<T, E>(
    provider: ClaudeCodeCli,
    lines: baml.iter.Iterable<Item = string, Error = E>,
    on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
) -> ClaudeCodeCliRun<T> throws baml.errors.UnknownError | E {
    let raw_events: json[] = [];
    let events: root.ai.observe.AgentEvent[] = [];
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
                throw baml.errors.UnknownError.with_message<never>(
        { "line": trimmed, "parse_error": e },
        "Claude Code CLI emitted invalid JSONL",
      )
            },
        };
        raw_events.push(raw);

        let kind = baml.json.path_or<string>(raw, ".type", "");
        let event_session_id = baml.json.path_or<string>(raw, ".session_id", "");
        if (event_session_id.length() > 0) {
            session_id = event_session_id;
        }

        if (kind == "system" && baml.json.path_or<string>(raw, ".subtype", "") == "init") {
            actual_model = baml.json.path_or<string?>(raw, ".model", provider.model);
            root.ai.harness._emit_harness_event(
                root.ai.observe.ModelStartedEvent {
                    id: if (session_id.length() > 0) {
                        session_id
                    } else {
                        null
                    },
                    provider: "claude-code-cli",
                },
                events,
                on_event,
            );
        }

        if (
            kind == "stream_event"
                && baml.json.path_or<string>(raw, ".event.type", "") == "content_block_delta"
        ) {
            let delta_kind = baml.json.path_or<string>(raw, ".event.delta.type", "");
            if (delta_kind == "text_delta") {
                root.ai.harness._emit_harness_event(
                    root.ai.observe.TextDeltaEvent {
                        id: if (session_id.length() > 0) {
                            session_id
                        } else {
                            null
                        },
                        text: baml.json.path_or<string>(raw, ".event.delta.text", ""),
                        metadata: { "source": "claude_code_stream_json", "native_type": delta_kind },
                    },
                    events,
                    on_event,
                );
            }
            if (delta_kind == "input_json_delta") {
                root.ai.harness._emit_harness_event(
                    root.ai.observe.TextDeltaEvent {
                        id: if (session_id.length() > 0) {
                            session_id
                        } else {
                            null
                        },
                        text: baml.json.path_or<string>(raw, ".event.delta.partial_json", ""),
                        metadata: { "source": "claude_code_structured_output", "native_type": delta_kind },
                    },
                    events,
                    on_event,
                );
            }
        }

        if (
            kind == "stream_event"
                && baml.json.path_or<string>(raw, ".event.type", "") == "content_block_start"
                && baml.json.path_or<string>(raw, ".event.content_block.type", "") == "tool_use"
        ) {
            let call = root.ai.tools.ToolCall {
                id: baml.json.path_or<string>(raw, ".event.content_block.id", ""),
                name: baml.json.path_or<string>(raw, ".event.content_block.name", ""),
                args: baml.json.path_or<json>(raw, ".event.content_block.input", {}),
            };
            root.ai.harness._emit_harness_event(
                root.ai.observe.ToolCallEvent {
                    id: if (session_id.length() > 0) {
                        session_id
                    } else {
                        null
                    },
                    phase: "proposed",
                    call: call,
                    result: null,
                },
                events,
                on_event,
            );
        }

        if (kind == "result") {
            terminal = raw;
            terminal_found = true;
        }
    }

    if (!terminal_found) {
        throw baml.errors.UnknownError.with_message<never>(raw_events, "Claude Code CLI stream ended without a result event");
    }
    let result = terminal;
    if (baml.json.path_or<bool>(result, ".is_error", false)) {
        throw baml.errors.UnknownError.with_message<never>(result, "Claude Code CLI returned an error result");
    }

    let result_text = baml.json.path_or<string>(result, ".result", "");
    let structured = baml.json.path<json>(result, ".structured_output") catch (e) {
        _ => {
            baml.json.parse(result_text) catch (parse_error) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(parse_error, "Claude Code CLI result did not contain structured JSON output")
                },
            }
        }
    };
    let value: T = baml.json.from_json<T>(structured) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "Claude Code CLI structured output did not match the BAML type")
        },
    };

    let usage = root.ai.Usage {
        input_tokens: baml.json.path_or<int>(result, ".usage.input_tokens", 0),
        output_tokens: baml.json.path_or<int>(result, ".usage.output_tokens", 0),
        cached_input_tokens: baml.json.path_or<int?>(result, ".usage.cache_read_input_tokens", null),
        reasoning_tokens: null,
        cost_usd: baml.json.path_or<float?>(result, ".total_cost_usd", null),
    };
    root.ai.harness._emit_harness_event(
        root.ai.observe.UsageEvent {
            id: if (session_id.length() > 0) {
                session_id
            } else {
                null
            },
            usage: usage,
        },
        events,
        on_event,
    );
    root.ai.harness._emit_harness_event(
        root.ai.observe.RunFinishedEvent {
            id: if (session_id.length() > 0) {
                session_id
            } else {
                null
            },
            outcome: baml.json.path_or<string>(result, ".terminal_reason", "completed"),
        },
        events,
        on_event,
    );

    ClaudeCodeCliRun<T> {
        response: root.ai.ResponseWithMetadata<T> {
            value: value,
            metadata: root.ai.ResponseMetadata {
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

// Claude Code requires the structured-output schema to be rooted at an object.
// Keep the BAML class non-generic and supply the precise generic schema
// separately so an unresolved nested TypeVar cannot reach ctx.output_format.
class ClaudeCodeCompletionTurn {
    outcome: json,
}

function _claude_code_completion_turn_schema<T>() -> json throws baml.errors.UnknownError {
    let final_schema = baml.schema.json_schema(reflect.type_of<T>()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion result cannot be represented as JSON Schema")
        },
    };
    let final_root = baml.json.from_json<map<string, json>>(final_schema) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion schema must have an object root")
        },
    };
    // References emitted for T are rooted at #/$defs. Move those definitions
    // to the combined envelope root before nesting T beneath outcome.anyOf.
    let definitions: json = final_root.delete("$defs") ?? {};
    let embedded_final = baml.json.to_json(final_root) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion schema could not be embedded")
        },
    };
    // Tool arguments are checked again by invoke_tool. Keeping args as an
    // object avoids importing ToolCalls' recursive json $defs here.
    let calls_schema: json = {
        "type": "object",
        "properties": {
            "calls": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" },
                        "args": { "type": "object" },
                    },
                    "required": ["id", "name", "args"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["calls"],
        "additionalProperties": false,
    };
    {
        "type": "object",
        "properties": { "outcome": { "anyOf": [embedded_final, calls_schema] } },
        "required": ["outcome"],
        "additionalProperties": false,
        "$defs": definitions,
    }
}

function _claude_code_completion_suffix<T>(
    tools: root.ai.tools.Tool[],
) -> string throws baml.errors.UnknownError {
    let final_schema = baml.schema.json_schema(reflect.type_of<T>()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion result cannot be represented as JSON Schema")
        },
    };
    let schemas = tools.map((tool: root.ai.tools.Tool) -> json {
        root.ai.tools._prompt_tool_schema(tool)
    });
    "\n\n"
        + `
            The transport returns one object with an outcome field. Set outcome
            to either the final result or ToolCalls. If an application tool is needed,
            return ToolCalls using an exact tool name and matching arguments. After tool
            results appear in the conversation, return the final result in outcome.

            The Conversation so far section is authoritative across Claude Code CLI
            invocations. A tool entry with is_error=false means that application tool
            already ran successfully. Never repeat the same successful tool call. Use its
            output to produce the final result, or request only a different tool that is
            still necessary.

            Final result schema:
            ${baml.json.stringify(final_schema)}

            Application tools:
            ${baml.json.stringify(schemas)}
        `
}

function _claude_code_complete_with_application_tools<T>(
    provider: ClaudeCodeCli,
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T> throws baml.errors.UnknownError {
    let tools = task.tools;
    let adapter = root.ai.tools.PromptToolProvider { inner: provider };
    let conversation = adapter.begin<T>(task);
    let state = match (conversation) {
        let value: root.ai.tools.PromptToolConversation => value,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(null, "Claude Code completion conversation had the wrong type")
        },
    };
    let schema = _claude_code_completion_turn_schema<T>();
    let suffix = _claude_code_completion_suffix<T>(tools);
    let step = 0;
    //# Fulfill complete<T> through an explicitly bounded application-tool loop
    while (step < provider.max_application_tool_steps) {
        let run = provider
            .complete_with_events<ClaudeCodeCompletionTurn>(
                root.ai.tools._prompt_tool_turn<ClaudeCodeCompletionTurn>(
                    adapter,
                    state,
                    tools,
                    suffix,
                ),
                output_schema = ClaudeCodeJsonSchema { value: schema },
            );
        let outcome: T | root.ai.tools.ToolCalls = baml.json
            .from_json<T | root.ai.tools.ToolCalls>(run.response.value.outcome) catch (e) {
            _ => {
                throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion returned an invalid outcome")
            },
        };
        step = step + 1;
        //# Return the final typed value
        match (outcome) {
            let calls: root.ai.tools.ToolCalls => {
                //## Preserve the requested calls in the prompt conversation
                state
                    .history
                    .entries
                    .push(
                        root.ai.ChatMessage
                            .assistant(
                                baml.json
                                    .stringify(
                                        {
                                            "calls": calls.calls.map((call: root.ai.tools.ToolCall) -> json {
                                                {
                                                    "id": call.id,
                                                    "name": call.name,
                                                    "args": call.args,
                                                }
                                            }),
                                        },
                                    ),
                            ),
                    );
                //## Dispatch BAML functions and submit correlated results
                let results = root.ai.internal._run_tool_calls(
                    calls.calls,
                    tools,
                    null,
                    null,
                    null,
                    [],
                    [],
                    null,
                    step,
                ) catch (e) {
                    _ => {
                        throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion tool callback failed")
                    },
                };
                let _ = adapter.submit(state, results) catch (e) {
                    _ => {
                        throw baml.errors.UnknownError.with_message<never>(e, "Claude Code completion could not submit BAML tool results")
                    },
                };
            },
            let value: T => {
                return root.ai.ResponseWithMetadata<T> {
                    value: value,
                    metadata: run.response.metadata,
                    conversation: state,
                };
            },
        }
    }

    throw baml.errors.UnknownError.with_message<never>({ "max_application_tool_steps": provider.max_application_tool_steps }, "Claude Code completion exhausted its application-tool step budget")
}

implements root.ai.Provider for ClaudeCodeCli {
  function name(self) -> string throws never { "claude-code-cli" }
  function render_shorthand(self) -> string throws never { "anthropic/" + self.model }
}

implements root.ai.GenerationProvider for ClaudeCodeCli {
  function generate<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.ResponseWithMetadata<T> throws baml.errors.UnknownError {
    self.complete_with_events<T>(task).response
  }
}

implements root.ai.CompletionProvider for ClaudeCodeCli {
  function complete<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.ResponseWithMetadata<T> throws baml.errors.UnknownError {
    //# Completion owns the strategy; direct callers never choose by capability
    if (task.tools.length() == 0) {
      return self.complete_with_events<T>(task.with_provider(self)).response;
    }
    _claude_code_complete_with_application_tools<T>(
      self,
      task.with_provider(self),
    )
  }
  function replay_policy<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.ReplayPolicy throws never {
    root.ai.ReplayPolicy {
        kind: root.ai.ReplayKind.Never,
        idempotency_key: null,
    }
  }
}

function _claude_code_harness_provider(
    provider: ClaudeCodeCli,
    session: ClaudeCodeHarnessSession,
) -> ClaudeCodeCli throws never {
    ClaudeCodeCli {
        executable: provider.executable,
        model: provider.model,
        cwd: session.options.cwd ?? provider.cwd,
        timeout_ms: provider.timeout_ms,
        max_turns: provider.max_turns,
        max_application_tool_steps: provider.max_application_tool_steps,
        max_budget_usd: provider.max_budget_usd,
        permission_mode: session.options.permission_mode ?? provider.permission_mode,
        tools: provider.tools,
        allowed_tools: provider.allowed_tools,
        safe_mode: if (session.options.sandbox != null) {
            true
        } else {
            provider.safe_mode
        },
        persist_session: provider.persist_session,
        harness_sessions: provider.harness_sessions,
    }
}

function _claude_code_harness_task<T>(
    provider: ClaudeCodeCli,
    session: ClaudeCodeHarnessSession,
    task: root.ai.Task<T>,
) -> root.ai.Task<T> throws never {
    let rebound = task.with_provider(provider);
    if (session.steering.length() == 0) {
        return rebound;
    }
    root.ai.Task<T> {
        ...rebound,
        prompt: baml.llm.assemble_prompt_ast(
            ["", "\nHarness steering (follow exactly):\n", ""],
            [
                root.ai.messages._prompt_text_content(rebound.prompt),
                session.steering.join("\n"),
            ],
        ),
    }
}

implements root.ai.harness.Harness for ClaudeCodeCli {
    function label(self) -> string throws never { "claude-code-cli" }

    function open(
        self,
        options: root.ai.harness.HarnessOptions,
    ) -> root.ai.harness.HarnessSession throws never {
        let session = ClaudeCodeHarnessSession {
            session_id: "claude-code-session-" + (self.harness_sessions.length() + 1).to_string(),
            options: options,
            history: root.ai.MessageHistory.empty(),
            is_stopped: false,
            steering: [],
        };
        self.harness_sessions.push(session);
        session
    }

    function run<T>(
        self,
        session: root.ai.harness.HarnessSession,
        task: root.ai.Task<T>,
        on_event: ((root.ai.observe.AgentEvent) -> null throws never)?,
    ) -> root.ai.harness.HarnessRun<T> throws baml.errors.UnknownError {
        let local = match (session) {
            let value: ClaudeCodeHarnessSession => value,
            _ => {
                throw baml.errors.UnknownError.with_message<never>(null, "Claude Code received a session owned by another harness")
            },
        };
        if (local.is_stopped) {
            throw baml.errors.UnknownError.with_message<never>({ "session_id": local.session_id }, "Claude Code harness session is stopped");
        }
        //# Apply per-session isolation and steering before launching Claude Code
        let configured = _claude_code_harness_provider(self, local);
        let effective_task = _claude_code_harness_task<T>(configured, local, task);
        //# Normalize Claude Code JSONL into the shared harness event callback
        let cli_run = configured.complete_with_events<T>(effective_task, on_event = on_event);
        if (cli_run.session_id.length() > 0) {
            local.session_id = cli_run.session_id;
        }
        local.history = effective_task.messages().to_history();
        root.ai.harness.HarnessRun<T> {
            value: cli_run.response.value,
            events: cli_run.events,
            token: self.save_session(local),
            history: local.history,
        }
    }

    function save_session(
        self,
        session: root.ai.harness.HarnessSession,
    ) -> root.ai.harness.HarnessSessionToken
        throws baml.errors.UnknownError {
        let local = match (session) {
            let value: ClaudeCodeHarnessSession => value,
            _ => {
                throw baml.errors.UnknownError.with_message<never>(null, "cannot save a session owned by another harness")
            },
        };
        root.ai.harness.HarnessSessionToken {
            provider: self.label(),
            runtime: "claude-code-cli",
            session_id: local.session_id,
            version: 1,
            sealed: baml.json.stringify({ "session_id": local.session_id }),
        }
    }

    function restore_session(
        self,
        token: root.ai.harness.HarnessSessionToken,
    ) -> root.ai.harness.HarnessSession
        throws baml.errors.UnknownError {
        if (
            token.provider != self.label()
                || token.runtime != "claude-code-cli"
                || token.version != 1
        ) {
            throw baml.errors.UnknownError.with_message<never>(token, "Claude Code harness token belongs to another runtime");
        }
        for (let session in self.harness_sessions) {
            if (session.session_id == token.session_id) {
                return session;
            }
        }
        throw baml.errors.UnknownError.with_message<never>(token, "Claude Code harness session was not found")
    }

    function steer(
        self,
        session: root.ai.harness.HarnessSession,
        instruction: string,
    ) -> null
        throws baml.errors.UnknownError {
        let local = match (session) {
            let value: ClaudeCodeHarnessSession => value,
            _ => {
                throw baml.errors.UnknownError.with_message<never>(null, "cannot steer a session owned by another harness")
            },
        };
        local.steering.push(instruction);
        null
    }

    function interrupt(self, session: root.ai.harness.HarnessSession) -> null
        throws baml.errors.UnknownError {
        let local = match (session) {
            let value: ClaudeCodeHarnessSession => value,
            _ => {
                throw baml.errors.UnknownError.with_message<never>(null, "cannot interrupt a session owned by another harness")
            },
        };
        local.is_stopped = true;
        null
    }
}
