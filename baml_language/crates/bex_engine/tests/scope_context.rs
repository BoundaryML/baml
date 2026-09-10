//! Scope attribution is observable through captured artifacts, not BAML return values.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, logger::TraceLogger};
use bex_events::{
    ids::BoundaryId,
    prof::backend::{ProfilerConfig, ProfilerSession, StreamReader, ValueState, list_executions},
};
use prost::Message as _;
use sys_native::SysOpsExt as _;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const SOURCE: &str = r#"
function Child(label: string) -> void {
    log.info({ label: label, org_id: "payload-only" }, event_name = label);
}

function Fails() -> int throws string {
    Child("throw");
    throw "expected";
}

function Parent(missing_file: string) -> void {
    Child("before");
    Child("inherited");
    Child("retry", $id = boundary.id().context(
        metadata = { mode: "retry", retries: null, missing: null },
        distinct_id = "job-4"
    ));
    log.debug("after child", event_name = "after");
    (() -> { log.warn("iife", event_name = "iife"); })(
        $id = boundary.id().context(metadata = { mode: "iife" })
    );
    let callback = Child;
    callback("dynamic", $id = boundary.id().context(metadata = { mode: "dynamic" }));
    let _ = Fails($id = boundary.id().context(
        metadata = { mode: "throw" }, distinct_id = "error-task"
    )) catch (e) { _ => 0 };
    Child("caught");
    let _ = baml.env.get("BAML_BEP_074_MISSING", $id = boundary.id().context(
        metadata = { mode: "sysop" }, distinct_id = "sysop-task"
    )) catch (e) { _ => null };
    Child("after_sysop");
    let _ = baml.fs.read(missing_file, $id = boundary.id().context(
        metadata = { mode: "failed_sysop" }, distinct_id = "failed-sysop"
    )) catch (e) { _ => "" };
    Child("after_failed_sysop");
    let pending = (() -> {
        return spawn { log.error("spawned", event_name = "spawned"); };
    })($id = boundary.id().context(
        metadata = { mode: "spawn" }, distinct_id = "spawn-task"
    ));
    let _ = await pending;
    Child("after_spawn");
}

function main(missing_file: string) -> void {
    let metadata: map<string, string | int | float | bool | null> = {
        org_id: "org-2", mode: "normal", retries: 3, ratio: 1.25, active: true
    };
    let id = boundary.id().context(metadata = metadata)
        .context(distinct_id = "user-7")
        .capture(inputs = false, output = false, error = false);
    metadata["mode"] = "mutated";
    metadata["new"] = "later";
    Parent(missing_file, $id = id);
    Child("outside");
    log.info("unnamed");
    log.debug("explicitly unnamed", event_name = null);
}
"#;

fn map_bytes(entries: BTreeMap<String, BexExternalValue>) -> Vec<u8> {
    use bridge_ctypes::baml_bridge::cffi::{BamlOutboundValue, baml_outbound_value};
    let bytes = bridge_ctypes::artifact_safe_outbound_bytes(&BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::unknown(),
        entries: entries.into_iter().collect(),
    })
    .unwrap();
    let mut value = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
    let Some(baml_outbound_value::Value::MapValue(map)) = &mut value.value else {
        panic!("map expected")
    };
    // Profiler snapshots omit the SDK's optional container type annotations.
    map.key_type = None;
    map.value_type = None;
    value.encode_to_vec()
}

fn scope_bytes(mode: &str, retries: bool) -> Vec<u8> {
    let mut entries = BTreeMap::from([
        (
            "org_id".to_owned(),
            BexExternalValue::String("org-2".into()),
        ),
        ("mode".to_owned(), BexExternalValue::String(mode.into())),
        ("ratio".to_owned(), BexExternalValue::Float(1.25)),
        ("active".to_owned(), BexExternalValue::Bool(true)),
    ]);
    if retries {
        entries.insert("retries".to_owned(), BexExternalValue::Int(3));
    }
    map_bytes(entries)
}

async fn exercise(profiling: bool, history: bool) {
    let _guard = TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join(".baml/profiles-v1");
    let config = ProfilerConfig {
        enabled: profiling,
        store_root: root.clone(),
        process_memory_bytes: 32 * 1024 * 1024,
        publish_interval: Duration::MAX,
        ..ProfilerConfig::default()
    };
    let (session, diagnostic) = if history {
        ProfilerSession::from_config_with_log_history(config)
    } else {
        ProfilerSession::from_config(config)
    };
    assert!(diagnostic.is_none(), "{diagnostic:?}");
    let logger = TraceLogger::bounded(64);
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            baml_tests::stdlib_prefix::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            session,
        )
        .unwrap(),
    );
    let boundary = BoundaryId::new_random();
    engine
        .call_function(
            "main",
            vec![BexExternalValue::String(
                temp.path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned()
                    .into(),
            )],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary)
                .with_logger(logger.clone())
                .build(),
            true,
        )
        .await
        .unwrap();
    let captured = logger.drain_encoded_logs();
    assert!(captured.failures.is_empty(), "{:?}", captured.failures);
    let find = |name: &str| {
        captured
            .logs
            .iter()
            .find(|log| log.event.event_name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("missing {name} log"))
    };
    for label in [
        "before",
        "inherited",
        "after",
        "caught",
        "after_sysop",
        "after_failed_sysop",
        "after_spawn",
    ] {
        let log = find(label);
        assert_eq!(log.event.distinct_id.as_deref(), Some("user-7"), "{label}");
        assert_eq!(
            log.context.as_ref(),
            Some(&scope_bytes("normal", true)),
            "{label}"
        );
    }
    for (label, id, mode, retries) in [
        ("retry", "job-4", "retry", false),
        ("iife", "user-7", "iife", true),
        ("dynamic", "user-7", "dynamic", true),
        ("throw", "error-task", "throw", true),
        ("spawned", "spawn-task", "spawn", true),
    ] {
        let log = find(label);
        assert_eq!(log.event.distinct_id.as_deref(), Some(id), "{label}");
        assert_eq!(
            log.context.as_ref(),
            Some(&scope_bytes(mode, retries)),
            "{label}"
        );
    }
    assert_eq!(find("outside").event.distinct_id, None);
    assert_eq!(
        find("outside").context.as_ref(),
        Some(&map_bytes(BTreeMap::new()))
    );
    assert_eq!(
        captured
            .logs
            .iter()
            .filter(|log| log.event.event_name.is_none())
            .count(),
        2
    );
    assert_eq!(
        find("retry").body,
        map_bytes(BTreeMap::from([
            ("label".to_owned(), BexExternalValue::String("retry".into())),
            (
                "org_id".to_owned(),
                BexExternalValue::String("payload-only".into())
            ),
        ]))
    );

    if profiling || history {
        assert!(bex_events::prof::flush_and_join(Duration::from_secs(5)));
        let summary = list_executions(&root)
            .unwrap()
            .into_iter()
            .find(|summary| summary.runtime_id == Some(boundary))
            .unwrap();
        let reader = StreamReader::open(&root, summary.stream)
            .unwrap()
            .execution(summary.id)
            .unwrap();
        let profile = reader.load().unwrap();
        assert_eq!(profile.logs.len(), captured.logs.len());
        for log in &profile.logs {
            let ValueState::Available { cid, .. } = log.context else {
                panic!("scope unavailable: {:?}", log.context)
            };
            assert!(!reader.read_value(cid).unwrap().body.is_empty());
        }
        if profiling {
            assert!(!profile.spans.is_empty());
            for span in profile.spans.values() {
                assert!(span.scope.is_some(), "selected call has no scope: {span:?}");
            }
            let scopes = profile
                .spans
                .values()
                .filter_map(|span| span.scope.as_ref())
                .collect::<Vec<_>>();
            for (id, mode) in [("sysop-task", "sysop"), ("failed-sysop", "failed_sysop")] {
                let scope = scopes
                    .iter()
                    .find(|scope| scope.distinct_id.as_deref() == Some(id))
                    .unwrap_or_else(|| panic!("missing selected {id} scope"));
                let ValueState::Available { cid, .. } = scope.context else {
                    panic!("scope unavailable")
                };
                assert_eq!(
                    reader.read_value(cid).unwrap().body,
                    scope_bytes(mode, true)
                );
            }
            assert!(
                !profile
                    .spans
                    .contains_key(&find("inherited").call.call_ref())
            );
        } else {
            assert!(profile.spans.is_empty());
        }
    } else {
        assert!(!root.exists(), "memory-only scopes must not create history");
    }
    drop(engine);
    assert!(bex_events::prof::drain_logs(Duration::from_secs(5)));
}

#[tokio::test]
async fn scope_context_without_profiling_is_memory_only() {
    exercise(false, false).await;
}

#[tokio::test]
async fn scope_context_in_history_only_mode() {
    exercise(false, true).await;
}

#[tokio::test]
async fn retained_calls_and_logs_store_effective_scope() {
    exercise(true, false).await;
}
