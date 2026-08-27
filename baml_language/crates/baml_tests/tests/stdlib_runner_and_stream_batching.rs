//! Executable oracles for two stdlib changes:
//!
//! - `ai.Runner` is method-generic (`run<Out>` on the method, `type Error`
//!   associated) and `ai.Agent` implements it. Custom runners implement the
//!   same interface, override `Error`, and dispatch through
//!   `ai.Runner<Error = …>`-typed values with `Out` inferred per call.
//! - `ai.stream.TurnStream.next()` returns every decoded-and-ready delta as
//!   one `string[]` batch — delta boundaries preserved — and never blocks on
//!   the wire while holding deliverable text. This is the
//!   drain-ready-then-parse fix: a consumer that keeps up sees one-element
//!   batches; a backlogged consumer gets the whole backlog in one return
//!   instead of replaying it delta by delta (and re-running the partial
//!   parse per delta, which is quadratic in output size).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn agent_implements_runner_and_custom_runners_dispatch() {
    let source = r####"
        // An LLM function only to mint specs; never called.
        function SpecDonor(x: int) -> int {
            client: "openai/gpt-4o-mini"
            prompt: `Echo ${x}. ${ctx.output_format()}`
        }

        // A runner that "answers" from a canned payload via the schema-aware
        // parser. Overrides the interface's associated Error type.
        class CannedRunner {
            payload: string,
            implements ai.Runner {
                type Error = baml.errors.ParseError | baml.errors.LlmClient
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

        // Dispatch through the interface type; Out inferred from the spec.
        function drive(
            r: ai.Runner<Error = baml.errors.ParseError | baml.errors.LlmClient>,
            spec: ai.FunctionSpec<int>,
        ) -> int throws baml.errors.ParseError | baml.errors.LlmClient {
            r.run(spec).value
        }

        function main() -> string throws unknown {
            let out = drive(CannedRunner { payload: "7" }, SpecDonor@spec(x = 1));
            if (out != 7) {
                throw `canned runner returned ${out}, expected 7`
            }
            // Static conformance: the default Agent is assignable to Runner.
            let r: ai.Runner<
                Error = baml.errors.InvalidArgument
                    | baml.errors.Timeout
                    | baml.errors.UnknownError
                    | baml.panics.Cancelled
                    | reflect.errors.CompilationError
                    | ai.errors.Failure
            > = ai.Agent.new();
            if (r == null) {
                throw "unreachable"
            }
            "ok"
        }
    "####;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::String("ok".into())));
}

#[tokio::test]
async fn turnstream_backlog_drains_in_one_batch_with_boundaries() {
    let source = r####"
        function make_backlog(n: int) -> ai.stream.TurnStream {
            let pending: ai.stream.StreamEvent[] = [];
            let i = 0;
            while (i < n) {
                pending.push(ai.stream.TextDelta { text: `chunk${i};` });
                i = i + 1;
            }
            pending.push(ai.stream.TurnDone {  });
            ai.stream.TurnStream {
                _event_source: null, _sse: null, _decode: null,
                _chunks: [], _cursor: 0,
                _pending: pending, _pcursor: 0,
                _text: "", _stop_reason: null,
                _input_tokens: null, _output_tokens: null,
                _done: false, _require_terminal: null, _saw_terminal: false,
                _calls: [], _wire_call: null, _call_started: null,
                _ttft_from: null, _capture_sse: false,
            }
        }

        function main() -> string throws unknown {
            let ts = make_backlog(100);
            // pull 1: the entire backlog as ONE batch, boundaries preserved
            match (ts.next()) {
                let b: string[] => {
                    if (b.length() != 100) {
                        throw `expected 100 deltas in one batch, got ${b.length()}`
                    }
                    if ((b.at(0) ?? "") != "chunk0;") {
                        throw "first delta lost its boundary"
                    }
                    if ((b.at(99) ?? "") != "chunk99;") {
                        throw "last delta lost its boundary"
                    }
                    null
                },
                _ => throw "expected a batch on the first pull",
            };
            // text decoded ahead of the terminal was delivered before Done
            match (ts.next()) {
                let d: ai.stream.Done => null,
                _ => throw "expected Done on the second pull",
            };
            match (ts.next()) {
                let d: ai.stream.Done => null,
                _ => throw "next() after Done must keep returning Done",
            };
            "ok"
        }
    "####;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::String("ok".into())));
}

#[tokio::test]
async fn turnstream_scripted_chunks_stay_per_chunk() {
    let source = r####"
        function main() -> string throws unknown {
            let ts = ai.stream.TurnStream.from_chunks(["a", "b"]);
            match (ts.next()) {
                let b: string[] => {
                    if (b != ["a"]) {
                        throw "scripted streams must yield one-element batches"
                    }
                    null
                },
                _ => throw "expected a batch",
            };
            match (ts.next()) {
                let b: string[] => {
                    if (b != ["b"]) {
                        throw "second chunk wrong"
                    }
                    null
                },
                _ => throw "expected a batch",
            };
            match (ts.next()) {
                let d: ai.stream.Done => null,
                _ => throw "expected Done",
            };
            "ok"
        }
    "####;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::String("ok".into())));
}
