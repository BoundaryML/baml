//! The bytes the runtime allocates for one LLM call stay small.
//!
//! A Rust test because BAML cannot observe allocations. Each parse used to build
//! a schema-aligned-parsing model of every type in the program, the standard
//! library included: about 5 MB per call for a one-function program, kept alive
//! until the VM collected it. And every `match` type test that failed
//! canonicalized the recursive `json` alias: the request builder tests each
//! node of the request JSON that way, so the cost grew with the conversation.
//! Parsing a reply also used to build a second, external copy of the parsed
//! value and land it on the heap by name, so the parse cost of a large or
//! streamed output was larger than it needs to be.
//! A counting global allocator measures every byte allocated while the calls
//! run, on every thread, so the bounds do not depend on when the GC runs.

#![allow(
    unsafe_code,
    reason = "a counting global allocator forwards every call to `System`"
)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use baml_tests::engine::compile_source;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// Every byte handed out since the process started.
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// Held for the whole of each test. `ALLOCATED` counts every thread, so a test
/// running beside a measurement (`cargo test` runs this binary's tests on
/// parallel threads; nextest gives each its own process) would add its bytes
/// to that measurement.
static MEASURING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, which
// upholds the `GlobalAlloc` contract; the counter has no effect on memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's contract for `alloc` is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's contract for `alloc_zeroed` is forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATED.fetch_add(new_size.saturating_sub(layout.size()), Ordering::Relaxed);
        // SAFETY: the caller's contract for `realloc` is forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract for `dealloc` is forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Calls before measuring: the first ones pay one-time setup (lazy statics,
/// the HTTP connection pool).
const WARM_UP_CALLS: usize = 3;
/// Calls measured.
const MEASURED_CALLS: usize = 20;
/// The ceiling for one small LLM call. A debug build allocates about 0.7 MB; it
/// allocated about 3.4 MB while failed type tests expanded `json`, and about
/// 13 MB while each parse converted the whole program.
const MAX_BYTES_PER_CALL: usize = 2 * 1024 * 1024;
/// The extra bytes per call that 400 unrelated classes may cost: about 22 KB with
/// the fix, about 6.3 MB before it (each call converted every class twice).
const MAX_UNRELATED_GROWTH_PER_CALL: usize = 256 * 1024;
/// User/assistant exchanges in the long conversation.
const LONG_CONVERSATION_TURNS: usize = 10;
/// Items in the large parsed output, and the characters a stream adds per batch.
const PARSED_ITEMS: i64 = 200;
const STREAM_STEP: usize = 256;
/// The ceiling for parsing a 13 KB reply of 200 items: about 665 KB; about
/// 1.15 MB while each parse built an external copy of the value.
const MAX_FINAL_PARSE_BYTES: usize = 900 * 1024;
/// The ceiling for the same reply streamed in 52 batches, each reparsed from
/// the start: about 32.3 MB; about 45.4 MB with the external copy.
const MAX_STREAMED_PARSE_BYTES: usize = 40 * 1024 * 1024;
/// The extra bytes per call that one more prompt message may cost: about 35 KB,
/// and about 1.56 MB while every failed type test on the request JSON expanded
/// `json`.
const MAX_BYTES_PER_MESSAGE: usize = 128 * 1024;

fn completed_json(text: &str) -> String {
    let text = serde_json::to_string(text).expect("response text is JSON-serializable");
    format!(
        "{{\"status\":\"completed\",\"output\":[{{\"type\":\"message\",\"role\":\"assistant\",\
         \"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\
         \"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}"
    )
}

/// A program with `functions` and a Responses client named `Fake` that a mock
/// server answers with `reply`. Keep the server alive while calling.
async fn engine_for(functions: &str, reply: &str) -> (MockServer, Arc<BexEngine>) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(completed_json(reply)),
        )
        .mount(&server)
        .await;
    let source = format!(
        r#"
client Fake = openai.ResponsesClient.new(model = "fake", api_key = "test-key", base_url = "{base_url}");
{functions}
"#,
        base_url = server.uri()
    );
    (server, engine(&source))
}

/// An engine running `source` with the native sys ops.
fn engine(source: &str) -> Arc<BexEngine> {
    let engine = BexEngine::new_with_runtime_compiler(
        compile_source(source),
        Arc::new(sys_ops::SysOps::native()),
        Vec::new(),
        bex_project::runtime_compiler(),
    )
    .expect("engine builds");
    Arc::new(engine)
}

async fn call(engine: &Arc<BexEngine>, function: &str) -> BexExternalValue {
    engine
        .call_function(
            function,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("the LLM call succeeds")
}

/// The average bytes allocated per call of `function` (a plain function that
/// makes one LLM call: an LLM function's own hidden parameters are not passed
/// here), after warm-up.
async fn bytes_per_call(engine: &Arc<BexEngine>, function: &str) -> usize {
    for _ in 0..WARM_UP_CALLS {
        call(engine, function).await;
    }
    let before = ALLOCATED.load(Ordering::Relaxed);
    for _ in 0..MEASURED_CALLS {
        call(engine, function).await;
    }
    (ALLOCATED.load(Ordering::Relaxed) - before) / MEASURED_CALLS
}

/// `count` classes that nothing in the parse target refers to, each with a few
/// fields and a reference to the one before it.
fn unrelated_classes(count: usize) -> String {
    (0..count)
        .map(|index| {
            let previous = if index == 0 {
                String::new()
            } else {
                format!("\n    previous Unrelated{}?", index - 1)
            };
            format!(
                "class Unrelated{index} {{\n    name string\n    count int\n    tags string[]{previous}\n}}\n"
            )
        })
        .collect()
}

/// A chat program whose prompt holds `turns` user/assistant exchanges before
/// the last question.
fn chat_program(turns: usize) -> String {
    let history: String = (0..turns)
        .map(|turn| {
            format!("${{role(\"user\")}}Question {turn}?${{role(\"assistant\")}}Answer {turn}.")
        })
        .collect();
    format!(
        r#"
function Chat() -> string {{
    client: Fake
    prompt: `${{role("system")}}Be concise.{history}${{role("user")}}Say hi.`
}}

function main() -> string {{
    Chat()
}}
"#
    )
}

/// A program that parses a 13 KB JSON reply of `PARSED_ITEMS` items: whole in
/// `final_parse`, and in `stream_parse` the way `ai.stream.Stream` does, one
/// growing prefix per batch through one parse cache.
fn parse_program() -> String {
    let items: Vec<String> = (0..PARSED_ITEMS)
        .map(|i| format!(r#"{{"id": {i}, "name": "item number {i}", "tags": ["alpha", "beta"]}}"#))
        .collect();
    let text = serde_json::to_string(&format!("[{}]", items.join(", ")))
        .expect("the reply is JSON-serializable");
    format!(
        r#"
class Item {{
    id: int,
    name: string,
    tags: string[],
}}

function final_parse() -> int {{
    baml.sap.parse<Item[]>({text}).length()
}}

function stream_parse() -> int {{
    let text = {text};
    let cache = baml.sap._new_parse_cache<Item[]>();
    let yielded = 0;
    let end = {STREAM_STEP};
    while (end < text.length() + {STREAM_STEP}) {{
        let upto = if (end > text.length()) {{ text.length() }} else {{ end }};
        let parsed: Item[] | baml.sap._NoYield = cache._parse_partial(text.slice(0, upto));
        match (parsed) {{
            let items: Item[] => {{ yielded = yielded + 1; }},
            _ => {{}},
        }};
        end = end + {STREAM_STEP};
    }}
    yielded
}}
"#
    )
}

const PICK_TOOL: &str = r#"
class Choice {
    tool string
}

function PickTool() -> Choice {
    client: Fake
    prompt: `
        Pick the tool to run.
        ${ctx.output_format()}
    `
}

function main() -> string {
    PickTool().tool
}
"#;

#[tokio::test]
async fn call_cost_does_not_grow_with_unrelated_types() {
    let _measuring = MEASURING.lock().await;
    let (_small_server, small) = engine_for(PICK_TOOL, r#"{"tool": "search"}"#).await;
    let large_program = format!("{PICK_TOOL}\n{}", unrelated_classes(400));
    let (_large_server, large) = engine_for(&large_program, r#"{"tool": "search"}"#).await;
    let small_bytes = bytes_per_call(&small, "main").await;
    let large_bytes = bytes_per_call(&large, "main").await;
    let growth = large_bytes.saturating_sub(small_bytes);
    assert!(
        growth <= MAX_UNRELATED_GROWTH_PER_CALL,
        "400 unrelated classes added {growth} bytes per call ({small_bytes} -> {large_bytes}); \
         the parse must convert only the types its target reaches"
    );
}

#[tokio::test]
async fn string_output_call_allocates_little() {
    let _measuring = MEASURING.lock().await;
    let (_server, engine) = engine_for(
        r#"
function SayHi() -> string {
    client: Fake
    prompt: `Say hi.`
}

function main() -> string {
    SayHi()
}
"#,
        "hi",
    )
    .await;
    assert_eq!(
        call(&engine, "main").await,
        BexExternalValue::String("hi".to_string().into())
    );
    let bytes = bytes_per_call(&engine, "main").await;
    assert!(
        bytes <= MAX_BYTES_PER_CALL,
        "one LLM call allocated {bytes} bytes (limit {MAX_BYTES_PER_CALL})"
    );
}

#[tokio::test]
async fn call_cost_does_not_grow_with_conversation_length() {
    let _measuring = MEASURING.lock().await;
    let (_short_server, short) = engine_for(&chat_program(0), "hi").await;
    let (_long_server, long) = engine_for(&chat_program(LONG_CONVERSATION_TURNS), "hi").await;
    let short_bytes = bytes_per_call(&short, "main").await;
    let long_bytes = bytes_per_call(&long, "main").await;
    let extra_messages = 2 * LONG_CONVERSATION_TURNS;
    let per_message = long_bytes.saturating_sub(short_bytes) / extra_messages;
    assert!(
        per_message <= MAX_BYTES_PER_MESSAGE,
        "each prompt message added {per_message} bytes per call ({short_bytes} -> {long_bytes} \
         for {extra_messages} more messages); building the request must stay linear in its size \
         with a small constant"
    );
}

#[tokio::test]
async fn large_parse_allocates_little() {
    let _measuring = MEASURING.lock().await;
    let engine = engine(&parse_program());
    assert_eq!(
        call(&engine, "final_parse").await,
        BexExternalValue::Int(PARSED_ITEMS)
    );
    let bytes = bytes_per_call(&engine, "final_parse").await;
    assert!(
        bytes <= MAX_FINAL_PARSE_BYTES,
        "parsing {PARSED_ITEMS} items allocated {bytes} bytes (limit {MAX_FINAL_PARSE_BYTES})"
    );
}

#[tokio::test]
async fn streamed_parse_allocates_little() {
    let _measuring = MEASURING.lock().await;
    let engine = engine(&parse_program());
    assert!(
        matches!(call(&engine, "stream_parse").await, BexExternalValue::Int(yields) if yields > 0),
        "the stream never yielded a partial value"
    );
    let bytes = bytes_per_call(&engine, "stream_parse").await;
    assert!(
        bytes <= MAX_STREAMED_PARSE_BYTES,
        "streaming {PARSED_ITEMS} items allocated {bytes} bytes (limit {MAX_STREAMED_PARSE_BYTES})"
    );
}

/// The parser drops a `@skip` field, so the types only a skipped field names
/// cost a parse nothing.
#[tokio::test]
async fn call_cost_does_not_grow_with_types_only_skipped_fields_name() {
    let _measuring = MEASURING.lock().await;
    let (_small_server, small) = engine_for(PICK_TOOL, r#"{"tool": "search"}"#).await;
    let skipping_program = format!(
        "{}\n{}",
        unrelated_classes(400),
        PICK_TOOL.replace(
            "class Choice {\n    tool string\n}",
            "class Choice {\n    tool string\n    extra Unrelated399 @skip\n}",
        )
    );
    assert!(skipping_program.contains("extra Unrelated399 @skip"));
    let (_skipping_server, skipping) = engine_for(&skipping_program, r#"{"tool": "search"}"#).await;
    let small_bytes = bytes_per_call(&small, "main").await;
    let skipping_bytes = bytes_per_call(&skipping, "main").await;
    let growth = skipping_bytes.saturating_sub(small_bytes);
    assert!(
        growth <= MAX_UNRELATED_GROWTH_PER_CALL,
        "400 classes a skipped field names added {growth} bytes per call \
         ({small_bytes} -> {skipping_bytes}); the parse must not convert them"
    );
}
