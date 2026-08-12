//! Regression test for BEP-027 §"`baml.argv`": the host can set argv on a
//! constructed engine *after* compilation finishes, so `baml run`'s
//! root-main case can derive `argv[1]` from the file the main function
//! was compiled from. See `BexEngine::set_argv`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;
use sys_types::CallId;

#[tokio::test]
async fn set_argv_updates_what_baml_sys_argv_returns() {
    let snapshot = common::compile_for_engine(
        r#"
            function main() -> string[] {
                baml.sys.argv()
            }
        "#,
    );

    let mut engine = BexEngine::new(
        snapshot,
        sys_native::SysOps::native().into(),
        vec!["/bin/baml".into(), "placeholder".into()],
    )
    .expect("BexEngine::new should succeed");

    // Patch argv as `baml run` does post-compile (see run_command.rs).
    engine.set_argv(vec![
        "/bin/baml".into(),
        "/path/to/main.baml".into(),
        "extra".into(),
    ]);

    let engine = Arc::new(engine);
    let result = engine
        .call_function(
            "user.main",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await
        .expect("call_function should succeed");

    let items = match result {
        BexExternalValue::Array { items, .. } => items,
        other => panic!("expected Array, got {other:?}"),
    };
    let strings: Vec<String> = items
        .into_iter()
        .map(|v| match v {
            BexExternalValue::String(s) => s.to_string(),
            other => panic!("expected String, got {other:?}"),
        })
        .collect();

    assert_eq!(strings.len(), 3);
    assert_eq!(strings[0], "/bin/baml");
    assert_eq!(strings[1], "/path/to/main.baml", "argv[1] = patched path");
    assert_eq!(strings[2], "extra");
}

/// `BexEngine::argv()` exposes the current argv so a host can implement
/// read-modify-write patching (used by the `baml run` script-alias flow,
/// which after resolving an alias replaces `argv[1]` with the
/// post-expansion function name while preserving argv[0] and argv[2+]).
#[test]
fn argv_getter_returns_current_value() {
    let snapshot = common::compile_for_engine("function main() -> int { 1 }");
    let mut engine = BexEngine::new(
        snapshot,
        sys_native::SysOps::native().into(),
        vec!["a".into(), "b".into(), "c".into()],
    )
    .expect("BexEngine::new should succeed");

    assert_eq!(engine.argv(), &["a", "b", "c"]);

    // Round-trip a read-modify-write patch — `argv[1]` is the slot
    // `baml run`'s script-alias flow rewrites post-resolution.
    let mut patched: Vec<String> = engine.argv().to_vec();
    patched[1] = "post-expansion-name".to_string();
    engine.set_argv(patched);

    assert_eq!(engine.argv(), &["a", "post-expansion-name", "c"]);
}
