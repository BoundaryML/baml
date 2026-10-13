//! Opt-in release measurement against ordinary direct and indirect calls.
#![expect(clippy::print_stdout, reason = "explicit benchmark results")]
mod common;
use std::{sync::Arc, time::Instant};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;
const SOURCE: &str = r#"
function baseline(n: int) -> int { n }
/// baml:$trace=trace.hidden
function hidden(n: int) -> int { n }
/// baml:$trace=trace.empty_span
function rich(n: int) -> int { n }
/// baml:$trace=trace.span
function span(n: int) -> int { n }
/// baml:$trace=trace.timing
function timing(n: int) -> int { n }
function policy(n: int, settings: trace.Settings) -> trace.Options throws never { trace.hidden() }
/// baml:$trace=policy
function aware(n: int) -> int { n }
function direct_baseline(n: int) -> int { let i = 0; let sum = 0; while (i < n) { sum += baseline(i); i += 1; } sum }
function direct_hidden(n: int) -> int { let i = 0; let sum = 0; while (i < n) { sum += hidden(i); i += 1; } sum }
function direct_rich(n: int) -> int { let i = 0; let sum = 0; while (i < n) { sum += rich(i); i += 1; } sum }
function direct_span(n: int) -> int { let i = 0; let sum = 0; while (i < n) { sum += span(i); i += 1; } sum }
function direct_timing(n: int) -> int { let i = 0; let sum = 0; while (i < n) { sum += timing(i); i += 1; } sum }
function direct_aware(n: int) -> int { let i = 0; let sum = 0; while (i < n) { sum += aware(i); i += 1; } sum }
function indirect_loop(n: int, callback: (int) -> int) -> int { let i = 0; let sum = 0; while (i < n) { sum += callback(i); i += 1; } sum }
function indirect_baseline(n: int) -> int { indirect_loop(n, baseline) }
function indirect_hidden(n: int) -> int { indirect_loop(n, hidden) }
"#;
#[tokio::test]
#[ignore = "run --release --ignored --nocapture with BAML_TELEMETRY=off or high"]
#[expect(
    clippy::assertions_on_constants,
    reason = "explicit release benchmark guard"
)]
async fn trace_hook_performance() {
    assert!(!cfg!(debug_assertions), "release measurements only");
    let engine = Arc::new(
        BexEngine::new(
            common::compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    );
    let iterations = 20_000;
    let expected = BexExternalValue::Int(iterations * (iterations - 1) / 2);
    for name in [
        "direct_baseline",
        "direct_hidden",
        "direct_rich",
        "direct_span",
        "direct_timing",
        "direct_aware",
        "indirect_baseline",
        "indirect_hidden",
    ] {
        let mut samples = Vec::new();
        for trial in 0..7 {
            let start = Instant::now();
            let result = engine
                .call_function(
                    name,
                    vec![BexExternalValue::Int(iterations)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
                .unwrap();
            assert_eq!(result, expected);
            if trial > 1 {
                samples.push(start.elapsed().as_secs_f64() * 1_000_000_000.0 / 20_000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "TRACE_HOOK_PERF {name} ns_per_call={:.1} samples={samples:?}",
            samples[2]
        );
    }
    engine.shutdown().await;
}
