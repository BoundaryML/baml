// Deterministic coverage for ai.run.AgentSession: pairing, continuation
// intents, forking, value unwrapping, and save/restore.

function _AgentSessionFake(name: string, final_output: string) -> ai.testing.FakeToolProvider throws never {
    ai.testing.FakeToolProvider {
        provider_name: name,
        tool_calls: [],
        final_output: final_output,
        provider_tools: [],
    }
}

function _paused_openai_session() -> ai.run.AgentSession<Resolution> {
    //# A real provider type paused before its first request: exercises
    //# save/restore without any network traffic
    let provider = root.openai.OpenAIProvider {
        ...root.openai.responses(),
        model: "gpt-5.6-luna",
        api_key: "unused",
    };
    let task = ResolveTicketWithTools@task(sample_ticket())
        .with_provider(provider)
        .with_tools([search_knowledge]);
    let paused = task.run(
        runner = ai.run.Agent<Resolution>.new(max_steps = 0),
    );
    ai.run.AgentSession<Resolution>.of(task, paused)
}

testset "agent-sessions" {
    test "a session pairs any outcome with its task" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("session-pairing", `"ack"`));
        let outcome = task.run(runner = ai.run.Agent<string>.new());
        let session = ai.run.AgentSession<string>.of(task, outcome);
        assert.equal(session.conversation().provider().name(), "session-pairing");
        assert.equal(session.task().identity?.name, task.identity?.name)
    }

    test "send appends exactly one message and advances the session in place" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("session-send", `"ack"`));
        let base = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        let before = base.conversation().messages().items().length();

        let _ = base.send(ai.ChatMessage.user("next turn"));

        //# The same session now points at the advanced conversation
        assert.equal(base.conversation().messages().items().length(), before + 1);
        assert.equal(
            ai.message_text(base.conversation().messages().items()[before]),
            "next turn",
        )
    }

    test "a busy session refuses a second concurrent continuation" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("session-busy", `"ack"`));
        let base = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        base._busy = true;
        let caught = false;
        let _ = base.send(ai.ChatMessage.user("racing")) catch (e) {
            let busy: ai.run.SessionBusy => {
                caught = true;
                base.conversation().provider().name()
            },
            _ => "unexpected",
        };
        assert.is_true(caught);
        //# The rejected call did nothing: the session is still usable
        base._busy = false;
        let outcome = base.send(ai.ChatMessage.user("after release"));
        match (outcome) {
            let done: ai.Done<string> => assert.equal(done.value, "ack"),
            _ => assert.is_true(false),
        }
    }

    test "forked sessions continue independently from a shared prefix" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("session-fork", `"ack"`));
        let base = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        let before = base.conversation().messages().items().length();

        let branch_a = base.fork();
        let branch_b = base.fork();
        let _ = branch_a.send(ai.ChatMessage.user("branch a"));
        let _ = branch_b.send(ai.ChatMessage.user("branch b"));

        let last_a = branch_a.conversation().messages().items();
        let last_b = branch_b.conversation().messages().items();
        assert.equal(ai.message_text(last_a[last_a.length() - 1]), "branch a");
        assert.equal(ai.message_text(last_b[last_b.length() - 1]), "branch b");
        //# Neither branch advanced the session they forked from
        assert.equal(base.conversation().messages().items().length(), before)
    }

    test "complete returns the final typed value" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("session-ask", `"the answer"`));
        let base = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        assert.equal(base.complete(ai.ChatMessage.user("question")), "the answer")
    }

    test "complete surfaces a stop as IncompleteRun carrying the resumable outcome" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("session-stop", `"never"`));
        let base = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        let caught = false;
        let _ = base.complete(ai.ChatMessage.user("question"),
            runner = ai.run.Agent<string>.new(max_steps = 0),) catch (e) {
            let incomplete: ai.IncompleteRun => {
                caught = true;
                match (incomplete.outcome) {
                    let stopped: ai.Stopped => {
                        assert.equal(stopped.steps_taken, 0);
                        //# Lossless: the stop still carries its conversation
                        assert.equal(stopped.conversation.provider().name(), "session-stop");
                    },
                    _ => assert.is_true(false),
                };
                ""
            },
        };
        assert.is_true(caught)
    }

    test "submit_tool_results completes a handoff and continues the same turn" {
        let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
        let provider = fake_tool_provider(
            [
                ai.tools.ToolCall {
                    id: "session-handoff-1",
                    name: "lookup_account",
                    args: { "customer_id": "C-1" },
                },
            ],
        );
        let task = ResolveTicketWithTools@task(sample_ticket())
            .with_provider(provider)
            .with_tools([handoff_tool]);
        let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
        let session = ai.run.AgentSession<Resolution>.of(task, outcome);
        match (outcome) {
            let handoff: ai.Handoff => {
                let resumed = session.submit_tool_results(
                    [ai.tools.ToolOk.of(handoff.call, { "customer_id": "C-1", "status": "handled" })],
                    runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]),
                );
                match (resumed) {
                    let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
                    _ => assert.is_true(false),
                }
            },
            _ => assert.is_true(false),
        }
    }

    test "save produces a token naming the task and restore verifies it" {
        let session = _paused_openai_session();
        let token = session.save();
        assert.equal(token.task_identity, `${session.task().identity?.package ?? ""}.${session.task().identity?.name ?? ""}`);
        assert.equal(token.conversation.provider, "openai");

        let provider = root.openai.OpenAIProvider {
            ...root.openai.responses(),
            model: "gpt-5.6-luna",
            api_key: "unused",
        };
        let same_task = ResolveTicketWithTools@task(sample_ticket())
            .with_provider(provider)
            .with_tools([search_knowledge]);
        let restored = ai.run.AgentSession<Resolution>.restore(same_task, token);
        assert.equal(restored.conversation().provider().name(), "openai");
        assert.equal(
            restored.conversation().output_type_fingerprint(),
            session.conversation().output_type_fingerprint(),
        )
    }

    test "restore refuses a token saved under a different task" {
        let session = _paused_openai_session();
        let token = session.save();
        let other_provider = root.openai.OpenAIProvider {
            ...root.openai.responses(),
            model: "gpt-5.6-luna",
            api_key: "unused",
        };
        let wrong_task = _SessionProbe@task().with_provider(other_provider);
        let caught = false;
        let _ = ai.run.AgentSession<string>.restore(wrong_task, token) catch (e) {
            let mismatch: ai.run.SessionMismatch => {
                caught = true;
                assert.equal(mismatch.expected, token.task_identity);
                ai.run.AgentSession<string> { _task: wrong_task, _conversation: session.conversation(), _busy: false }
            },
        };
        assert.is_true(caught)
    }
}

// ---------------------------------------------------------------------------
// D11 transactional semantics: a minimal provider whose step can be told to
// fail, with everything else deterministic.
// ---------------------------------------------------------------------------

class _FlakyConversation {
    owner: _FlakyProvider,
    history: ai.MessageHistory,
    fingerprint: string?,

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never {
            self.owner
        }
        function messages(self) -> ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.fingerprint
        }
    }
}

class _FlakyProvider {
    fail_steps_remaining: int,
    steps_served: int,

    implements ai.Provider {
        function name(self) -> string throws never {
            "flaky"
        }
        function render_shorthand(self) -> string throws never {
            "baml-fallback/fake"
        }
    }

    implements ai.AgentProvider {
        function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
            _FlakyConversation {
                owner: self,
                history: ai.MessageHistory { entries: [ai.ChatMessage.user("prompt")] },
                fingerprint: ai.output_fingerprint<T>(),
            }
        }
        function step<T>(self, conversation: ai.Conversation, tools: ai.tools.Tool[]) -> ai.ModelStep<T>
            throws ai.Failure | baml.errors.UnknownError {
            if (self.fail_steps_remaining > 0) {
                self.fail_steps_remaining = self.fail_steps_remaining - 1;
                throw ai.RateLimited { provider: "flaky", retry_after_ms: 1 };
            }
            self.steps_served = self.steps_served + 1;
            let value = baml.json.from_string<T>(`"ok"`) catch (e) {
                _ => throw baml.errors.UnknownError.with_message<never>(e, "flaky final parse"),
            };
            ai.ModelStep<T> {
                outcome: value,
                metadata: ai.ResponseMetadata {
                    provider: "flaky",
                    model: "fake",
                    request_id: `flaky-${self.steps_served}`,
                    finish_reason: null,
                    usage: null,
                    attributes: {},
                },
            }
        }
        function submit(self, conversation: ai.Conversation, results: ai.tools.ToolResult[]) -> ai.Conversation
            throws never {
            conversation
        }
    }

    implements ai.ConversationAppendProvider {
        function append_messages(self, conversation: ai.Conversation, messages: ai.Messages) -> null
            throws ai.Failure {
            let state = match (conversation) {
                let value: _FlakyConversation => value,
                _ => throw ai.InvalidRequest {
                    provider: "flaky",
                    status_code: null,
                    detail: "wrong flaky conversation",
                },
            };
            for (let message in messages.items()) {
                state.history.entries.push(message);
            }
            null
        }
    }
}

testset "agent-session-transactions" {
    test "a failure after the append returns Failed and resume continues without double-append" {
        let provider = _FlakyProvider { fail_steps_remaining: 0, steps_served: 0 };
        let task = _SessionProbe@task().with_provider(provider);
        let session = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));

        //# Arm one step failure, then send: the append commits, the step fails
        provider.fail_steps_remaining = 1;
        let outcome = session.send(ai.ChatMessage.user("next question"));
        match (outcome) {
            let failed: ai.Failed => {
                match (failed.cause) {
                    let limited: ai.RateLimited => assert.equal(limited.provider, "flaky"),
                    _ => assert.is_true(false),
                }
            },
            _ => assert.is_true(false),
        }
        //# The session advanced to the committed state: message appended once
        let appended = session.conversation().messages().items().filter((m: ai.Message) -> bool {
            ai.message_text(m) == "next question"
        });
        assert.equal(appended.length(), 1);

        //# Resume retries the model step — no re-append, no lost state
        let resumed = session.resume();
        match (resumed) {
            let done: ai.Done<string> => assert.equal(done.value, "ok"),
            _ => assert.is_true(false),
        }
        let after = session.conversation().messages().items().filter((m: ai.Message) -> bool {
            ai.message_text(m) == "next question"
        });
        assert.equal(after.length(), 1)
    }

    test "a pre-progress failure still throws with the session unchanged" {
        let provider = _FlakyProvider { fail_steps_remaining: 1, steps_served: 0 };
        let task = _SessionProbe@task().with_provider(provider);
        //# Fresh run, first step fails: nothing committed, ordinary throw
        let caught = false;
        let _ = task.run(runner = ai.run.Agent<string>.new()) catch (e) {
            let limited: ai.RateLimited => {
                caught = true;
                null
            },
            _ => null,
        };
        assert.is_true(caught);
        assert.equal(provider.steps_served, 0)
    }

    test "complete over a failed continuation throws the cause and stays resumable" {
        let provider = _FlakyProvider { fail_steps_remaining: 0, steps_served: 0 };
        let task = _SessionProbe@task().with_provider(provider);
        let session = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        provider.fail_steps_remaining = 1;
        let caught = false;
        let _ = session.complete(ai.ChatMessage.user("demanding")) catch (e) {
            let limited: ai.RateLimited => {
                caught = true;
                ""
            },
            _ => "",
        };
        assert.is_true(caught);
        //# The session sits at the committed checkpoint; resume finishes
        match (session.resume()) {
            let done: ai.Done<string> => assert.equal(done.value, "ok"),
            _ => assert.is_true(false),
        }
    }
}

testset "agent-session-guards" {
    test "phase reports pending handoff calls without retaining the outcome" {
        let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
        let provider = fake_tool_provider(
            [ai.tools.ToolCall { id: "phase-1", name: "lookup_account", args: { "customer_id": "C-1" } }],
        );
        let task = ResolveTicketWithTools@task(sample_ticket())
            .with_provider(provider)
            .with_tools([handoff_tool]);
        let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
        let session = ai.run.AgentSession<Resolution>.of(task, outcome);
        match (session.phase()) {
            let waiting: ai.run.AwaitingToolResults => {
                assert.equal(waiting.calls.length(), 1);
                assert.equal(waiting.calls[0].id, "phase-1")
            },
            _ => assert.is_true(false),
        }
    }

    test "send on an unanswered handoff is refused with a teaching error" {
        let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
        let provider = fake_tool_provider(
            [ai.tools.ToolCall { id: "guard-1", name: "lookup_account", args: { "customer_id": "C-1" } }],
        );
        let task = ResolveTicketWithTools@task(sample_ticket())
            .with_provider(provider)
            .with_tools([handoff_tool]);
        let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
        let session = ai.run.AgentSession<Resolution>.of(task, outcome);
        let refused = false;
        let _ = session.send(ai.ChatMessage.user("ignore the handoff")) catch (e) {
            let invalid: ai.InvalidRequest => {
                refused = invalid.detail.includes("submit_tool_results");
                ai.Interrupted {
                    conversation: session.conversation(),
                    steps_taken: 0,
                    usage: ai.Usage { input_tokens: 0, output_tokens: 0, cached_input_tokens: 0, reasoning_tokens: 0 },
                    reason: "unused",
                }
            },
            _ => baml.sys.panic("expected InvalidRequest"),
        };
        assert.is_true(refused);
        //# The rejected send changed nothing: answering still works
        match (session.phase()) {
            let waiting: ai.run.AwaitingToolResults => assert.equal(waiting.calls[0].id, "guard-1"),
            _ => assert.is_true(false),
        }
    }

    test "submit_tool_results demands exact correlation" {
        let handoff_tool = ai.tools.tool(lookup_account).as_handoff();
        let provider = fake_tool_provider(
            [ai.tools.ToolCall { id: "corr-1", name: "lookup_account", args: { "customer_id": "C-1" } }],
        );
        let task = ResolveTicketWithTools@task(sample_ticket())
            .with_provider(provider)
            .with_tools([handoff_tool]);
        let outcome = task.run(runner = ai.run.Agent<Resolution>.new(tools = [handoff_tool]));
        let session = ai.run.AgentSession<Resolution>.of(task, outcome);
        let refused = false;
        let _ = session.submit_tool_results(
            [ai.tools.ToolOk.of(ai.tools.ToolCall { id: "wrong-id", name: "lookup_account", args: {} }, { "ok": true })],
        ) catch (e) {
            let invalid: ai.InvalidRequest => {
                refused = invalid.detail.includes("exactly once");
                ai.Interrupted {
                    conversation: session.conversation(),
                    steps_taken: 0,
                    usage: ai.Usage { input_tokens: 0, output_tokens: 0, cached_input_tokens: 0, reasoning_tokens: 0 },
                    reason: "unused",
                }
            },
            _ => baml.sys.panic("expected InvalidRequest"),
        };
        assert.is_true(refused)
    }

    test "submit_tool_results with nothing pending points back at send" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("guard-none", `"ack"`));
        let session = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        let refused = false;
        let _ = session.submit_tool_results(
            [ai.tools.ToolOk.of(ai.tools.ToolCall { id: "x", name: "y", args: {} }, { "ok": true })],
        ) catch (e) {
            let invalid: ai.InvalidRequest => {
                refused = invalid.detail.includes("did you mean send");
                ai.Interrupted {
                    conversation: session.conversation(),
                    steps_taken: 0,
                    usage: ai.Usage { input_tokens: 0, output_tokens: 0, cached_input_tokens: 0, reasoning_tokens: 0 },
                    reason: "unused",
                }
            },
            _ => baml.sys.panic("expected InvalidRequest"),
        };
        assert.is_true(refused)
    }

    test "a batch send appends every message in one turn" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("batch-send", `"ack"`));
        let session = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        let before = session.conversation().messages().items().length();
        let outcome = session.send([
            ai.ChatMessage.user("first queued"),
            ai.ChatMessage.user("second queued"),
        ]);
        match (outcome) {
            let done: ai.Done<string> => assert.equal(done.value, "ack"),
            _ => assert.is_true(false),
        }
        assert.equal(session.conversation().messages().items().length(), before + 2)
    }

    test "export and import cross the portable pole visibly" {
        let task = _SessionProbe@task().with_provider(_AgentSessionFake("export-side", `"ack"`));
        let session = ai.run.AgentSession<string>.of(task, task.run(runner = ai.run.Agent<string>.new()));
        let _ = session.send(ai.ChatMessage.user("remember this"));
        let portable = session.export();

        let import_task = _SessionProbe@task().with_provider(_AgentSessionFake("import-side", `"ack"`));
        let imported = ai.run.AgentSession<string>.from(import_task, portable);
        assert.equal(imported.conversation().provider().name(), "import-side");
        let texts = imported.conversation().messages().items().map((m: ai.Message) -> string {
            ai.message_text(m)
        });
        assert.is_true(texts.includes("remember this"))
    }
}

testset "agent-session-contract" {
    test "restore refuses a structurally different task with the same name" {
        let session = _paused_openai_session();
        let token = session.save();
        //# Same function identity, different tool roster: not the same contract
        let provider = root.openai.OpenAIProvider {
            ...root.openai.responses(),
            model: "gpt-5.6-luna",
            api_key: "unused",
        };
        let different_tools = ResolveTicketWithTools@task(sample_ticket())
            .with_provider(provider)
            .with_tools([lookup_account]);
        let caught = false;
        let _ = ai.run.AgentSession<Resolution>.restore(different_tools, token) catch (e) {
            let mismatch: ai.run.SessionMismatch => {
                caught = true;
                assert.contains(mismatch.detail, "structurally different");
                ai.run.AgentSession<Resolution> { _task: different_tools, _conversation: session.conversation(), _busy: false }
            },
        };
        assert.is_true(caught)
    }

    test "contract fingerprints are stable across equivalent constructions" {
        let a = _paused_openai_session().task().contract_fingerprint();
        let b = _paused_openai_session().task().contract_fingerprint();
        assert.equal(a, b);
        assert.contains(a, "baml-contract/v1;")
    }
}
