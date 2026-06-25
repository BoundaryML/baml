//! BEP-042 Commit 2: GC-triggered `cleanup` finalization.
//!
//! When an instance of a class with a `cleanup(self) -> void` method becomes
//! unreachable, the collector keeps it alive for the collection, runs its
//! `cleanup`, and reclaims it on a later cycle. These tests force a collection
//! (`engine.collect_garbage`) and observe the finalizer's side effect through a
//! shared array the test roots with a handle.

mod common;

use std::sync::Arc;

use ::bex_heap::CollectionLevel;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

// `Resource.cleanup` appends a marker to its `log` field. The test roots the
// log array (a handle keeps it alive across GC) and aliases it into a `Resource`
// that is left to become garbage, so the number of markers observed after a
// collection is exactly how many times `cleanup` ran.
const SOURCE: &str = r#"
class Resource {
  log string[]
  function cleanup(self) -> void {
    self.log.push("cleaned")
  }
}
function make_log() -> string[] { [] }
function make_resource(log: string[]) -> Resource { Resource { log: log } }
function clean(r: Resource) -> void { r.cleanup() }
function read_log(log: string[]) -> string[] { log }
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    )
}

fn ctx() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[track_caller]
fn expect_strings(v: &BexExternalValue) -> Vec<String> {
    match v {
        BexExternalValue::Array { items, .. } => items
            .iter()
            .map(|it| match it {
                BexExternalValue::String(s) => s.to_string(),
                other => panic!("expected String element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected Array, got {other:?}"),
    }
}

#[tokio::test]
async fn cleanup_runs_when_instance_is_collected() {
    let engine = engine();

    // A handle to a shared log array — keeps it alive (a GC root) for the test.
    let log = engine
        .call_function("make_log", vec![], ctx(), /* copy_objects = */ false)
        .await
        .unwrap();
    assert!(
        matches!(log, BexExternalValue::Handle(_)),
        "expected a Handle"
    );

    // Create a `Resource` aliasing the log; hold it by handle so it is a real,
    // rooted allocation (not optimized away).
    let resource = engine
        .call_function("make_resource", vec![log.clone()], ctx(), false)
        .await
        .unwrap();
    assert!(matches!(resource, BexExternalValue::Handle(_)));

    // Before any collection, the finalizer has not run.
    let before = engine
        .call_function("read_log", vec![log.clone()], ctx(), true)
        .await
        .unwrap();
    assert_eq!(expect_strings(&before), Vec::<String>::new());

    // Drop the only root to the `Resource` — it is now garbage (the log survives
    // via the test's separate handle).
    drop(resource);

    // Collect: the unreachable `Resource` is finalized, pushing to the log.
    engine.collect_garbage(CollectionLevel::Major).await;

    let after = engine
        .call_function("read_log", vec![log], ctx(), true)
        .await
        .unwrap();
    assert_eq!(expect_strings(&after), vec!["cleaned"]);
}

#[tokio::test]
async fn gc_does_not_rerun_cleanup_already_done_explicitly() {
    // SuppressFinalize: an instance cleaned explicitly (latch set) before it
    // becomes garbage is NOT finalized again by the GC — the marker count stays
    // at 1, not 2.
    let engine = engine();

    let log = engine
        .call_function("make_log", vec![], ctx(), false)
        .await
        .unwrap();

    let resource = engine
        .call_function("make_resource", vec![log.clone()], ctx(), false)
        .await
        .unwrap();

    // Clean it explicitly (sets the run-once latch), pushing one marker.
    engine
        .call_function("clean", vec![resource.clone()], ctx(), false)
        .await
        .unwrap();
    let before = engine
        .call_function("read_log", vec![log.clone()], ctx(), true)
        .await
        .unwrap();
    assert_eq!(expect_strings(&before), vec!["cleaned"]);

    // Drop the root, then GC must skip the already-cleaned instance — no second
    // marker.
    drop(resource);
    engine.collect_garbage(CollectionLevel::Major).await;

    let after = engine
        .call_function("read_log", vec![log], ctx(), true)
        .await
        .unwrap();
    assert_eq!(expect_strings(&after), vec!["cleaned"]);
}
