// Offline streaming tests: a scripted StreamingClient feeds literal delta
// chunks through ai.TurnStream.from_chunks, driving the typed partial
// stream with no network.

// A deterministic streaming fake: `invoke_stream` returns a TurnStream over
// pre-written delta chunks; `invoke` (required by StreamingClient's
// `requires Client`) answers with the joined text in one turn.
class ScriptedStreamingClient {
    chunks: string[],

    function new(chunks: string[]) -> ScriptedStreamingClient {
        ScriptedStreamingClient { chunks: chunks }
    }

    implements ai.Client {
        function id(self) -> string {
            "scripted-stream"
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let joined = "";
            for (let c in self.chunks) {
                joined = `${joined}${c}`;
            }
            ai.ModelTurn {
                content: [ai.Text { text: joined }],
                stop_reason: ai.StopReason.Complete,
                usage: null,
            }
        }
    }

    implements ai.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.TurnStream {
            ai.TurnStream.from_chunks(self.chunks)
        }
    }
}

class Contact {
    name: string,
    email: string,
}

// A tool-less LLM function: gets a `$stream` companion.
function ExtractContact(text: string) -> Contact {
    client "openai/gpt-4o-mini"
    tools []
    prompt `
        Extract the contact from: ${text}
        ${ctx.output_format}
    `
}

// A tool-less function whose default client (claude_code) cannot stream.
function NonStreamingDefault(q: string) -> string {
    client "claude-code/sonnet"
    tools []
    prompt `
        ${q}
        ${ctx.output_format}
    `
}

test "a scripted TurnStream yields deltas then a completed turn" {
    let ts = ai.TurnStream.from_chunks(["hel", "lo"]);
    let first = ts.next();
    match (first) {
        let s: string => assert.equal(s, "hel"),
        _ => assert.is_true(false),
    };
    let turn = ts.final_turn();
    assert.equal(turn.terminal_text() ?? "", "hello");
    assert.is_true(turn.stop_reason == ai.StopReason.Complete)
}

test "$stream yields progressive partials and a typed final value" {
    let scripted = ScriptedStreamingClient.new(
        [
            `{"name": "Ada Lovelace"`,
            `, "email": "ada@`,
            `example.com"}`,
        ],
    );
    let stream = ExtractContact$stream(
        text = "Ada Lovelace <ada@example.com>",
        client = scripted,
    );
    let partials: Contact$stream[] = [];
    while (true) {
        match (stream.next()) {
            let done: baml.stream.StreamFinished => break,
            let p: Contact$stream => {
                partials.push(p);
                null
            },
            null => null,
        };
    }
    assert.is_true(partials.length() > 1);
    // fields appear progressively: the first partial has the name but no email
    let early = partials.at(0);
    assert.equal(early?.name ?? "", "Ada Lovelace");
    assert.is_true(early?.email == null);
    // "call final() after StreamFinished" returns the full typed value
    let value = stream.final();
    assert.equal(value.name, "Ada Lovelace");
    assert.equal(value.email, "ada@example.com")
}

test "stream_spec drives a spec directly through a streaming client" {
    let scripted = ScriptedStreamingClient.new(
        [`{"name": "Grace Hopper", "email": `, `"grace@navy.mil"}`],
    );
    let stream = ai.stream_spec<Contact$stream, Contact>(
        ExtractContact@spec(text = "Grace Hopper <grace@navy.mil>"),
        client = scripted,
    );
    let yields = 0;
    while (true) {
        match (stream.next()) {
            let done: baml.stream.StreamFinished => break,
            _ => {
                yields += 1;
                null
            },
        };
    }
    assert.is_true(yields > 0);
    let value = stream.final();
    assert.equal(value.email, "grace@navy.mil")
}

test "streaming rejects a tools-bearing spec" {
    // NamedToolbox (tests/spec_desugar.baml) carries a non-empty toolbox;
    // streaming does not run the tool loop, so stream_spec throws typed.
    let scripted = ScriptedStreamingClient.new([]);
    let outcome = ai.stream_spec<string, string>(
        NamedToolbox@spec(q = "find flights"),
        client = scripted,
    ) catch_all (e) {
        let bad: baml.errors.InvalidArgument => null,
        _ => throw e,
    };
    assert.is_true(outcome == null)
}

test "a non-streaming default client fails typed" {
    // No override: the spec's default client (claude_code) does not implement
    // ai.StreamingClient, so the runtime downcast throws StreamingUnsupported.
    let outcome = ai.stream_spec<string, string>(
        NonStreamingDefault@spec(q = "hello"),
    ) catch_all (e) {
        let un: ai.errors.StreamingUnsupported => null,
        _ => throw e,
    };
    assert.is_true(outcome == null)
}
