//! BEP-042: the collector finalizes the same instances now that it checks a
//! list of finalizable instances instead of walking every object in
//! from-space. Each test needs a program of its own (the corpus is one
//! program); the expected logs are what the full walk produced.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[track_caller]
fn values(output: Result<BexExternalValue, impl std::fmt::Debug>) -> Vec<BexExternalValue> {
    match output.expect("main runs") {
        BexExternalValue::Array { items, .. } => items,
        other => panic!("main returns an array, got {other:?}"),
    }
}

#[track_caller]
fn strings(output: Result<BexExternalValue, impl std::fmt::Debug>) -> Vec<String> {
    values(output)
        .into_iter()
        .map(|item| match item {
            BexExternalValue::String(s) => s.to_string(),
            other => panic!("a log entry is a string, got {other:?}"),
        })
        .collect()
}

/// The stdlib's `ai.stream.Stream` defines `cleanup`: a stream abandoned
/// before EOF is finalized, which closes its tracing span.
#[tokio::test]
async fn an_abandoned_stdlib_stream_is_finalized() {
    let output = baml_test!(
        r####"
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
        function provider(self) -> string {
            "test"
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

function abandon() -> trace.Span {
  let stream = ExtractNote@stream("x", client = ChunkedStreamClient { chunks: ["{"] });
  stream._span
}

function main() -> bool[] {
  let span = abandon();
  let before = span.is_closed();
  baml.sys.collect_garbage();
  [before, span.is_closed()]
}
"####
    );
    let closed: Vec<bool> = values(output.result)
        .into_iter()
        .map(|item| match item {
            BexExternalValue::Bool(b) => b,
            other => panic!("expected a bool, got {other:?}"),
        })
        .collect();
    assert_eq!(closed, [false, true]);
}

/// A session redeclares a class with a `cleanup` method it did not have: the
/// new class's instances are finalized, the old class's are not.
#[tokio::test]
async fn a_session_republish_that_adds_cleanup_finalizes_new_instances() {
    let output = baml_test!(
        r####"
function main() -> string[] {
  let s = reflect.Session.new();
  s.eval(`let log = ["start"]`);
  s.eval(`class R { tag string
    sink string[] }`);
  s.eval(`function make(tag: string, sink: string[]) -> null throws never { let r = R { tag: tag, sink: sink }; r.sink.push("made " + tag); null }`);
  s.eval(`make("v1", log)`);
  s.eval(`class R { tag string
    sink string[]
    function cleanup(self) -> void { self.sink.push("cleaned " + self.tag); } }`);
  s.eval(`function make2(tag: string, sink: string[]) -> null throws never { let r = R { tag: tag, sink: sink }; r.sink.push("made " + tag); null }`);
  s.eval(`make2("v2", log)`);
  baml.sys.collect_garbage();
  s.eval<string[]>(`log`)
}
"####
    );
    assert_eq!(
        strings(output.result),
        ["start", "made v1", "made v2", "cleaned v2"]
    );
}

/// A session redeclares a class without the `cleanup` method it had: an
/// instance of the old class that died before the redeclaration is still
/// finalized, and the new class's instances are not.
#[tokio::test]
async fn a_session_republish_that_removes_cleanup_still_finalizes_old_instances() {
    let output = baml_test!(
        r####"
function main() -> string[] {
  let s = reflect.Session.new();
  s.eval(`let log = ["start"]`);
  s.eval(`class R { tag string
    sink string[]
    function cleanup(self) -> void { self.sink.push("cleaned " + self.tag); } }`);
  s.eval(`function make(tag: string, sink: string[]) -> null throws never { let r = R { tag: tag, sink: sink }; r.sink.push("made " + tag); null }`);
  s.eval(`make("v1", log)`);
  s.eval(`class R { tag string
    sink string[] }`);
  s.eval(`function make2(tag: string, sink: string[]) -> null throws never { let r = R { tag: tag, sink: sink }; r.sink.push("made " + tag); null }`);
  s.eval(`make2("v2", log)`);
  baml.sys.collect_garbage();
  s.eval<string[]>(`log`)
}
"####
    );
    assert_eq!(
        strings(output.result),
        ["start", "made v1", "made v2", "cleaned v1"]
    );
}

/// An instance built through reflection (`reflect.Class.instantiate`) of a
/// class with a `cleanup` method is finalized like one built by a literal.
#[tokio::test]
async fn an_instance_built_through_reflection_is_finalized() {
    let output = baml_test!(
        r####"
class Resource {
  log: string[],
  function cleanup(self) -> void {
    self.log.push("cleaned");
  }
}

function abandon(log: string[]) -> null {
  let resource = reflect.Class.instantiate<Resource>({ "log": log });
  resource.log.push("created");
  null
}

function main() -> string[] {
  let log: string[] = [];
  abandon(log);
  baml.sys.collect_garbage();
  log
}
"####
    );
    assert_eq!(strings(output.result), ["created", "cleaned"]);
}
