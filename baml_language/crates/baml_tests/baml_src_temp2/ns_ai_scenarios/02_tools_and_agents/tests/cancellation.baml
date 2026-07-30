class AgentCancellationObserver {
    kinds: string[],
    interrupted_reasons: string[],
    finished_outcomes: string[],

    implements ai.observe.AgentObserver {
        function on_event(
            self,
            event: ai.observe.AgentEvent,
        ) -> null throws never {
            self.kinds.push(event.kind());
            match (event) {
                let interrupted: ai.observe.RunInterruptedEvent => {
                    self.interrupted_reasons.push(interrupted.reason);
                },
                let finished: ai.observe.RunFinishedEvent => {
                    self.finished_outcomes.push(finished.outcome);
                },
                _ => {},
            }
            null
        }
    }
}

function _agent_cancellation_observer() -> AgentCancellationObserver throws never {
    AgentCancellationObserver {
        kinds: [],
        interrupted_reasons: [],
        finished_outcomes: [],
    }
}

class AgentCancellationProbe {
    cancel: baml.spawn.CancelToken,
    effects: string[],

    function invoke(
        self,
        label: string,
    ) -> json throws never {
        self.effects.push(label);
        let _ = self.cancel.cancel();
        { "label": label }
    }
}

function _agent_cancellation_final_output() -> string throws never {
    `
        {
          "category": "billing",
          "priority": "Urgent",
          "summary": "Resumed without replay",
          "reply": "The committed tool results were retained."
        }
    `
}

test "guide 02: a pre-cancelled Agent returns a resumable zero-step checkpoint" {
    let cancel = baml.spawn.CancelToken.new();
    let _ = cancel.cancel();
    let observer = _agent_cancellation_observer();
    let provider = ai.testing.ScriptedToolProvider {
        turns: [],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(cancel = cancel, observers = [observer]),
        );

    match (outcome) {
        let interrupted: ai.Interrupted => {
            assert.equal(interrupted.steps_taken, 0);
            assert.equal(interrupted.reason, "cancelled");
            match (interrupted.conversation) {
                let state: ai.testing.ScriptedToolConversation => {
                    assert.equal(state.turn, 0);
                    assert.equal(state.submitted_results.length(), 0)
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    };
    assert.equal(observer.kinds, ["run_interrupted", "run_finished"]);
    assert.equal(observer.interrupted_reasons, ["cancelled"]);
    assert.equal(observer.finished_outcomes, ["interrupted"])
}

test "guide 02: cancellation during step preparation prevents the provider request" {
    let cancel = baml.spawn.CancelToken.new();
    let observer = _agent_cancellation_observer();
    let provider = ai.testing.ScriptedToolProvider {
        turns: [],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(
                    cancel = cancel,
                    prepare_step = (context) -> {
                        let _ = cancel.cancel();
                        ai.tools.StepPlan {
                            provider: null,
                            tools: null,
                            stop: null,
                        }
                    },
                    observers = [observer],
                ),
        );

    match (outcome) {
        let interrupted: ai.Interrupted => {
            assert.equal(interrupted.steps_taken, 0);
            match (interrupted.conversation) {
                let state: ai.testing.ScriptedToolConversation => {
                    assert.equal(state.turn, 0)
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    };
    assert.is_true(!observer.kinds.includes("model_started"));
    assert.equal(observer.finished_outcomes, ["interrupted"])
}

test "guide 02: cancellation during a tool effect commits results before interruption" {
    let cancel = baml.spawn.CancelToken.new();
    let probe = AgentCancellationProbe { cancel: cancel, effects: [] };
    let tool = ai
        .tools
        .tool(
            probe.invoke,
            name = "cancel_after_effect",
            description = "Record one effect and request cooperative cancellation.",
        );
    let provider = ai.testing.ScriptedToolProvider {
        turns: [
            ai.tools.ToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "cancel-one",
                        name: "cancel_after_effect",
                        args: { "label": "one" },
                    },
                ],
            },
        ],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([tool]);
    let observer = _agent_cancellation_observer();
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(tools = [tool], cancel = cancel, observers = [observer]),
        );

    let checkpoint = match (outcome) {
        let interrupted: ai.Interrupted => {
            assert.equal(interrupted.steps_taken, 1);
            match (interrupted.conversation) {
                let state: ai.testing.ScriptedToolConversation => {
                    assert.equal(state.turn, 1);
                    assert.equal(state.submitted_results.length(), 1);
                    assert.equal(state.submitted_results[0].length(), 1);
                    assert.equal(state.submitted_results[0][0].id, "cancel-one")
                },
                _ => assert.is_true(false),
            };
            interrupted.conversation
        },
        _ => baml.sys.panic("expected an Interrupted outcome"),
    };
    assert.equal(probe.effects, ["one"]);
    assert.contains(observer.kinds.join(","), "tool_call_finished");
    assert.equal(observer.finished_outcomes, ["interrupted"]);

    // A fresh Agent resumes the committed provider conversation. The tool is
    // not replayed because submit completed before Interrupted was returned.
    let resumed = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(tools = [tool], conversation = checkpoint),
        );
    match (resumed) {
        let done: ai.Done<Resolution> => {
            assert.equal(done.value.summary, "Resumed without replay")
        },
        _ => assert.is_true(false),
    };
    assert.equal(probe.effects, ["one"])
}

test "guide 02: cooperative cancellation drains a parallel tool batch" {
    let cancel = baml.spawn.CancelToken.new();
    let probe = AgentCancellationProbe { cancel: cancel, effects: [] };
    let tool = ai
        .tools
        .tool(
            probe.invoke,
            name = "parallel_cancel_probe",
            description = "Record an effect and request cooperative cancellation.",
        );
    let provider = ai.testing.ScriptedToolProvider {
        turns: [
            ai.tools.ToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "parallel-one",
                        name: "parallel_cancel_probe",
                        args: { "label": "one" },
                    },
                    ai.tools.ToolCall {
                        id: "parallel-two",
                        name: "parallel_cancel_probe",
                        args: { "label": "two" },
                    },
                ],
            },
        ],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([tool]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(tools = [tool], cancel = cancel),
        );

    match (outcome) {
        let interrupted: ai.Interrupted => {
            assert.equal(interrupted.steps_taken, 1);
            match (interrupted.conversation) {
                let state: ai.testing.ScriptedToolConversation => {
                    assert.equal(state.submitted_results.length(), 1);
                    assert.equal(state.submitted_results[0].length(), 2)
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    };
    assert.equal(probe.effects.length(), 2);
    assert.is_true(probe.effects.includes("one"));
    assert.is_true(probe.effects.includes("two"))
}

test "guide 02: a final value wins a cancellation race" {
    let cancel = baml.spawn.CancelToken.new();
    let observer = _agent_cancellation_observer();
    let provider = ai.testing.ScriptedToolProvider {
        turns: [],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(
                    cancel = cancel,
                    on_event = (event) -> {
                        if (event is ai.observe.ModelStartedEvent) {
                            let _ = cancel.cancel();
                        };
                        null
                    },
                    observers = [observer],
                ),
        );

    match (outcome) {
        let done: ai.Done<Resolution> => {
            assert.equal(done.value.summary, "Resumed without replay")
        },
        _ => assert.is_true(false),
    };
    assert.is_true(cancel.is_cancelled());
    assert.is_true(!observer.kinds.includes("run_interrupted"));
    assert.equal(observer.finished_outcomes, ["done"])
}

test "guide 02: a handoff wins a cancellation race" {
    let cancel = baml.spawn.CancelToken.new();
    let observer = _agent_cancellation_observer();
    let handoff_tool = ai.tools.tool(search_knowledge).as_handoff();
    let provider = ai.testing.ScriptedToolProvider {
        turns: [
            ai.tools.ToolCalls {
                calls: [
                    ai.tools.ToolCall {
                        id: "handoff-cancel-race",
                        name: "search_knowledge",
                        args: { "query": "duplicate charge" },
                    },
                ],
            },
        ],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([handoff_tool]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(
                    tools = [handoff_tool],
                    cancel = cancel,
                    on_event = (event) -> {
                        if (event is ai.observe.ModelStartedEvent) {
                            let _ = cancel.cancel();
                        };
                        null
                    },
                    observers = [observer],
                ),
        );

    match (outcome) {
        let handoff: ai.Handoff => {
            assert.equal(handoff.call.id, "handoff-cancel-race");
            assert.equal(handoff.steps_taken, 1)
        },
        _ => assert.is_true(false),
    };
    assert.is_true(cancel.is_cancelled());
    assert.is_true(!observer.kinds.includes("run_interrupted"));
    assert.equal(observer.finished_outcomes, ["handoff"])
}

test "guide 02: an unfired cooperative token does not change normal execution" {
    let cancel = baml.spawn.CancelToken.new();
    let observer = _agent_cancellation_observer();
    let provider = ai.testing.ScriptedToolProvider {
        turns: [],
        final_output: _agent_cancellation_final_output(),
        usage_per_step: null,
    };
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([]);
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Resolution>
                .new(cancel = cancel, observers = [observer]),
        );

    match (outcome) {
        let done: ai.Done<Resolution> => {
            assert.equal(done.value.summary, "Resumed without replay")
        },
        _ => assert.is_true(false),
    };
    assert.is_true(!observer.kinds.includes("run_interrupted"));
    assert.equal(observer.finished_outcomes, ["done"])
}
