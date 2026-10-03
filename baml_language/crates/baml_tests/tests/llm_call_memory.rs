//! The bytes the runtime allocates for one LLM call stay small.
//!
//! A Rust test because BAML cannot observe allocations. Each parse used to build
//! a schema-aligned-parsing model of every type in the program, the standard
//! library included: about 5 MB per call for a one-function program, kept alive
//! until the VM collected it. And every `match` type test that failed
//! canonicalized the recursive `json` alias: the request builder tests each
//! node of the request JSON that way, so the cost grew with the conversation.
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
    let engine = BexEngine::new_with_runtime_compiler(
        compile_source(&source),
        Arc::new(sys_ops::SysOps::native()),
        Vec::new(),
        bex_project::runtime_compiler(),
    )
    .expect("engine builds");
    (server, Arc::new(engine))
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
