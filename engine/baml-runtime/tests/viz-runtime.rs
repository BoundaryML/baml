use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use baml_compiler::watch::{
    shared_handler, SharedWatchHandler, VizExecDelta, VizExecEvent, WatchBamlValue,
    WatchNotification,
};
use baml_runtime::{FunctionResult, RuntimeContextManager, TripWire};
use baml_viz_events::{Frame, LexicalState, StateUpdate, VizStateReducer};
use internal_baml_core::feature_flags::FeatureFlags;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
struct EventRecord {
    kind: String,
    header: Option<HeaderEvent>,
    viz_event: Option<VizExecEvent>,
}

#[derive(Debug, Clone, Serialize)]
struct HeaderEvent {
    level: u8,
    title: String,
}

#[derive(Debug, Clone, Serialize)]
struct StreamSnapshot {
    watch_event: EventRecord,
    stack_after: Vec<String>,
    emitted_events: Vec<StateUpdate>,
}

#[derive(Debug, Serialize)]
struct FixtureSnapshot {
    fixture: String,
    snapshots: Vec<StreamSnapshot>,
}

#[test]
fn viz_runtime_snapshots() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let snapshot_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("viz-runtime")
        .join("snapshots");
    insta::with_settings!({snapshot_path => snapshot_root}, {
        insta::glob!("viz-runtime/testdata", "*.baml", |relative| {
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join(relative);
            let fixture_name = Path::new(relative)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();

            let snapshot = rt.block_on(run_fixture(&fixture)).expect("fixture run");

            let snapshot_name = format!("viz_runtime__{}", fixture_name.replace('-', "_"));
            insta::assert_yaml_snapshot!(snapshot_name, &snapshot);
        });
    });
}

async fn run_fixture(path: &Path) -> anyhow::Result<FixtureSnapshot> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let relative = path
        .strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let mut files = HashMap::new();
    let contents = fs::read_to_string(path)?;
    files.insert(relative, contents);

    let runtime = baml_runtime::BamlRuntime::from_file_content(
        root.to_str().expect("manifest dir to str"),
        &files,
        HashMap::<String, String>::new(),
        FeatureFlags::default(),
    )?;

    let events = Arc::new(Mutex::new(Vec::<EventRecord>::new()));
    let stacks = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let emitted_events = Arc::new(Mutex::new(Vec::<Vec<StateUpdate>>::new()));
    let reducer = Arc::new(Mutex::new(VizStateReducer::default()));

    let handler = build_watch_handler(
        events.clone(),
        stacks.clone(),
        emitted_events.clone(),
        reducer.clone(),
    );

    let ctx = RuntimeContextManager::new(None);
    let on_event: Option<fn(FunctionResult)> = None;
    let on_tick: Option<fn()> = None;

    let (result, _) = runtime
        .run_test_with_expr_events(
            "Main",
            "default",
            &ctx,
            on_event,
            None,
            None,
            HashMap::<String, String>::new(),
            None,
            TripWire::new(None),
            on_tick,
            Some(handler),
        )
        .await;

    result?;

    let events = events.lock().unwrap();
    let stacks = stacks.lock().unwrap();
    let emitted_events = emitted_events.lock().unwrap();

    let snapshots: Vec<_> = events
        .iter()
        .enumerate()
        .map(|(idx, event)| StreamSnapshot {
            watch_event: event.clone(),
            stack_after: stacks.get(idx).cloned().unwrap_or_default(),
            emitted_events: emitted_events.get(idx).cloned().unwrap_or_default(),
        })
        .collect();

    Ok(FixtureSnapshot {
        fixture: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string(),
        snapshots,
    })
}

fn build_watch_handler(
    events: Arc<Mutex<Vec<EventRecord>>>,
    stacks: Arc<Mutex<Vec<Vec<String>>>>,
    emitted_events: Arc<Mutex<Vec<Vec<StateUpdate>>>>,
    reducer: Arc<Mutex<VizStateReducer>>,
) -> SharedWatchHandler {
    shared_handler(move |notification: WatchNotification| {
        let event = to_event_record(&notification);

        let mut reducer_guard = reducer.lock().unwrap();
        let (updates, state_after) = if let Some(viz_event) = event.viz_event.as_ref() {
            let updates = reducer_guard.apply(&notification.function_name, viz_event);
            let state_after = reducer_guard.dump();
            (updates, state_after)
        } else {
            let state_after = reducer_guard.dump();
            (Vec::new(), state_after)
        };
        let stack_after: Vec<String> = state_after
            .iter()
            .map(|f| f.log_filter_key.clone())
            .collect();
        drop(reducer_guard);

        events.lock().unwrap().push(event);
        emitted_events.lock().unwrap().push(updates);
        stacks.lock().unwrap().push(stack_after);
    })
}

fn to_event_record(notification: &WatchNotification) -> EventRecord {
    let (kind, viz_event) = match &notification.value {
        WatchBamlValue::VizExecState(event) => ("viz_exec_state".to_string(), Some(event.clone())),
        _ => ("other".to_string(), None),
    };

    let header = None;

    EventRecord {
        kind,
        header,
        viz_event,
    }
}
