// Offline tests for partial parsing in `ai.stream.Stream`. A partial that can't
// be parsed yet (an opening ```json fence, or prose before the JSON) is skipped,
// as in BAML v0; only the final parse reports a malformed reply.

class Note {
    title: string,
    body: string,
}

function ExtractNote(input: string) -> Note {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize ${input}. ${ctx.output_format()}`
}

class ChunkedStreamClient {
    chunks: string[],

    implements ai.Client {
        function requested_model(self) -> string {
            "chunked"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/chunked",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                child_turns: [],
                calls: [],
                content: [ai.content.Text { text: self.chunks.join("") }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            ai.stream.TurnStream.from_chunks(self.chunks)
        }
    }
}

// Drains `chunks` through `ExtractNote@stream`: "<any partials>|<final title>/<final body>".
function stream_note(chunks: string[]) -> string {
    let stream = ai.stream.from_spec<Note>(
        ExtractNote@spec("x"),
        client = ChunkedStreamClient { chunks: chunks },
    );
    let titles: string[] = [];
    while let partial: Note = stream.next() {
        titles.push(partial.title);
    }
    let note = stream.final();
    `${titles.length() > 0}|${note.title}/${note.body}`
}

test "stream_skips_partials_before_a_json_fence_opens" {
    assert.equal(
        stream_note(["```", "json\n", "{\"title\": \"Hi\", ", "\"body\": \"there\"}", "\n```"]),
        "true|Hi/there",
    )
}

test "stream_skips_partials_for_prose_before_the_json" {
    assert.equal(
        stream_note(["Here", " is the JSON:\n", "{\"title\": \"Hi\", ", "\"body\": \"there\"}"]),
        "true|Hi/there",
    )
}

function stream_unparseable_reply() -> string {
    let stream = ai.stream.from_spec<Note>(
        ExtractNote@spec("x"),
        client = ChunkedStreamClient { chunks: ["no", " json", " here"] },
    );
    let partials = 0;
    while let partial: Note = stream.next() {
        let _ = partial;
        partials += 1;
    }
    let final_outcome = stream.final().title catch_all (e) {
        _ => "final-threw",
    };
    `${partials}|${final_outcome}`
}

test "stream_still_reports_an_unparseable_final_reply" {
    assert.equal(stream_unparseable_reply(), "0|final-threw")
}

test "stream EOF settles and repeated final returns cached value" {
    let stream = ExtractNote@stream("x", client = ChunkedStreamClient { chunks: ["{\"title\": \"Hi\", \"body\": \"there\"}"] });
    while let partial: Note = stream.next() {
        let _ = partial;
    }
    assert.equal(stream._span.is_closed(), true);
    assert.equal(stream.final().title, "Hi");
    assert.equal(stream.final().body, "there");
    stream.cleanup();
    assert.equal(stream.final().title, "Hi");
}

test "stream EOF caches parse failure and cleanup closes abandonment" {
    let bad = ExtractNote@stream("x", client = ChunkedStreamClient { chunks: ["invalid"] });
    while let partial: Note = bad.next() {
        let _ = partial;
    }
    assert.equal(bad._span.is_closed(), true);
    let errors = 0;
    bad.final() catch_all (e) {
        _ => {
            errors += 1;
        }
    };
    bad.final() catch_all (e) {
        _ => {
            errors += 1;
        }
    };
    assert.equal(errors, 2);
    let abandoned = ExtractNote@stream("x", client = ChunkedStreamClient { chunks: ["{"] });
    abandoned.cleanup();
    assert.equal(abandoned._span.is_closed(), true);
    assert.equal(abandoned.next() is ai.stream.Done, true);
}

function make_handoff_stream() -> ai.stream.Stream<Note> {
    ExtractNote@stream("x", client = ChunkedStreamClient { chunks: ["{\"title\": \"Hi\",", "\"body\": \"there\"}"] })
}

class StreamHolder {
    stream: ai.stream.Stream<Note>,
}

test "stream handoff retains lifetime through a spawned consumer" {
    let holder = StreamHolder { stream: make_handoff_stream() };
    let original_id = holder.stream._span.id();
    let task = spawn {
        let _ = holder.stream.next();
        assert.equal(holder.stream._span.id(), original_id);
        holder.stream.final()
    };
    let result = await task;
    assert.equal(result.title, "Hi");
    assert.equal(holder.stream.final().body, "there");
    assert.equal(holder.stream._span.is_closed(), true);
}


class HookCounter {
    calls: int,
}

function record_entry(input: string, counter: HookCounter, settings: trace.Settings) -> trace.Options throws never {
    counter.calls += 1;
    trace.span(inputs = true, output = true).context(metadata = { "entry": "direct" })
}

/// baml:$trace=record_entry
function HookedNote(input: string, counter: HookCounter) -> Note {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize ${input}. ${ctx.output_format()}`
}

test "AI declaration hooks run only on direct and pointer calls" {
    let counter = HookCounter { calls: 0 };
    let provider = ChunkedStreamClient { chunks: ["{\"title\":\"Hi\",\"body\":\"there\"}"] };
    // With telemetry disabled hooks are suppressed; compare paths to the first call.
    assert.equal(HookedNote("x", counter, client = provider).title, "Hi");
    let per_call = counter.calls;
    let callable = HookedNote;
    assert.equal(callable("x", counter, client = provider).title, "Hi");
    assert.equal(counter.calls, per_call * 2);
    let spec = HookedNote@spec("x", counter);
    assert.equal(spec.call(client = provider).title, "Hi");
    assert.equal(ai.Agent.new(client = provider).run(spec).value.title, "Hi");
    assert.equal(HookedNote@stream("x", counter, client = provider).final().title, "Hi");
    assert.equal(counter.calls, per_call * 2);
    assert.equal(spec.arguments().has("client"), false);
    assert.equal(spec.arguments().has("on_event"), false);
}

test "stream forwards entry reservation and raw alias cannot reclaim" {
    let reserved = trace.span(inputs = true, output = true).reserve();
    let stream = ExtractNote@stream("x", client = ChunkedStreamClient {
        chunks: ["{\"title\":\"Hi\",\"body\":\"there\"}"],
    }, $trace = reserved);
    if (stream._span.id() != null) {
        assert.equal(stream._span.id(), reserved.id());
    }
    assert.equal(stream.final().title, "Hi");
    let rejected = false;
    ExtractNote@stream("x", client = ChunkedStreamClient { chunks: [] }, $trace = reserved) catch_all_panics (e) {
        _ => { rejected = true; },
    };
    assert.equal(rejected, true);
}

test "independent spec executions preserve their argument map and reservations" {
    let provider = ChunkedStreamClient { chunks: ["{\"title\":\"Hi\",\"body\":\"there\"}"] };
    let spec = ExtractNote@spec("x");
    let first = trace.span().reserve();
    assert.equal(spec.call(client = provider, $trace = first).title, "Hi");
    assert.equal(spec.call(client = provider, $trace = trace.span().reserve()).title, "Hi");
    assert.equal(ai.Agent.new(client = provider).run(spec, $trace = trace.span().reserve()).value.title, "Hi");
    assert.equal(spec.arguments().has("client"), false);
}


test "stream panic closes its operation and rethrows the cached panic" {
    let stream = ExtractNote@stream("panic during read", client = ChunkedStreamClient {
        chunks: ["{\"title\":\"Hi\",\"body\":\"there\"}"],
    });
    stream._next_text = () -> { baml.sys.panic("stream read failed") };
    let caught = 0;
    stream.next() catch_all_panics (error) {
        let panic: baml.panics.UserPanic => {
            assert.equal(panic.message, "stream read failed");
            caught += 1;
        },
        _ => {},
    };
    assert.equal(stream._span.is_closed(), true);
    stream.final() catch_all_panics (error) {
        let panic: baml.panics.UserPanic => {
            assert.equal(panic.message, "stream read failed");
            caught += 1;
        },
        _ => {},
    };
    assert.equal(caught, 2);
}


class NonStreamingClient {
    implements ai.Client {
        function requested_model(self) -> string { "non-streaming" }
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            baml.sys.panic("client resolution must fail before render")
        }
        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            baml.sys.panic("client resolution must fail before invoke")
        }
    }
}

function InvalidClientNote(input: string) -> Note {
    client: NonStreamingClient {}
    prompt: `Summarize ${input}. ${ctx.output_format()}`
}

test "client resolution failure consumes its eager reservation claim" {
    let reserved = trace.span().reserve();
    let failed = false;
    InvalidClientNote@stream("bad client", $trace = reserved) catch_all (error) {
        _ => { failed = true; },
    };
    assert.equal(failed, true);
    let rejected = false;
    ExtractNote("retry", client = ChunkedStreamClient { chunks: ["{}"] }, $trace = reserved) catch_all_panics (error) {
        _ => { rejected = true; },
    };
    assert.equal(rejected, true);
}
