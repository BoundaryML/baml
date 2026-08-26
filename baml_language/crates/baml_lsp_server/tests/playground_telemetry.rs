//! The playground's telemetry reads against a real `profiles-v1` store.
//!
//! The store is written through the producer session (the same path a live
//! engine takes), then read back through the module the WebSocket handlers
//! call. This is what proves the server-authored SQL matches catalog v1:
//! a renamed or dropped column fails here rather than at a user's screen.

use std::path::Path;

use baml_lsp_server::playground_telemetry::{list_executions, read_execution};
use bex_prof_store::{
    ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid, ProgramId, ThreadRef},
    prof::{
        backend::{
            self, DiskBudget, ExecutionEndStatus, FunctionCaptureClass, ProfilerConfig,
            ProfilerSession, RootAdmission, RootProfileIntent,
        },
        clock,
        record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus},
    },
};

/// `maintain_sessions`/`flush_sessions` drain every live session in the
/// process, so two tests writing stores concurrently would drain each
/// other's pending work.
static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn config(root: &Path, euid: ProcessEuid) -> ProfilerConfig {
    ProfilerConfig {
        enabled: true,
        store_root: root.to_owned(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
        publish_interval: std::time::Duration::MAX,
        stream: Some(euid),
    }
}

/// One execution: a root thread running `root -> child`, where the root is
/// retained and the child is called twice but never selected. That shape is
/// the point — it is what makes `calls_started` (2) differ from the retained
/// span count (0) on the child's path, which is the gap the UI must show.
fn write_store(root: &Path, euid: ProcessEuid, engine: u64) -> ThreadRef {
    let _guard = STORE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (session, diagnostic) = ProfilerSession::from_config(config(root, euid));
    assert!(diagnostic.is_none(), "store setup: {diagnostic:?}");
    let engine_id = EngineId(engine);
    backend::register_engine_session(engine_id, &session);
    let thread_ref = ThreadRef {
        process_euid: euid,
        engine_id,
        thread_id: BexThreadId(3),
    };
    let RootAdmission::Active(admission) = session.register_root(
        RootProfileIntent::UserRoot {
            runtime_id: BoundaryId::from_bytes([0x77; 16]),
        },
        thread_ref,
        ProgramId([9; 16]),
    ) else {
        panic!("root must be admitted");
    };
    let emit = |record: RawRecord<'_>| {
        let mut bytes = [0; MAX_RECORD_LEN];
        let len = record.encode(&mut bytes);
        backend::consume_engine_bytes(euid, engine_id, &bytes[..len]);
    };
    // Ticks must come from the process clock: the consumer converts them
    // against its calibration, so invented values transcode to zero
    // nanoseconds and every timing assertion below would pass vacuously.
    clock::init();
    let tick = |millis: u64| {
        if millis > 0 {
            std::thread::sleep(std::time::Duration::from_millis(millis));
        }
        clock::now_ticks()
    };

    let selected =
        backend::resolve_capture_plan(true, FunctionCaptureClass::Ordinary, None).to_call_flags();
    let unselected =
        backend::resolve_capture_plan(false, FunctionCaptureClass::Ordinary, None).to_call_flags();

    emit(RawRecord::StartThread {
        flags: 0,
        thread_id: thread_ref.thread_id,
        parent_thread_id: BexThreadId(0),
        parent_call_id: BexCallId(0),
        ts_ticks: tick(0),
        name: b"",
    });
    emit(RawRecord::CallFunction {
        flags: selected,
        thread_id: thread_ref.thread_id,
        call_id: BexCallId(6),
        parent_call_id: BexCallId(0),
        function_id: bex_prof_store::ids::FunctionId(7),
        call_site: None,
        ts_ticks: tick(0),
    });
    // Two child calls that burn measurable wall time inside the root, so the
    // root keeps real self time once its children are deducted.
    for (call_id, busy_ms) in [(7u64, 12), (8, 8)] {
        let start = tick(0);
        let end = tick(busy_ms);
        emit(RawRecord::CallFunction {
            flags: unselected,
            thread_id: thread_ref.thread_id,
            call_id: BexCallId(call_id),
            parent_call_id: BexCallId(6),
            function_id: bex_prof_store::ids::FunctionId(8),
            call_site: None,
            ts_ticks: start,
        });
        emit(RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: thread_ref.thread_id,
            call_id: BexCallId(call_id),
            ts_ticks: end,
        });
    }
    emit(RawRecord::EndFunction {
        status: FunctionEndStatus::Ok,
        thread_id: thread_ref.thread_id,
        call_id: BexCallId(6),
        ts_ticks: tick(6),
    });
    emit(RawRecord::EndThread {
        status: ThreadEndStatus::Completed,
        thread_id: thread_ref.thread_id,
        ts_ticks: tick(0),
    });
    admission.completion.complete(ExecutionEndStatus::Succeeded);
    assert!(backend::maintain_sessions());
    let _ = backend::maintain_sessions();
    backend::flush_sessions();
    backend::unregister_engine_session(engine_id);
    thread_ref
}

#[tokio::test]
async fn lists_executions_from_the_profile_store() {
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();
    write_store(
        &project_root.join(".baml/profiles-v1"),
        ProcessEuid([0xD1; 16]),
        41,
    );

    let executions = list_executions(project_root).await.expect("read store");

    assert_eq!(executions.len(), 1, "one execution was written");
    let execution = &executions[0];
    assert_eq!(execution.status.as_deref(), Some("succeeded"));
    assert_eq!(execution.index_state.as_deref(), Some("complete"));
    // Population counts cover every call; retention covers only the root.
    assert_eq!(execution.total_calls, Some(3));
    assert_eq!(execution.calls_retained, Some(1));
    assert_eq!(execution.total_errors, Some(0));
    assert_eq!(execution.threads_total, Some(1));
    assert!(
        execution.duration_ns.is_some_and(|ns| ns > 0),
        "root span carries inclusive time: {:?}",
        execution.duration_ns
    );
    // Nothing published a function table or engine labels here, so the
    // identity columns are absent. The reader surfaces that as missing
    // fields rather than failing the read.
    assert!(execution.entry_fqn.is_none());
    assert!(execution.revision_id.is_none());
}

#[tokio::test]
async fn reads_one_execution_across_all_four_grains() {
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();
    let thread_ref = write_store(
        &project_root.join(".baml/profiles-v1"),
        ProcessEuid([0xD2; 16]),
        42,
    );
    let execution_id = bex_prof_store::ids::ExecutionId(thread_ref).encode();

    let telemetry = read_execution(project_root, &execution_id)
        .await
        .expect("read execution");

    assert_eq!(
        telemetry
            .execution
            .as_ref()
            .map(|row| row.execution_id.as_str()),
        Some(execution_id.as_str())
    );

    // Threads: the root lane is present and has no parent.
    assert_eq!(telemetry.threads.len(), 1);
    assert_eq!(telemetry.threads[0].kind.as_deref(), Some("root"));
    assert!(telemetry.threads[0].parent_thread_id.is_none());

    // Call paths: root plus the child path the two unselected calls took.
    assert_eq!(telemetry.call_paths.len(), 2, "root and child paths");
    let root_path = telemetry
        .call_paths
        .iter()
        .find(|path| path.parent_call_path_id.is_none())
        .expect("a root path");
    assert_eq!(root_path.edge_kind.as_deref(), Some("root"));
    assert_eq!(root_path.calls_started, Some(1));

    let child_path = telemetry
        .call_paths
        .iter()
        .find(|path| path.parent_call_path_id.is_some())
        .expect("a child path");
    // The whole point of the aggregate tier: both calls are counted even
    // though neither was retained as a span.
    assert_eq!(child_path.calls_started, Some(2));
    assert_eq!(child_path.completed_ok, Some(2));
    assert!(child_path.timing_complete.unwrap_or(false));
    assert!(child_path.overflow_reason.is_none());

    // self = inclusive - direct_child - await, so the parts must reconcile.
    let inclusive = root_path.inclusive_ns.unwrap_or(0);
    let direct_child = root_path.direct_child_ns.unwrap_or(0);
    let awaited = root_path.await_ns.unwrap_or(0);
    assert_eq!(
        root_path.self_ns,
        Some(inclusive - direct_child - awaited),
        "self time is the residue of inclusive time"
    );
    assert!(
        direct_child > 0 && root_path.self_ns.unwrap_or(0) > 0,
        "the identity must be checked on real quantities, not on zeros: \
         inclusive={inclusive} direct_child={direct_child} await={awaited}"
    );

    // Retained spans: only the root was selected, and it joins to its path.
    assert_eq!(telemetry.calls.len(), 1, "only the root was retained");
    let call = &telemetry.calls[0];
    assert_eq!(call.status.as_deref(), Some("ok"));
    assert_eq!(
        call.call_path_id.as_deref(),
        Some(root_path.call_path_id.as_str()),
        "the span joins its aggregate exactly, not by name"
    );
    assert!(
        call.selection_reasons.iter().any(|reason| reason == "root"),
        "retained because it is the root: {:?}",
        call.selection_reasons
    );

    assert!(telemetry.errors.is_empty(), "nothing errored");
}

#[tokio::test]
async fn orders_executions_by_wall_clock_not_process_relative_time() {
    // `started_ns` counts from each process's own start, so a run beginning
    // late inside a long-lived process carries a larger value than a newer
    // run in a fresh one. Ordering by it ranks them backwards.
    let temp = tempfile::TempDir::new().unwrap();
    let project_root = temp.path();
    let store = project_root.join(".baml/profiles-v1");
    write_store(&store, ProcessEuid([0xD3; 16]), 51);
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_store(&store, ProcessEuid([0xD4; 16]), 52);

    let executions = list_executions(project_root).await.expect("read store");

    assert_eq!(executions.len(), 2, "two processes wrote executions");
    let started: Vec<i64> = executions
        .iter()
        .map(|row| row.started_at_ms.expect("a clock anchor"))
        .collect();
    assert!(
        started[0] >= started[1],
        "newest first by wall clock: {started:?}"
    );
}

#[tokio::test]
async fn reports_a_missing_store_as_an_empty_state() {
    let temp = tempfile::TempDir::new().unwrap();

    let err = list_executions(temp.path())
        .await
        .expect_err("no store was written");

    assert!(
        matches!(
            err,
            baml_lsp_server::playground_telemetry::TelemetryError::NoStore(_)
        ),
        "a project that has never run is an empty state, not a query failure: {err}"
    );
}
