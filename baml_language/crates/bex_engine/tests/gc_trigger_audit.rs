//! Characterization of the current native trigger policy, not desired behavior.
//! Run explicitly with `--features gc_profiling -- --ignored --nocapture`.
#![cfg(feature = "gc_profiling")]
#![expect(
    clippy::print_stdout,
    reason = "Manual characterizations emit machine-readable results to the runner"
)]

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_heap::{BexHeap, CollectionLevel, Tlab};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Node { value int }
function Make() -> Node { Node { value: 1 } }
function Churn(n: int) -> int {
    let i = 0;
    let sum = 0;
    while (i < n) {
        let node = Node { value: i };
        sum = sum + node.value;
        i = i + 1;
    }
    sum
}
function Yield() -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(0n));
    1
}
"#;

async fn call(
    engine: &Arc<BexEngine>,
    name: &str,
    args: Vec<BexExternalValue>,
) -> BexExternalValue {
    engine
        .call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .suppress_internal_profile()
                .build(),
            true,
        )
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "records existing scheduling gaps; update when trigger policy changes"]
async fn characterize_engine_gc_checkpoints() {
    let program = baml_db::testing::compile_source(SOURCE);
    let mut observations = Vec::new();
    for scenario in [
        "sparse_short_calls",
        "short_calls_over_threshold",
        "one_compute_call",
    ] {
        let engine = Arc::new(
            BexEngine::new(
                program.clone(),
                Arc::new(sys_native::SysOps::native()),
                vec![],
            )
            .unwrap(),
        );
        engine.collect_garbage(CollectionLevel::Major).await;
        match scenario {
            "sparse_short_calls" => {
                for _ in 0..512 {
                    drop(call(&engine, "Make", vec![]).await);
                }
                assert_eq!(engine.heap().should_collect(), None);
                assert_eq!(engine.heap_stats().runtime_objects, 512 * 1024);
            }
            "short_calls_over_threshold" => {
                for _ in 0..100 {
                    assert_eq!(
                        call(&engine, "Churn", vec![BexExternalValue::Int(128)]).await,
                        BexExternalValue::Int(128 * 127 / 2)
                    );
                }
                assert_eq!(engine.heap().should_collect(), Some(CollectionLevel::Minor));
            }
            _ => {
                assert_eq!(
                    call(&engine, "Churn", vec![BexExternalValue::Int(20_000)]).await,
                    BexExternalValue::Int(20_000 * 19_999 / 2)
                );
                assert_eq!(engine.heap().should_collect(), Some(CollectionLevel::Minor));
            }
        }
        let slots_before = engine.heap_stats().runtime_objects;
        let decision_before = format!("{:?}", engine.heap().should_collect());
        // This bounded observation supplements the source audit; it does not
        // by itself prove the absence of every possible timer.
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        assert_eq!(engine.heap_stats().runtime_objects, slots_before);
        assert_eq!(
            call(&engine, "Yield", vec![]).await,
            BexExternalValue::Int(1)
        );
        let slots_after_yield = engine.heap_stats().runtime_objects;
        if scenario == "sparse_short_calls" {
            assert!(slots_after_yield >= slots_before);
        } else {
            assert!(slots_after_yield < slots_before);
            assert_eq!(engine.heap().should_collect(), None);
        }
        // Inspect the post-yield generations under exclusive GC access.
        let cleanup = engine.collect_garbage(CollectionLevel::Major).await;
        observations.push(serde_json::json!({
            "scenario": scenario, "decision_before_yield":decision_before,
            "slots_before_yield":slots_before, "slots_after_yield":slots_after_yield,
            "post_yield_generations":cleanup.profile.before.generation_slots,
            "allocations_since_gc_after_yield":cleanup.profile.before.new_objects,
        }));
        engine.shutdown().await;
    }
    println!(
        "GC_TRIGGER_AUDIT {}",
        serde_json::to_string(&observations).unwrap()
    );
}

#[test]
#[ignore = "records the moving old-generation threshold; update when policy changes"]
#[expect(
    unsafe_code,
    reason = "This standalone heap has exclusive single-threaded access during collection"
)]
fn characterize_automatic_level_selection_under_promotion() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new_empty(heap.clone());
    let mut roots = Vec::new();
    let mut observations = Vec::new();
    for round in 0..12 {
        for _ in 0..10_000 {
            roots.push(tlab.alloc_float(1.0));
        }
        // Retain only the newest two batches. Older promoted objects become
        // garbage, but a minor collection cannot reclaim existing Gen2.
        if roots.len() > 20_000 {
            roots.drain(..roots.len() - 20_000);
        }
        let selected = heap.should_collect().expect("allocation pressure");
        assert_eq!(selected, CollectionLevel::Minor);
        // SAFETY: standalone heap, single thread; all live pointers are roots.
        let (stats, remapped, _) = unsafe { heap.collect_garbage_generational(&roots, selected) };
        tlab.invalidate();
        roots = remapped;
        assert_eq!(heap.should_collect(), None);
        observations.push(serde_json::json!({"round":round+1,
            "selected":format!("{:?}", selected), "retained_roots":roots.len(),
            "generations_after":stats.profile.after.generation_slots,
            "decision_after":format!("{:?}", heap.should_collect()),
        }));
    }
    roots.clear();
    for _ in 0..10_000 {
        tlab.alloc_float(0.0);
    }
    assert_eq!(heap.should_collect(), Some(CollectionLevel::Minor));
    // SAFETY: no live roots remain, no concurrent heap access.
    let (minor, _, _) = unsafe { heap.collect_garbage_minor(&[]) };
    tlab.invalidate();
    assert_eq!(minor.profile.after.generation_slots, [0, 0, 110_000]);
    assert_eq!(heap.should_collect(), None);
    // SAFETY: no live roots remain, no concurrent heap access.
    let (major, _, _) = unsafe { heap.collect_garbage(&[]) };
    assert_eq!(major.profile.after.generation_slots, [0, 0, 0]);
    println!(
        "GC_PROMOTION_AUDIT {}",
        serde_json::json!({
            "cycles":observations,
            "unreachable_gen2_after_automatic_level":minor.profile.after.generation_slots[2],
            "gen2_after_explicit_major":major.profile.after.generation_slots[2],
        })
    );
}
