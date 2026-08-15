// The Runner interface, the loop building blocks, and the default Agent
// runner. The runner owns the loop: tool execution, correlation, budgets,
// journal writes, and the typed parse of the final output.
// Mirrors _planv2/pages/02_guides/02_specs_and_runners/02_the_default_runner.md.

interface Runner<Out> {
    type Output
    type Error

    function run(self, spec: FunctionSpec<Out>) -> Self.Output throws Self.Error
}

class RunResult<Out> {
    value: Out,
    journal: Journal,
    usage: root.events.Usage,
}

// The default runner. A spec-mode LLM function's direct call runs
// `Agent<Out>.new(client = client).run(Fn@spec(...)).value`; construct it
// yourself to set the other fields (`$` call-site parameters are a future
// phase).
class Agent<Out> {
    max_steps: int,
    client: Client?,
    tool_errors: root.tools.ErrorMode,
    on_event: (root.events.Event) -> null throws never,

    function new(
        max_steps: int = 12,
        client: Client? = null,
        tool_errors: root.tools.ErrorMode = root.tools.ErrorMode.Report,
        on_event: ((root.events.Event) -> null throws never)? = null,
    ) -> Agent<Out> {
        Agent {
            max_steps: max_steps,
            client: client,
            tool_errors: tool_errors,
            on_event: on_event
                ?? (
                    (e: root.events.Event) -> {
                        null
                    }
                ),
        }
    }

    // Execute one call: validate, dispatch, append the correlated result.
    // A Raise-mode failure throws ToolFailedError after ToolFailed lands.
    function _run_one_tool(
        self,
        tb: root.tools.Toolbox,
        call: root.content.ToolUse,
        j: Journal,
    ) -> null {
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

    function _parses(self, candidate: string) -> bool {
        let _ = baml.sap.parse<Out>(candidate) catch_all (e) {
            _ => {
                return false;
            },
        };
        true
    }

    // One accepted model turn, with the within-turn repair loop: when a
    // Complete turn's candidate does not parse, commit the failed turn and a
    // correction request as ordinary events and invoke again, under a fixed
    // attempt budget. Everything that happened is on the record; a repair
    // attempt does not consume a step. Token usage of every attempt is
    // added to `total`.
    function _attempt_turn(
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
            || self._parses(candidate);
        if (accepted) {
            j.append_all(batch);
            // terminal wire outcomes are thrown here, with the turn on the record
            match (turn.stop_reason) {
                root.content.StopReason.MaxTokens => {
                    throw root.errors.ParseFailed { provider: c.id(), raw_output: candidate }
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
            self._attempt_turn(c, spec, j, total, attempts_left - 1)
        } else {
            j.append_all(batch);
            throw root.errors.ParseFailed { provider: c.id(), raw_output: candidate }
        }
    }

    // Emit any events appended since `seen` to on_event; returns the new count.
    function _emit_from(self, j: Journal, seen: int) -> int {
        let entries = j.entries();
        let i = seen;
        while (i < entries.length()) {
            if let e: root.events.Event = entries.at(i) {
                let _ = self.on_event(e);
            }
            i = i + 1;
        }
        entries.length()
    }

    implements Runner<Out> {
        type Output = RunResult<Out>
        type Error = root.errors.Failure | baml.errors.UnknownError | baml.reflect.errors.CompilationError

        function run(self, spec: FunctionSpec<Out>) -> RunResult<Out> {
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
                // repair re-asks happen inside the step: 1 turn + up to 1 re-asks
                let turn = self._attempt_turn(selected, spec, j, total, 2);
                steps = steps + 1;
                seen = self._emit_from(j, seen);
                let calls = turn.tool_uses();
                if (calls.length() > 0) {
                    let before = j.entries().length();
                    // execute concurrently; each call appends its correlated result
                    let futures = calls.map((call) -> {
                        spawn { self._run_one_tool(spec.tools(), call, j) }
                    });
                    let _ = await baml.future.all(futures);
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
                    let value = baml.sap.parse<Out>(candidate) catch_all (e) {
                        _ => {
                            throw root.errors.ParseFailed { provider: selected.id(), raw_output: candidate }
                        },
                    };
                    let value_json = baml.json.to_json(value) catch_all (e) {
                        _ => {
                            throw baml.errors.UnknownError { data: e, message: [e.to_string()] }
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
