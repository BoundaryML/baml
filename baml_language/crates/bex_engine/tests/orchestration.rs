//! Integration tests for LLM orchestration plan building.
//!
//! These tests verify that `baml.llm.build_plan` correctly expands client trees
//! (primitive, fallback, round-robin) into flat lists of `OrchestrationStep`s,
//! and that `baml.llm.wrap_with_retry` applies the correct retry logic with
//! exponential backoff delays.

mod common;

use bex_engine::{BexEngine, BexExternalValue};
use sys_native::SysOpsExt;

/// Helper: compile source, create engine, call function, return result.
async fn run(source: &str, entry: &str) -> Result<BexExternalValue, bex_engine::EngineError> {
    let snapshot = common::compile_for_engine(source);
    let engine =
        BexEngine::new(snapshot, sys_types::SysOps::native()).expect("Failed to create engine");
    engine.call_function(entry, vec![], None, &[]).await
}

/// Extract `(client_name, delay_ms)` tuples from a plan result.
///
/// Validates that the result is an array of `OrchestrationStep` instances,
/// each containing a `PrimitiveClient` instance with a `name` field and
/// an integer `delay_ms` field.
fn extract_steps(result: &BexExternalValue) -> Vec<(&str, i64)> {
    let BexExternalValue::Array { items, .. } = result else {
        panic!("expected Array, got {result:?}");
    };

    items
        .iter()
        .map(|step| {
            let BexExternalValue::Instance { class_name, fields } = step else {
                panic!("expected OrchestrationStep Instance, got {step:?}");
            };
            assert_eq!(
                class_name, "baml.llm.OrchestrationStep",
                "expected baml.llm.OrchestrationStep, got {class_name}"
            );

            let delay = match fields.get("delay_ms") {
                Some(BexExternalValue::Int(d)) => *d,
                other => panic!("expected Int for delay_ms, got {other:?}"),
            };

            let client_name = match fields.get("primitive_client") {
                Some(BexExternalValue::Instance {
                    class_name: pc_class,
                    fields: pc_fields,
                }) => {
                    assert_eq!(
                        pc_class, "baml.llm.PrimitiveClient",
                        "expected baml.llm.PrimitiveClient, got {pc_class}"
                    );
                    match pc_fields.get("name") {
                        Some(BexExternalValue::String(s)) => s.as_str(),
                        other => panic!("expected String for primitive_client.name, got {other:?}"),
                    }
                }
                other => panic!("expected PrimitiveClient Instance, got {other:?}"),
            };

            (client_name, delay)
        })
        .collect()
}

// ============================================================================
// build_plan: plan shape (number of steps)
// ============================================================================

/// A primitive client produces a single-step plan.
#[tokio::test]
async fn plan_primitive_has_one_step() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

function F(x: string) -> string {
    client A
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(extract_steps(&result), vec![("A", 0)]);
}

/// A fallback client with two sub-clients produces two steps.
#[tokio::test]
async fn plan_fallback_has_two_steps() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> FB {
    provider fallback
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(extract_steps(&result), vec![("A", 0), ("B", 0)]);
}

/// A fallback client with three sub-clients produces three steps.
#[tokio::test]
async fn plan_fallback_three_clients() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> C {
    provider openai
    options { model "gpt-4o" }
}

client<llm> FB {
    provider fallback
    options { strategy [A, B, C] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(extract_steps(&result), vec![("A", 0), ("B", 0), ("C", 0)]);
}

/// A round-robin client with two sub-clients produces a single step
/// (it picks one sub-client per invocation).
#[tokio::test]
async fn plan_round_robin_has_one_step() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> RR {
    provider round-robin
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client RR
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    let steps = extract_steps(&result);
    // Round-robin picks one sub-client; first call picks A (index 0 % 2 = 0)
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].1, 0); // delay is 0
    assert!(steps[0].0 == "A" || steps[0].0 == "B");
}

/// Round-robin honors `options { start N }` for the initial selection.
#[tokio::test]
async fn plan_round_robin_respects_start_index() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> RR {
    provider round-robin
    options { strategy [A, B] start 1 }
}

function F(x: string) -> string {
    client RR
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    let steps = extract_steps(&result);
    assert_eq!(steps, vec![("B", 0)]);
}

/// Calling `build_plan` must not advance runtime RR counters.
///
/// Planner expansion should be side-effect free; only execution should mutate
/// the global round-robin counter.
#[tokio::test]
async fn plan_round_robin_has_no_runtime_side_effects() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> RR {
    provider round-robin
    options { strategy [A, B] start 0 }
}

function F(x: string) -> string {
    client RR
    prompt #"{{ x }}"#
}

function check_plan_side_effects() -> int {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c);
    baml.llm.build_plan(c);
    baml.llm.round_robin_next("RR")
}
"##;

    let result = run(source, "check_plan_side_effects")
        .await
        .expect("test failed");

    assert_eq!(result, BexExternalValue::Int(0));
}

// ============================================================================
// build_plan: retry expands steps
// ============================================================================

/// A primitive client with retry(max=2) produces 3 steps (1 original + 2 retries).
#[tokio::test]
async fn plan_primitive_with_retry_expands() {
    let source = r##"
retry_policy Retry2 {
    max_retries 2
    initial_delay_ms 100
    multiplier 2
    max_delay_ms 500
}

client<llm> A {
    provider openai
    retry_policy Retry2
    options { model "gpt-4" }
}

function F(x: string) -> string {
    client A
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(
        extract_steps(&result),
        vec![("A", 0), ("A", 100), ("A", 200)]
    );
}

/// A fallback[A, B] with retry(max=1) produces 4 steps:
/// attempt 0: [A, B], attempt 1: [A, B]
#[tokio::test]
async fn plan_fallback_with_retry_multiplies() {
    let source = r##"
retry_policy Retry1 {
    max_retries 1
    initial_delay_ms 200
}

client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> FB {
    provider fallback
    retry_policy Retry1
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    // 2 sub-clients x (1 original + 1 retry) = 4 steps
    assert_eq!(
        extract_steps(&result),
        vec![("A", 0), ("B", 0), ("A", 200), ("B", 200)]
    );
}

// ============================================================================
// build_plan: delay values
// ============================================================================

/// First step always has `delay_ms=0`, retry steps have exponential backoff.
#[tokio::test]
async fn plan_delays_exponential_backoff() {
    let source = r##"
retry_policy ExpBackoff {
    max_retries 3
    initial_delay_ms 100
    multiplier 2
    max_delay_ms 1000
}

client<llm> A {
    provider openai
    retry_policy ExpBackoff
    options { model "gpt-4" }
}

function F(x: string) -> string {
    client A
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    // 4 steps: original(0) + retry1(100) + retry2(200) + retry3(400)
    assert_eq!(
        extract_steps(&result),
        vec![("A", 0), ("A", 100), ("A", 200), ("A", 400)]
    );
}

/// Delays are capped at `max_delay_ms`.
#[tokio::test]
async fn plan_delays_capped_at_max() {
    let source = r##"
retry_policy CappedBackoff {
    max_retries 4
    initial_delay_ms 100
    multiplier 3
    max_delay_ms 500
}

client<llm> A {
    provider openai
    retry_policy CappedBackoff
    options { model "gpt-4" }
}

function F(x: string) -> string {
    client A
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    // 5 steps: 0, 100, 300, 500(capped), 500(capped)
    assert_eq!(
        extract_steps(&result),
        vec![("A", 0), ("A", 100), ("A", 300), ("A", 500), ("A", 500)]
    );
}

/// Fallback with retry: delays apply uniformly to all sub-client steps in a retry attempt.
#[tokio::test]
async fn plan_fallback_retry_delays() {
    let source = r##"
retry_policy R {
    max_retries 1
    initial_delay_ms 200
}

client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> FB {
    provider fallback
    retry_policy R
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    // attempt 0: [A(0), B(0)], attempt 1: [A(200), B(200)]
    assert_eq!(
        extract_steps(&result),
        vec![("A", 0), ("B", 0), ("A", 200), ("B", 200)]
    );
}

// ============================================================================
// build_plan: no retry (null retry policy)
// ============================================================================

/// A client without `retry_policy` produces steps with all delays = 0.
#[tokio::test]
async fn plan_no_retry_all_zero_delays() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> FB {
    provider fallback
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(extract_steps(&result), vec![("A", 0), ("B", 0)]);
}

// ============================================================================
// build_plan: nested composition
// ============================================================================

/// `Fallback[RoundRobin[A, B], C]` produces 2 steps: one from RR (picks one) + C.
#[tokio::test]
async fn plan_nested_fallback_round_robin() {
    let source = r##"
client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> RR {
    provider round-robin
    options { strategy [A, B] }
}

client<llm> C {
    provider openai
    options { model "gpt-4o" }
}

client<llm> FB {
    provider fallback
    options { strategy [RR, C] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    let steps = extract_steps(&result);
    // RR picks 1 sub-client, then C = 2 steps total
    assert_eq!(steps.len(), 2);
    assert!(steps[0].0 == "A" || steps[0].0 == "B");
    assert_eq!(steps[0].1, 0);
    assert_eq!(steps[1], ("C", 0));
}

/// Nested retry: inner client has retry, outer fallback does not.
/// `Fallback[A(retry=1), B]` = A, A(retry), B = 3 steps.
#[tokio::test]
async fn plan_nested_inner_retry() {
    let source = r##"
retry_policy InnerRetry {
    max_retries 1
    initial_delay_ms 50
}

client<llm> A {
    provider openai
    retry_policy InnerRetry
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> FB {
    provider fallback
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    // A(original) + A(retry) + B = 3 steps
    assert_eq!(extract_steps(&result), vec![("A", 0), ("A", 50), ("B", 0)]);
}

// ============================================================================
// build_plan: client names in steps
// ============================================================================

/// Verify that the correct primitive clients appear in a fallback plan.
#[tokio::test]
async fn plan_step_client_names() {
    let source = r##"
client<llm> Primary {
    provider openai
    options { model "gpt-4" }
}

client<llm> Backup {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> FB {
    provider fallback
    options { strategy [Primary, Backup] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(extract_steps(&result), vec![("Primary", 0), ("Backup", 0)]);
}

/// Retry duplicates client names: [A, A, A] for retry=2.
#[tokio::test]
async fn plan_retry_duplicates_client_names() {
    let source = r##"
retry_policy R {
    max_retries 2
}

client<llm> A {
    provider openai
    retry_policy R
    options { model "gpt-4" }
}

function F(x: string) -> string {
    client A
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    assert_eq!(extract_steps(&result), vec![("A", 0), ("A", 0), ("A", 0)]);
}

// ============================================================================
// build_plan: round-robin + retry rotation
// ============================================================================

/// Round-robin with retry rotates across children on each retry attempt.
///
/// RR { A, B } with retry=1 should produce [A(0), B(delay)] — NOT [A(0), A(delay)].
/// This matches legacy behavior where retry re-expands the strategy.
#[tokio::test]
async fn plan_round_robin_retry_rotates() {
    let source = r##"
retry_policy R {
    max_retries 1
    initial_delay_ms 100
}

client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> RR {
    provider round-robin
    retry_policy R
    options { strategy [A, B] }
}

function F(x: string) -> string {
    client RR
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    let steps = extract_steps(&result);
    // 2 attempts: attempt 0 picks one child, attempt 1 picks the other
    assert_eq!(steps.len(), 2);
    assert_ne!(
        steps[0].0, steps[1].0,
        "retry should pick a different RR child"
    );
    assert_eq!(steps[0].1, 0); // first attempt: no delay
    assert_eq!(steps[1].1, 100); // second attempt: 100ms delay
}

/// Fallback with RR child + retry: RR rotates across the fallback's retry attempts.
///
/// Fallback(retry=1) { RR { A, B }, C } should produce:
///   attempt 0: [RR→A, C], attempt 1: [RR→B, C]
#[tokio::test]
async fn plan_fallback_with_rr_child_retry_rotates() {
    let source = r##"
retry_policy R {
    max_retries 1
    initial_delay_ms 200
}

client<llm> A {
    provider openai
    options { model "gpt-4" }
}

client<llm> B {
    provider openai
    options { model "gpt-3.5-turbo" }
}

client<llm> RR {
    provider round-robin
    options { strategy [A, B] }
}

client<llm> C {
    provider openai
    options { model "gpt-4o" }
}

client<llm> FB {
    provider fallback
    retry_policy R
    options { strategy [RR, C] }
}

function F(x: string) -> string {
    client FB
    prompt #"{{ x }}"#
}

function check_plan() -> baml.llm.OrchestrationStep[] {
    let c = baml.llm.get_client("F");
    baml.llm.build_plan(c)
}
"##;

    let result = run(source, "check_plan").await.expect("test failed");
    let steps = extract_steps(&result);
    // 4 steps: 2 attempts x (1 RR pick + C)
    assert_eq!(steps.len(), 4);
    // Attempt 0: RR picks one child, then C
    assert_eq!(steps[0].1, 0);
    assert_eq!(steps[1], ("C", 0));
    // Attempt 1: RR picks different child, then C
    assert_eq!(steps[2].1, 200);
    assert_eq!(steps[3], ("C", 200));
    // RR should have rotated
    assert_ne!(steps[0].0, steps[2].0, "retry should rotate the RR child");
}
