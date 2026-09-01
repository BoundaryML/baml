// The Runner interface, the loop building blocks, and the default Agent
// runner. The runner owns the loop: tool execution, correlation, budgets,
// journal writes, and the typed parse of the final output.
// Mirrors _planv2/pages/02_guides/02_specs_and_runners/02_the_default_runner.md.

interface Runner {
    /// The error surface of this runner (`never` if the runner cannot throw).
    type Error = never

    /// Execute one spec to completion. `Out` lives on the method and is
    /// inferred from the spec argument, so no call site needs to name it.
    function run<Out>(self, spec: FunctionSpec<Out>) -> RunResult<Out> throws Self.Error
}

class RunResult<Out> {
    value: Out,
    journal: Journal,
    usage: root.events.Usage,
}

/// The default runner. A spec-mode LLM function's direct call runs
/// `Agent.new(client = client).run(Fn@spec(...)).value`; construct it
/// yourself to set the other fields (`$` call-site parameters are a future
/// phase).
///
/// The class itself is not generic: none of its configuration involves the
/// output type. `Out` lives on `run`, where it is inferred from the spec
/// argument, so no call site needs to name it.
class Agent {
    max_steps: int,
    // Total model invocations allowed for one schema candidate. The default
    // preserves the existing initial attempt plus one correction re-ask.
    schema_attempts: int,
    client: Client?,
    tool_errors: root.tools.ErrorMode,
    on_event: ((root.events.Event) -> void throws never)?,

    function new(
        max_steps: int = 12,
        schema_attempts: int = 2,
        client: Client? = null,
        tool_errors: root.tools.ErrorMode = root.tools.ErrorMode.Report,
        on_event: ((root.events.Event) -> void)? = null,
    ) -> Agent {
        if (schema_attempts < 1) {
            throw baml.errors.InvalidArgument {
                message: "Agent schema_attempts must be at least 1",
            }
        }
        Agent {
            max_steps: max_steps,
            schema_attempts: schema_attempts,
            client: client,
            tool_errors: tool_errors,
            on_event: root.events.guard(on_event),
        }
    }

    function _validated_schema_attempts(self) -> int {
        if (self.schema_attempts < 1) {
            throw baml.errors.InvalidArgument {
                message: "Agent schema_attempts must be at least 1",
            }
        }
        self.schema_attempts
    }

    /// (internal) Execute one call: validate, dispatch, append the correlated result.
    /// A Raise-mode failure throws ToolFailedError after ToolFailed lands.
    function _run_one_tool(
        self,
        tb: root.tools.Toolbox,
        call: root.content.ToolUse,
        j: Journal,
    ) -> void {
        match (tb.get(call.name)) {
            let t: root.tools.Tool => {
                let outcome: string? = t.call(call.args) catch_all (e) {
                    _ => {
                        j.append_all(
                            [root.events.ToolFailed { id: call.id, message: e.to_string() }],
                        );
                        // the per-tool setting wins; null inherits the run's mode
                        if ((t.on_error ?? self.tool_errors) == root.tools.ErrorMode.Raise) {
                            let classified = match (e) {
                                let f: root.errors.Failure => f,
                                _ => null,
                            };
                            throw root.errors.ToolFailedError {
                                id: call.id,
                                name: call.name,
                                message: e.to_string(),
                                cause: classified,
                            }
                        } else {
                            null
                        }
                    },
                };
                if let output: string = outcome {
                    j.append_all([root.events.ToolCompleted { id: call.id, output: output }]);
                }
                null
            },
            null => {
                j.append_all(
                    [
                        root.events.ToolFailed {
                            id: call.id,
                            message: `tool is not in the active toolbox: ${call.name}`,
                        },
                    ],
                );
                null
            },
        }
    }

    /// (internal) Does this turn carry content a media-shaped `Out` can be built from?
    /// Mirrors the `Ok(None)` condition of sys_llm's
    /// `parse_llm_output_for_target`: nothing to build from means fall back to
    /// the ordinary text parse. Always false when `Out` holds no media.
    function _has_media_output<Out>(self, turn: ModelTurn) -> bool {
        let shape = root.internal._media_shape(reflect.Type.of<Out>());
        let form = shape.split(":").at(0) ?? "";
        if (form == "") {
            return false;
        }
        if (form == "mixed" || form == "mixedlist") {
            return root.internal._media_mixed_parts(turn).length() > 0;
        }
        root.internal._media_of_kind(turn, shape.split(":").at(1) ?? "").length() > 0
    }

    /// (internal) Build a media-shaped `Out` from the turn's content blocks. Only called
    /// when `_has_media_output` holds; the shape-specific errors below are the
    /// ported sys_llm messages, thrown after the turn is already on the record.
    function _media_value<Out>(self, turn: ModelTurn, provider: string) -> Out {
        let t = reflect.Type.of<Out>();
        let shape = root.internal._media_shape(t);
        let form = shape.split(":").at(0) ?? "";
        let kind = shape.split(":").at(1) ?? "";
        let payload = match (form) {
            "mixedlist" => {
                let items: baml.json.json = root.internal._media_mixed_parts(turn);
                items
            },
            "mixed" => {
                let items = root.internal._media_mixed_parts(turn);
                if (items.length() == 1) {
                    let only: baml.json.json = null;
                    for (let i in items) {
                        only = i;
                    }
                    only
                } else if (root.internal._media_all_text(items)) {
                    // several plain text parts collapse into one string
                    root.internal._media_join_text(items)
                } else {
                    throw root.errors.ParseFailed {
                        provider: provider,
                        raw_output: `Expected one text or image output for ${t.to_string()}, got ${items.length()} parts. Use (image | string)[] to preserve mixed outputs.`,
                    }
                }
            },
            "list" => {
                root.internal._media_reject_mixed(turn, kind, provider);
                let items: baml.json.json = root.internal._media_of_kind(turn, kind);
                items
            },
            _ => {
                let parts = root.internal._media_of_kind(turn, kind);
                root.internal._media_reject_mixed(turn, kind, provider);
                if (parts.length() != 1) {
                    throw root.errors.ParseFailed {
                        provider: provider,
                        raw_output: if (form == "one") {
                            `Expected exactly one ${kind} output, got ${parts.length()}. Use ${kind}[] for multiple outputs.`
                        } else {
                            `Expected zero or one ${kind} output for ${t.to_string()}, got ${parts.length()}. Use ${kind}[] for multiple outputs.`
                        },
                    }
                }
                let only: baml.json.json = null;
                for (let p in parts) {
                    only = p;
                }
                only
            },
        };
        baml.json.from_json<Out>(payload) catch_all (e) {
            _ => {
                throw root.errors.ParseFailed {
                    provider: provider,
                    raw_output: `the ${kind} output did not fit ${t.to_string()}: ${e.to_string()}`,
                }
            },
        }
    }

    function _parses<Out>(self, turn: ModelTurn, candidate: string) -> bool {
        // A media-shaped `Out` is satisfied by the turn's blocks. When the turn
        // carries NONE, this falls through to the ordinary text parse rather
        // than declaring failure — that is sys_llm's `Ok(None)` -> SAP path
        // (`lib.rs:594-645`), and SAP is what turns a media-less answer into
        // `null` for `image?` and `[]` for `image[]`. Short-circuiting to
        // `false` here made those shapes unrepresentable: the runner re-asked
        // and then raised ParseFailed on a perfectly valid absence. Only a
        // required `image` genuinely fails the text parse.
        if (self._has_media_output<Out>(turn)) {
            return true;
        }
        let _ = baml.sap.parse<Out>(candidate) catch_all (e) {
            _ => {
                return false;
            },
        };
        true
    }

    /// (internal) One accepted model turn, with the within-turn repair loop: when a
    /// Complete turn's candidate does not parse, commit the failed turn and a
    /// correction request as ordinary events and invoke again, under a fixed
    /// attempt budget. Everything that happened is on the record; a repair
    /// attempt does not consume a step. Token usage of every attempt is
    /// added to `total`.
    function _attempt_turn<Out>(
        self,
        c: Client,
        spec: FunctionSpec<Out>,
        j: Journal,
        total: root.events.Usage,
        attempts_left: int,
    ) -> ModelTurn {
        let turn = c.invoke(
            ModelTurnInput {
                prompt: spec.prompt_template,
                journal: j,
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            },
        );
        if let u: root.events.Usage = turn.usage {
            total.input_tokens += u.input_tokens;
            total.output_tokens += u.output_tokens;
            if let ci: int = u.cached_input_tokens {
                total.cached_input_tokens = (total.cached_input_tokens ?? 0) + ci;
            }
            if let rt: int = u.reasoning_tokens {
                total.reasoning_tokens = (total.reasoning_tokens ?? 0) + rt;
            }
        }
        // every attempt commits: the journal is the complete record
        let batch: root.events.Event[] = [root.events.AssistantMessage { content: turn.content, client_id: c.id() }];
        // wire records ride the journal: failed legs first, selected last
        for (let call in turn.calls) {
            batch.push(call);
        }
        for (let u in turn.tool_uses()) {
            batch.push(root.events.ToolRequested { id: u.id, name: u.name, args: u.args });
        }
        if let u: root.events.Usage = turn.usage {
            batch.push(u);
        }
        let candidate = turn.terminal_text() ?? "";
        let accepted = turn.tool_uses().length() > 0
            || turn.stop_reason == root.content.StopReason.MaxTokens
            || turn.stop_reason == root.content.StopReason.Refused
            || self._parses<Out>(turn, candidate);
        if (accepted) {
            j.append_all(batch);
            // terminal wire outcomes are thrown here, with the turn on the record
            match (turn.stop_reason) {
                root.content.StopReason.MaxTokens => {
                    throw root.errors.FinishReasonError {
                        provider: c.id(),
                        finish_reason: turn.stop_reason,
                        raw_output: candidate,
                    }
                },
                root.content.StopReason.Refused => {
                    throw root.errors.Refused { provider: c.id(), reason: "the model declined to answer", raw_body: null }
                },
                _ => null,
            };
            turn
        } else if (attempts_left > 1) {
            batch.push(
                root.events.UserMessage {
                    content: "The reply did not match the required schema. Answer again with only the corrected value.",
                },
            );
            j.append_all(batch);
            self._attempt_turn<Out>(c, spec, j, total, attempts_left - 1)
        } else {
            j.append_all(batch);
            throw root.errors.ParseFailed { provider: c.id(), raw_output: candidate }
        }
    }

    /// (internal) Emit any events appended since `seen` to on_event; returns the new count.
    function _emit_from(self, j: Journal, seen: int) -> int {
        let entries = j.entries();
        if let cb: (root.events.Event) -> void throws never = self.on_event {
            let i = seen;
            while (i < entries.length()) {
                if let e: root.events.Event = entries.at(i) {
                    cb(e);
                }
                i = i + 1;
            }
        }
        entries.length()
    }

    implements Runner {
        // Cancellation is absent for the same reason it is absent from
        // `ai.errors.InvokeError`: a panic unwinds past the error channel
        // rather than travelling in it.
        type Error = baml.errors.InvalidArgument
            | baml.errors.Timeout
            | baml.errors.UnknownError
            | reflect.errors.CompilationError
            | root.errors.Failure

        function run<Out>(self, spec: FunctionSpec<Out>) -> RunResult<Out> {
            let schema_attempts = self._validated_schema_attempts();
            let selected = self.client ?? spec.default_client;
            let j = Journal.new(spec);
            let seen = self._emit_from(j, 0);
            let steps = 0;
            let total = root.events.Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: null,
                reasoning_tokens: null,
            };
            // the futility breaker: identical repeated failures stop the run
            let fail_counts: map<string, int> = {};
            while (steps < self.max_steps) {
                // Repair re-asks happen inside the step and share this total
                // schema-attempt budget.
                let turn = self._attempt_turn<Out>(selected, spec, j, total, schema_attempts);
                steps = steps + 1;
                seen = self._emit_from(j, seen);
                let calls = turn.tool_uses();
                if (calls.length() > 0) {
                    let before = j.entries().length();
                    // execute concurrently; each call appends its correlated result
                    let futures = calls.map((call) -> {
                        spawn { self._run_one_tool(spec.tools(), call, j) }
                    });
                    let _ = await baml.future.all(futures) catch_all (e) {
                        _ => {
                            // A Raise-mode tool failure: its ToolFailed event is
                            // already on the record; deliver it before the error
                            // propagates so listeners never miss journal events.
                            seen = self._emit_from(j, seen);
                            throw e
                        },
                    };
                    seen = self._emit_from(j, seen);
                    // Reporting a failure lets the model adapt; repeating the
                    // IDENTICAL failing call is futile, and each step can be
                    // expensive. Three identical failures end the run.
                    let entries = j.entries();
                    let i = before;
                    while (i < entries.length()) {
                        if let f: root.events.ToolFailed = entries.at(i) {
                            let failed_call = calls.find((one: root.content.ToolUse) -> {
                                one.id == f.id
                            });
                            if let known: root.content.ToolUse = failed_call {
                                let key = `${known.name}|${f.message}`;
                                let n = (fail_counts.get(key) ?? 0) + 1;
                                let _ = fail_counts.set(key, n);
                                if (n >= 3) {
                                    throw root.errors.ToolFailedError {
                                            id: known.id,
                                            name: known.name,
                                            message: `the same tool call failed ${n} times; stopping the run: ${f.message}`,
                                            cause: null,
                                        }
                                }
                            }
                        }
                        i = i + 1;
                    }
                } else {
                    let candidate = turn.terminal_text() ?? "";
                    // A media-shaped Out comes from the turn's media blocks;
                    // everything else, including a media Out the turn produced
                    // no media for, goes through the ordinary text parse. That
                    // second branch is REACHABLE for a media Out and is how
                    // absence is expressed: SAP answers `null` for `image?` /
                    // `image[]?` and `[]` for `image[]`, and only a required
                    // `image` fails there (`_parses` agrees, by construction).
                    let value = if (self._has_media_output<Out>(turn)) {
                        self._media_value<Out>(turn, selected.id())
                    } else {
                        baml.sap.parse<Out>(candidate) catch_all (e) {
                            _ => {
                                throw root.errors.ParseFailed { provider: selected.id(), raw_output: candidate }
                            },
                        }
                    };
                    let value_json = baml.json.to_json(value) catch_all (e) {
                        _ => {
                            throw baml.errors.UnknownError.with_message<never>(
                                    e,
                                    "final value serialization failed",
                                )
                        },
                    };
                    j.append_all(
                        [root.events.FinalProduced { value_json: baml.json.stringify(value_json) }],
                    );
                    seen = self._emit_from(j, seen);
                    return RunResult { value: value, journal: j, usage: total };
                }
            }
            throw root.errors.StepBudgetExceeded { steps: steps }
        }
    }
}
