// Native regression coverage for ai.Runner dispatch and TurnStream batching.

// This LLM function only mints a spec; the tests never call the provider.
function SpecDonor(x: int) -> int {
    client: "openai/gpt-4o-mini"
    prompt: `Echo ${x}. ${ctx.output_format()}`
}

class CannedRunner {
    payload: string,

    implements ai.Runner {
        type Error = baml.errors.ParseError | baml.errors.LlmClient;

        function run<Out>(self, spec: ai.FunctionSpec<Out>) -> ai.RunResult<Out> {
            ai.RunResult {
                value: baml.sap.parse<Out>(self.payload),
                journal: ai.Journal.new(spec),
                usage: ai.events.Usage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cached_input_tokens: null,
                    reasoning_tokens: null,
                },
            }
        }
    }
}

function drive(
    r: ai.Runner<Error = baml.errors.ParseError | baml.errors.LlmClient>,
    spec: ai.FunctionSpec<int>,
) -> int throws baml.errors.ParseError | baml.errors.LlmClient {
    r.run(spec).value
}

function make_backlog(n: int) -> ai.stream.TurnStream {
    let pending: ai.stream.Event[] = [];
    let i = 0;
    while (i < n) {
        pending.push(ai.stream.TextDelta { text: `chunk${i};` });
        i = i + 1;
    }
    pending.push(ai.stream.TurnDone {  });
    ai.stream.TurnStream {
        _event_source: null,
        _sse: null,
        _decode: null,
        _chunks: [],
        _cursor: 0,
        _pending: pending,
        _pcursor: 0,
        _text: "",
        _stop_reason: null,
        _input_tokens: null,
        _output_tokens: null,
        _done: false,
        _require_terminal: null,
        _saw_terminal: false,
        _calls: [],
        _wire_call: null,
        _call_started: null,
        _ttft_from: null,
        _capture_sse: false,
    }
}

test "agent_implements_runner_and_custom_runners_dispatch" {
    let out = drive(CannedRunner { payload: "7" }, SpecDonor@spec(x = 1));
    assert.equal(out, 7);

    // Static conformance: the default Agent is assignable to Runner.
    let runner: ai.Runner<
        Error = baml.errors.InvalidArgument
            | baml.errors.Timeout
            | baml.errors.UnknownError
            | reflect.errors.CompilationError
            | ai.errors.Failure
    > = ai.Agent.new();
    assert.not_null(runner)
}

test "turnstream_backlog_drains_in_one_batch_with_boundaries" {
    let stream = make_backlog(100);
    match (stream.next()) {
        let batch: string[] => {
            assert.equal(batch.length(), 100);
            assert.equal(batch.at(0) ?? "", "chunk0;");
            assert.equal(batch.at(99) ?? "", "chunk99;");
            null
        },
        _ => throw "expected a batch on the first pull",
    };
    match (stream.next()) {
        let done: ai.stream.Done => null,
        _ => throw "expected Done on the second pull",
    };
    match (stream.next()) {
        let done: ai.stream.Done => null,
        _ => throw "next() after Done must keep returning Done",
    }
}

test "turnstream_scripted_chunks_stay_per_chunk" {
    let stream = ai.stream.TurnStream.from_chunks(["a", "b"]);
    match (stream.next()) {
        let batch: string[] => assert.equal(batch, ["a"]),
        _ => throw "expected a batch",
    };
    match (stream.next()) {
        let batch: string[] => assert.equal(batch, ["b"]),
        _ => throw "expected a batch",
    };
    match (stream.next()) {
        let done: ai.stream.Done => null,
        _ => throw "expected Done",
    }
}
