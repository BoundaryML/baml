//! `string.slice` must not scan the text from its start: on a long ASCII
//! string, slicing near the end costs the same as slicing near the start.
//! A scan from the start (the generic body once did `char_indices().nth`)
//! made k-nucleotide quadratic; this pins the constant per-slice cost by
//! comparing the two ends, so it does not depend on the machine's speed.

mod common;

use std::{sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
function make_text(n: int) -> string {
  let parts: string[] = [];
  for (let i = 0; i < n; i += 1) { parts.push("acgt"); }
  parts.join("")
}
function slice_run(text: string, from: int, count: int) -> int {
  let total = 0;
  for (let i = from; i < from + count; i += 1) {
    total += text.slice(i, i + 3).length();
  }
  total
}
"#;

const CHARS: i64 = 200_000;
const SLICES: i64 = 10_000;

fn ctx() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

async fn time_slices(engine: &Arc<BexEngine>, text: &BexExternalValue, from: i64) -> Duration {
    let start = std::time::Instant::now();
    engine
        .call_function(
            "slice_run",
            vec![text.clone(), BexExternalValue::Int(from), BexExternalValue::Int(SLICES)],
            ctx(),
            false,
        )
        .await
        .unwrap();
    start.elapsed()
}

#[tokio::test]
async fn slicing_near_the_end_costs_what_slicing_near_the_start_does() {
    let engine = Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );
    let text = engine
        .call_function("make_text", vec![BexExternalValue::Int(CHARS / 4)], ctx(), false)
        .await
        .unwrap();
    // Warm both paths once, then take the faster of three runs at each end.
    let mut front = Duration::MAX;
    let mut back = Duration::MAX;
    for _ in 0..3 {
        front = front.min(time_slices(&engine, &text, 0).await);
        back = back.min(time_slices(&engine, &text, CHARS - SLICES - 3).await);
    }
    // A scan from the start would make the back about 38x the front.
    assert!(
        back <= front * 4 + Duration::from_millis(50),
        "slicing near the end took {back:?}, near the start {front:?}"
    );
}
