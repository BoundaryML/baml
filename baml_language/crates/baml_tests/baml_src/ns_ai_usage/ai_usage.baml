// Usage on model calls: each `ModelCall` carries the tokens it reported, and
// a turn's or run's usage is its calls' usage summed per model. Offline: every
// client here is scripted.

function UsageSpec() -> int {
    client: "openai/gpt-4o-mini"
    prompt: `Return a number. ${ctx.output_format()}`
}

function usage_input() -> ai.ModelTurnInput {
    let spec = UsageSpec@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function tokens(input: int, output: int) -> ai.events.Usage {
    ai.events.Usage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: null,
        reasoning_tokens: null,
    }
}

/// A count, `-` when it was not reported.
function count(n: int?) -> string {
    match (n) {
        let reported: int => reported.to_string(),
        null => "-",
    }
}

/// `provider/model=input/output` for each entry, in insertion order.
function usage_label(usage: ai.events.ModelUsage[]) -> string {
    usage
        .map((entry: ai.events.ModelUsage) -> string {
            let u = entry.usage;
            `${entry.resolved_model.provider}/${entry.resolved_model.model ?? "-"}=${count(u.input_tokens)}/${count(u.output_tokens)}`
        })
        .join(",")
}

/// `provider/model=input/output` of a call, `none` for a call with no usage.
function call_label(call: ai.events.ModelCall) -> string {
    let tokens = match (call.usage) {
        let u: ai.events.Usage => `${count(u.input_tokens)}/${count(u.output_tokens)}`,
        null => "none",
    };
    `${call.resolved_model.provider}/${call.resolved_model.model ?? "-"}=${tokens}`
}

/// A turn's total, its own calls, then its child turns, each labeled the same
/// way; a failed attempt is marked `failed:`.
function turn_label(turn: ai.ModelTurn) -> string {
    let calls = turn.calls.map(call_label).join(",");
    let failed = if (turn.error == null) { "" } else { "failed: " };
    let label = `${failed}total[${usage_label(ai.events.call_usage(turn.all_calls()))}] calls[${calls}]`;
    if (turn.child_turns.length() == 0) {
        label
    } else {
        `${label} children[${turn.child_turns.map(turn_label).join("; ")}]`
    }
}

/// A leaf client: each call bills `input`/`output` tokens to its provider and
/// model. The first `fail_first` calls fail before a response, as a dropped
/// connection does, and bill nothing.
class UsageClient {
    provider: string,
    model: string,
    input: int,
    output: int,
    replies: string[],
    fail_first: int,
    invocations: int,

    function call_record(self) -> ai.events.ModelCall {
        ai.events.ModelCall {
            resolved_model: ai.events.ModelKey { provider: self.provider, model: self.model },
            usage: null,
            timing: ai.events.Timing { start_time: null, duration: null, ttft: null },
            http_request: null,
            http_response: null,
            sse_responses: null,
        }
    }

    implements ai.Client {
        function requested_model(self) -> string {
            self.model
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/usage",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.invocations += 1;
            if (self.invocations <= self.fail_first) {
                throw ai.errors.NetworkFailure {
                    status_code: null,
                    provider: self.provider,
                    detail: "scripted drop",
                    raw_body: null,
                }
            }
            let reply = self.replies.at(self.invocations - self.fail_first - 1) ?? "1";
            ai.internal.with_own_call(
                ai.ModelTurn {
                    child_turns: [],
                    calls: [],
                    content: [ai.content.Text { text: reply }],
                    stop_reason: ai.content.StopReason.Complete,
                },
                self.call_record(),
                tokens(self.input, self.output),
                self.model,
                null,
            )
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            self.invocations += 1;
            if (self.invocations <= self.fail_first) {
                throw ai.errors.NetworkFailure {
                    status_code: null,
                    provider: self.provider,
                    detail: "scripted drop",
                    raw_body: null,
                }
            }
            let events: ai.stream.Event[] = [
                ai.stream.TextDelta { text: "1" },
                ai.stream.TurnMeta {
                    stop_reason: ai.content.StopReason.Complete,
                    usage: tokens(self.input, self.output),
                },
                ai.stream.TurnDone {},
            ];
            let cursor = 0;
            ai.stream.TurnStream.from_event_source(
                () -> {
                    let next = events.at(cursor);
                    cursor += 1;
                    match (next) {
                        let e: ai.stream.Event => e,
                        null => ai.stream.Done {},
                    }
                },
                call = self.call_record(),
            )
        }
    }
}

function usage_client(
    provider: string,
    model: string,
    input: int,
    output: int,
    fail_first: int = 0,
    replies: string[] = [],
) -> UsageClient {
    UsageClient {
        provider: provider,
        model: model,
        input: input,
        output: output,
        replies: replies,
        fail_first: fail_first,
        invocations: 0,
    }
}

/// A client that asks a cheap classifier first and then delegates the turn to
/// `inner`: both turns are its children, and it makes no call itself.
class ClassifyThenAnswer {
    classifier: UsageClient,
    inner: ai.Client,

    implements ai.Client {
        function requested_model(self) -> string {
            "classify-then-answer"
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            self.inner.render(input)
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let classified = self.classifier.invoke(input);
            let answered = self.inner.invoke(input);
            ai.ModelTurn { ...answered, calls: [], child_turns: [classified, answered] }
        }
    }
}

function no_backoff() -> ai.clients.Backoff {
    ai.clients.Backoff.new(initial_ms = 0, max_ms = 0)
}

function drain_turn(client: ai.stream.StreamingClient) -> ai.ModelTurn {
    client.invoke_stream(usage_input()).final_turn()
}

// ─── merge_usage ─────────────────────────────────────────────────────────────

test "merge_usage sums a shared model and keeps the others" {
    let a = ai.events.ModelKey { provider: "anthropic", model: "claude-sonnet-5-5" };
    let b = ai.events.ModelKey { provider: "openai", model: "gpt-5" };
    let into = [
        ai.events.ModelUsage {
            resolved_model: a,
            usage: ai.events.Usage {
                input_tokens: 10,
                output_tokens: 1,
                cached_input_tokens: 4,
                reasoning_tokens: null,
                cache_write_input_tokens: null,
                uncached_input_tokens: 6,
            },
        },
    ];
    let from = [
        ai.events.ModelUsage {
            resolved_model: a,
            usage: ai.events.Usage {
                input_tokens: 5,
                output_tokens: 2,
                cached_input_tokens: null,
                reasoning_tokens: null,
                cache_write_input_tokens: null,
                uncached_input_tokens: 5,
            },
        },
        ai.events.ModelUsage { resolved_model: b, usage: tokens(7, 3) },
    ];
    ai.events.merge_usage(into, from);
    assert.equal(usage_label(into), "anthropic/claude-sonnet-5-5=15/3,openai/gpt-5=7/3");
    // null + n = n; null + null = null
    let merged = ai.events.usage_for(into, a);
    assert.equal(merged?.cached_input_tokens, 4);
    assert.equal(merged?.reasoning_tokens, null);
    assert.equal(merged?.uncached_input_tokens, 11);
    // keys compare by value, not identity
    assert.equal(
        ai.events.usage_for(into, ai.events.ModelKey { provider: "openai", model: "gpt-5" })
            ?.input_tokens,
        7,
    );
    // `from` is unchanged
    assert.equal(usage_label(from), "anthropic/claude-sonnet-5-5=5/2,openai/gpt-5=7/3")
}

test "an unknown model is its own key" {
    let unknown = ai.events.ModelKey { provider: "custom", model: null };
    let usage = [ai.events.ModelUsage { resolved_model: unknown, usage: tokens(1, 2) }];
    ai.events.merge_usage(
        usage,
        [
            ai.events.ModelUsage {
                resolved_model: ai.events.ModelKey { provider: "custom", model: "m" },
                usage: tokens(3, 4),
            },
        ],
    );
    ai.events.merge_usage(usage, [ai.events.ModelUsage { resolved_model: unknown, usage: tokens(5, 6) }]);
    assert.equal(usage_label(usage), "custom/-=6/8,custom/m=3/4")
}

test "a count no call reported stays null when summed" {
    let key = ai.events.ModelKey { provider: "custom", model: "m" };
    let only_output = ai.events.Usage { output_tokens: 2 };
    let usage = [ai.events.ModelUsage { resolved_model: key, usage: only_output }];
    ai.events.merge_usage(usage, [ai.events.ModelUsage { resolved_model: key, usage: only_output }]);
    ai.events.merge_usage(
        usage,
        [ai.events.ModelUsage { resolved_model: key, usage: ai.events.Usage { input_tokens: 5 } }],
    );
    assert.equal(usage.length(), 1);
    let summed = ai.events.usage_for(usage, key);
    assert.equal(summed?.input_tokens, 5);
    assert.equal(summed?.output_tokens, 4);
    assert.equal(summed?.cached_input_tokens, null)
}

test "a stream folds counts reported across events" {
    let events: ai.stream.Event[] = [
        ai.stream.TurnMeta {
            stop_reason: null,
            usage: ai.events.Usage { input_tokens: 10, cached_input_tokens: 4 },
        },
        ai.stream.TextDelta { text: "1" },
        ai.stream.TurnMeta {
            stop_reason: ai.content.StopReason.Complete,
            usage: ai.events.Usage { output_tokens: 3 },
        },
        ai.stream.TurnMeta { stop_reason: null, usage: ai.events.Usage { output_tokens: 5 } },
    ];
    let cursor = 0;
    let stream = ai.stream.TurnStream.from_event_source(
        () -> {
            let next = events.at(cursor);
            cursor += 1;
            match (next) {
                let e: ai.stream.Event => e,
                null => ai.stream.Done {},
            }
        },
        call = usage_client("anthropic", "sonnet", 0, 0).call_record(),
    );
    let call = stream.final_turn().calls.at(0) ?? baml.sys.panic("the stream's call");
    // The latest value of each count wins; counts no event reported stay null.
    assert.equal(call.usage?.input_tokens, 10);
    assert.equal(call.usage?.output_tokens, 5);
    assert.equal(call.usage?.cached_input_tokens, 4);
    assert.equal(call.usage?.reasoning_tokens, null)
}

test "call_usage skips calls with no usage" {
    let failed = usage_client("openai", "gpt-5", 0, 0).call_record();
    let answered = usage_client("anthropic", "sonnet", 0, 0).call_record();
    answered.usage = tokens(100, 20);
    assert.equal(usage_label(ai.events.call_usage([failed, answered])), "anthropic/sonnet=100/20")
}

// ─── invoke ──────────────────────────────────────────────────────────────────

test "a leaf client's turn is its one call" {
    let turn = usage_client("anthropic", "sonnet", 100, 20).invoke(usage_input());
    assert.equal(turn_label(turn), "total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]");
    assert.equal(turn.resolved_model?.model, "sonnet")
}

test "a retry's usage is its successful attempt" {
    let retry = ai.clients.Retry.new(
        usage_client("anthropic", "sonnet", 100, 20, fail_first = 1),
        max_attempts = 3,
        backoff = no_backoff(),
    );
    let turn = retry.invoke(usage_input());
    assert.equal(
        turn_label(turn),
        "total[anthropic/sonnet=100/20] calls[] children[failed: total[] calls[]; total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]",
    )
}

test "a fallback's usage is the member that answered" {
    let fallback = ai.clients.Fallback {
        members: [
            usage_client("openai", "gpt-5", 50, 5, fail_first = 1),
            usage_client("anthropic", "sonnet", 100, 20),
        ],
    };
    let turn = fallback.invoke(usage_input());
    assert.equal(
        turn_label(turn),
        "total[anthropic/sonnet=100/20] calls[] children[failed: total[] calls[]; total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]",
    )
}

test "a round robin's usage is the member it picked" {
    let rr = ai.clients.RoundRobin.new(
        [usage_client("openai", "gpt-5", 50, 5), usage_client("anthropic", "sonnet", 100, 20)],
    );
    assert.equal(
        turn_label(rr.invoke(usage_input())),
        "total[openai/gpt-5=50/5] calls[] children[total[openai/gpt-5=50/5] calls[openai/gpt-5=50/5]]",
    );
    assert.equal(
        turn_label(rr.invoke(usage_input())),
        "total[anthropic/sonnet=100/20] calls[] children[total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]",
    )
}

test "nested wrappers nest their members' turns" {
    let inner = ai.clients.Fallback {
        members: [
            usage_client("openai", "gpt-5", 50, 5, fail_first = 1),
            usage_client("anthropic", "sonnet", 100, 20),
        ],
    };
    let retry = ai.clients.Retry.new(inner, max_attempts = 2, backoff = no_backoff());
    assert.equal(
        turn_label(retry.invoke(usage_input())),
        "total[anthropic/sonnet=100/20] calls[] children[total[anthropic/sonnet=100/20] calls[] children[failed: total[] calls[]; total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]]",
    )
}

test "a client's own call and its delegate's both count" {
    let composite = ClassifyThenAnswer {
        classifier: usage_client("openai", "gpt-5-mini", 30, 2),
        inner: ai.clients.Retry.new(usage_client("anthropic", "sonnet", 100, 20), backoff = no_backoff()),
    };
    assert.equal(
        turn_label(composite.invoke(usage_input())),
        "total[openai/gpt-5-mini=30/2,anthropic/sonnet=100/20] calls[] children[total[openai/gpt-5-mini=30/2] calls[openai/gpt-5-mini=30/2]; total[anthropic/sonnet=100/20] calls[] children[total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]]",
    )
}

// ─── the runner ──────────────────────────────────────────────────────────────

test "a run sums every call and journals each with its usage" {
    // The first reply does not parse, so the runner asks again: two turns.
    let composite = ClassifyThenAnswer {
        classifier: usage_client("openai", "gpt-5-mini", 30, 2),
        inner: usage_client("anthropic", "sonnet", 100, 20, replies = ["nope", "7"]),
    };
    let result = ai.Agent.new(client = composite, schema_attempts = 2).run(UsageSpec@spec());
    assert.equal(result.value, 7);
    assert.equal(usage_label(result.usage), "openai/gpt-5-mini=60/4,anthropic/sonnet=200/40");
    let journaled: string[] = [];
    for (let e in result.journal.entries()) {
        if let c: ai.events.ModelCall = e {
            journaled.push(call_label(c));
        }
    }
    assert.equal(
        journaled,
        [
            "openai/gpt-5-mini=30/2",
            "anthropic/sonnet=100/20",
            "openai/gpt-5-mini=30/2",
            "anthropic/sonnet=100/20",
        ],
    )
}

// ─── invoke_stream ───────────────────────────────────────────────────────────

test "a leaf stream's turn is its one call" {
    assert.equal(
        turn_label(drain_turn(usage_client("anthropic", "sonnet", 100, 20))),
        "total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]",
    )
}

test "a retried stream's usage is its successful attempt" {
    let retry = ai.clients.Retry.new(
        usage_client("anthropic", "sonnet", 100, 20, fail_first = 1),
        max_attempts = 2,
        backoff = no_backoff(),
    );
    assert.equal(
        turn_label(drain_turn(retry)),
        "total[anthropic/sonnet=100/20] calls[] children[failed: total[] calls[]; total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]",
    )
}

test "a fallback stream's usage is the member that answered" {
    let fallback = ai.clients.Fallback {
        members: [
            usage_client("openai", "gpt-5", 50, 5, fail_first = 1),
            usage_client("anthropic", "sonnet", 100, 20),
        ],
    };
    assert.equal(
        turn_label(drain_turn(fallback)),
        "total[anthropic/sonnet=100/20] calls[] children[failed: total[] calls[]; total[anthropic/sonnet=100/20] calls[anthropic/sonnet=100/20]]",
    )
}

test "a round robin stream's usage is the member it picked" {
    let rr = ai.clients.RoundRobin.new([usage_client("openai", "gpt-5", 50, 5)]);
    assert.equal(
        turn_label(drain_turn(rr)),
        "total[openai/gpt-5=50/5] calls[] children[total[openai/gpt-5=50/5] calls[openai/gpt-5=50/5]]",
    )
}

test "a stream with counts and no call records no usage" {
    let stream = ai.stream.TurnStream.from_event_source(() -> {
        ai.stream.Done {}
    });
    stream._usage = tokens(3, 4);
    assert.equal(turn_label(stream.final_turn()), "total[] calls[]")
}

// ─── requested and resolved models ───────────────────────────────────────────

function openai_call() -> ai.events.ModelCall {
    ai.events.ModelCall {
        resolved_model: ai.events.ModelKey { provider: "openai", model: "gpt-5" },
        usage: null,
        timing: ai.events.Timing { start_time: null, duration: null, ttft: null },
        http_request: null,
        http_response: null,
        sse_responses: null,
    }
}

function empty_turn() -> ai.ModelTurn {
    ai.ModelTurn { content: [], stop_reason: ai.content.StopReason.Complete, calls: [], child_turns: [] }
}

test "a turn keeps the model it asked for and the one that answered" {
    let turn = ai.internal.with_own_call(empty_turn(), openai_call(), tokens(1, 1), "gpt-5", "gpt-5-2025-08-07");
    assert.equal(turn.requested_model, "gpt-5");
    assert.equal(turn.resolved_model?.model, "gpt-5-2025-08-07");
    assert.equal(turn.calls[0].resolved_model.model, "gpt-5-2025-08-07")
}

test "a response that names no model resolves to the one requested" {
    let turn = ai.internal.with_own_call(empty_turn(), openai_call(), tokens(1, 1), "gpt-5", null);
    assert.equal(turn.resolved_model?.model, "gpt-5")
}

/// How many blocks of the journal's one assistant turn survive replay to a
/// client of `provider` asking for `requested`.
function replayed_blocks(provider: string, requested: string) -> int {
    let journal = ai.Journal {
        log: [
            ai.events.AssistantMessage {
                content: [ai.content.Reasoning { summary: "thinking" }, ai.content.Text { text: "4" }],
                resolved_model: ai.events.ModelKey { provider: "openai", model: "gpt-5-2025-08-07" },
                requested_model: "gpt-5",
            },
        ],
    };
    let out = 0;
    for (let e in ai.wire.sanitize_for_client(journal, provider, requested).entries()) {
        if let m: ai.events.AssistantMessage = e {
            out = m.content.length();
        }
    }
    out
}

test "reasoning replays to the client that asked for the model, whatever it resolved to" {
    assert.equal(replayed_blocks("openai", "gpt-5"), 2);
    assert.equal(replayed_blocks("openai", "gpt-4o"), 1);
    assert.equal(replayed_blocks("azure", "gpt-5"), 1)
}

test "a stream resolves to the model its events name, as a whole response does" {
    let events: ai.stream.Event[] = [
        ai.stream.TextDelta { text: "1" },
        ai.stream.TurnMeta { stop_reason: null, usage: tokens(1, 1), resolved_model: "gpt-5-2025-08-07" },
        ai.stream.TurnDone {},
    ];
    let cursor = 0;
    let stream = ai.stream.TurnStream.from_event_source(
        () -> {
            let next = events.at(cursor);
            cursor += 1;
            match (next) {
                let e: ai.stream.Event => e,
                null => ai.stream.Done {},
            }
        },
        call = openai_call(),
    );
    stream._requested_model = "gpt-5";
    let turn = stream.final_turn();
    assert.equal(turn.requested_model, "gpt-5");
    assert.equal(turn.resolved_model?.model, "gpt-5-2025-08-07");
    assert.equal(turn.calls[0].resolved_model.model, "gpt-5-2025-08-07");
    assert.equal(turn.calls[0].usage?.output_tokens, 1)
}

test "a round robin asks for the model of the member its next call goes to" {
    let rr = ai.clients.RoundRobin.new(
        [usage_client("openai", "gpt-5", 50, 5), usage_client("anthropic", "sonnet", 100, 20)],
    );
    assert.equal(rr.requested_model(), "gpt-5");
    let _ = rr.invoke(usage_input());
    assert.equal(rr.requested_model(), "sonnet")
}

test "a streamed retry's failed attempt names the model its member asked for" {
    let retry = ai.clients.Retry.new(
        usage_client("anthropic", "sonnet", 100, 20, fail_first = 1),
        max_attempts = 2,
        backoff = no_backoff(),
    );
    let turn = drain_turn(retry);
    assert.equal(turn.child_turns[0].error != null, true);
    assert.equal(turn.child_turns[0].requested_model, "sonnet")
}
