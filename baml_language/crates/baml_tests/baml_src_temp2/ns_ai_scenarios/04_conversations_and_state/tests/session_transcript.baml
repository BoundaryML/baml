// D10: steering notes, media appends, the turn-structured transcript, and
// the display/telemetry events.

function _transcript_provider() -> ai.testing.FakeToolProvider throws never {
    ai.testing.FakeToolProvider {
        provider_name: "fake-transcript",
        tool_calls: [],
        provider_tools: [],
        final_output: `{"category":"billing","priority":"Urgent","summary":"s","reply":"r"}`,
    }
}

test "D10b: steer injects a system note that rides into the next turn" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_transcript_provider());
    let session = ai.run.AgentSession<Resolution>.start(task);
    let before = session.conversation().messages().items().length();
    let _ = session.steer("The user switched to the billing account; prior order IDs are stale.");
    let after = session.conversation().messages().items();
    assert.equal(after.length(), before + 1);
    assert.is_true(after[after.length() - 1].role() == ai.MessageRole.System);
    //# The note does not block the next turn
    match (session.send("Continue.")) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
}

test "D10a: a user message with an image part appends to the exact conversation" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_transcript_provider());
    let session = ai.run.AgentSession<Resolution>.start(task);
    let message = ai.ChatMessage {
        message_role: ai.MessageRole.User,
        content: [
            ai.messages.TextPart { value: "What is in this screenshot?", metadata: {} },
            ai.messages.ImagePart {
                value: image.from_url("https://example.com/screenshot.png", "image/png"),
                metadata: {},
            },
        ],
        metadata: null,
    };
    match (session.send(message)) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
}

test "D10d: turns() gives one Turn per committed continuation" {
    let task = ResolveTicket@task(sample_ticket()).with_provider(_transcript_provider());
    let session = ai.run.AgentSession<Resolution>.start(task);
    assert.equal(session.turns().length(), 0);
    match (session.send("First question.")) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
    match (session.send("Second question.")) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
    let turns = session.turns();
    assert.equal(turns.length(), 2);
    //# Each turn's slice contains its own user message
    assert.is_true(ai.message_text(turns[0].messages[0]).includes("First question"));
    assert.is_true(ai.message_text(turns[1].messages[0]).includes("Second question"));
    //# Per-turn accounting rode along
    assert.is_true(turns[0].usage.input_tokens >= 0);
    match (session.last_turn()) {
        let last: ai.run.Turn => assert.is_true(ai.message_text(last.messages[0]).includes("Second question")),
        null => assert.is_true(false),
    }
}

// A minimal from-scratch provider that exposes display channels, so the
// event tests exercise the exact path a real adapter uses.
class _TalkativeConversation {
    owner: _TalkativeProvider,
    history: ai.MessageHistory,
    fingerprint: string,

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never { self.owner }
        function messages(self) -> ai.Messages throws never { self.history }
        function output_type_fingerprint(self) -> string? throws never { self.fingerprint }
    }
}

class _TalkativeProvider {
    implements ai.Provider {
        function name(self) -> string throws never { "talkative" }
        function render_shorthand(self) -> string throws never { "talkative/fake-1" }
    }

    implements ai.AgentProvider {
        function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
            _TalkativeConversation {
                owner: self,
                history: task.messages().to_history(),
                fingerprint: ai.output_fingerprint<T>(),
            }
        }

        function step<T>(
            self,
            conversation: ai.Conversation,
            tools: ai.tools.Tool[],
        ) -> ai.ModelStep<T> throws baml.errors.UnknownError {
            let value = baml.sap.parse<T>(`{"category":"billing","priority":"Urgent","summary":"s","reply":"r"}`) catch (e) {
                _ => throw baml.errors.UnknownError.with_message<never>(e, "talkative parse"),
            };
            ai.ModelStep<T> {
                outcome: value,
                assistant_text: "Let me look into the duplicate charge.",
                reasoning_text: "The invoice IDs match, so this is likely a pending authorization.",
                metadata: ai.ResponseMetadata {
                    provider: "talkative",
                    model: "fake-1",
                    request_id: "talkative-1",
                    finish_reason: "stop",
                    usage: ai.Usage { input_tokens: 7, output_tokens: 3, cached_input_tokens: 0, reasoning_tokens: 0 },
                    attributes: {},
                    raw: null,
                },
            }
        }

        function submit(
            self,
            conversation: ai.Conversation,
            results: ai.tools.ToolResult[],
        ) -> ai.Conversation throws never {
            conversation
        }
    }
}

test "D10c/e: assistant text, reasoning, and step telemetry reach observers" {
    let kinds: string[] = [];
    let assistant_texts: string[] = [];
    let step_request_ids: string[] = [];
    let timestamped = 0;
    let outcome = ResolveTicket@task(sample_ticket())
        .with_provider(_TalkativeProvider {})
        .run(runner = ai.run.Agent<Resolution>.new(
            on_event = (event: ai.observe.AgentEvent) -> null {
                kinds.push(event.kind());
                match (event) {
                    let text: ai.observe.AssistantTextEvent => { assistant_texts.push(text.text); },
                    let step: ai.observe.StepFinishedEvent => {
                        step_request_ids.push(step.metadata.request_id ?? "");
                        match (step.usage_delta) {
                            let delta: ai.Usage => assert.equal(delta.input_tokens, 7),
                            null => assert.is_true(false),
                        }
                    },
                    _ => {},
                };
                match (event.emitted_at()) {
                    let at: baml.time.Instant => { timestamped = timestamped + 1; },
                    null => {},
                };
                null
            },
        ));
    match (outcome) {
        let done: ai.Done<Resolution> => {},
        _ => assert.is_true(false),
    }
    assert.is_true(kinds.includes("step_finished"));
    assert.is_true(kinds.includes("assistant_text"));
    assert.is_true(kinds.includes("reasoning"));
    assert.equal(assistant_texts[0], "Let me look into the duplicate charge.");
    assert.equal(step_request_ids[0], "talkative-1");
    //# The new events carry timestamps
    assert.is_true(timestamped >= 3)
}
