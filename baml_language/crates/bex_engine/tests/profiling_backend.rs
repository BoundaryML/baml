mod common;

use std::{sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, logger::TraceLogger};
use bex_events::{
    ids::BoundaryId,
    prof::backend::{
        DataState, DiskBudget, ErrorUnwindKind, EvidenceFact, ExecutionProfile, ExecutionStatus,
        IndexState, MetaRecord, Plane, ProfilerConfig, ProfilerSession, StreamId,
        TerminalErrorTarget, ThreadStartKind, ValueState, decode_data_segment, decode_meta_segment,
        list_executions, list_streams, segment_path, stream_directory,
    },
};
use common::compile_for_engine;
use sys_native::SysOpsExt;

static PROFILER_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn profiler_config(store_root: &std::path::Path, enabled: bool) -> ProfilerConfig {
    ProfilerConfig {
        enabled,
        store_root: store_root.to_owned(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
        // Manual publication; tests flush explicitly (streams spec §9).
        publish_interval: Duration::MAX,
        stream: None,
    }
}

/// Opens a session, waiting out the previous test's store drop: the stream
/// is keyed by the process euid, and the consumer thread may briefly hold
/// the prior session alive.
fn new_session(store_root: &std::path::Path) -> Arc<ProfilerSession> {
    for _ in 0..100 {
        let (session, diagnostic) = ProfilerSession::from_config(profiler_config(store_root, true));
        match diagnostic {
            None => return session,
            Some(diagnostic) if diagnostic.message.contains("StreamInUse") => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Some(diagnostic) => panic!("unexpected setup diagnostic: {diagnostic:?}"),
        }
    }
    panic!("stream stayed in use across tests");
}

fn load_profile(store_root: &std::path::Path, runtime_id: BoundaryId) -> ExecutionProfile {
    let executions = list_executions(store_root).unwrap();
    let summary = executions
        .iter()
        .find(|execution| execution.runtime_id == Some(runtime_id))
        .unwrap_or_else(|| panic!("execution with runtime token not listed: {executions:?}"));
    bex_events::prof::backend::StreamReader::open(store_root, summary.stream)
        .unwrap()
        .execution(summary.id)
        .unwrap()
        .load()
        .unwrap()
}

#[tokio::test]
async fn off_session_preserves_identity_logging_and_existing_store_bytes() {
    let _guard = PROFILER_TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let store_root = temp.path().join("preexisting/profiles-v1");
    std::fs::create_dir_all(&store_root).unwrap();
    let sentinel = store_root.join("sentinel");
    std::fs::write(&sentinel, b"unchanged").unwrap();
    let before = std::fs::metadata(&sentinel).unwrap();

    // §11: an off session never starts the consumer thread. Other tests in
    // this binary may already have started it, so pin "unchanged by this
    // run" rather than "never started in this process".
    let consumer_started_before = bex_events::prof::consumer_thread_started();
    let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        disk: DiskBudget {
            max_project_bytes: 1,
            minimum_free_bytes: u64::MAX,
        },
        ..profiler_config(&store_root, false)
    });
    assert!(diagnostic.is_none());
    assert!(session.boundary_registry().is_none());
    assert!(session.memory().is_none());

    let source = r#"
        function helper(x: int) -> int { x + 1 }
        function fail() -> int throws string { throw "expected" }
        function main() -> string {
            log.info("off-mode log")
            let capture_id = boundary.id().capture(inputs = true, output = true, error = true)
            let captured = helper(1, $id = capture_id)
            let failed = fail() catch (e) { _ => 0 };
            let future = spawn { helper(captured + failed) };
            let _ = await future;
            boundary.id.current()
        }
    "#;
    let logger = TraceLogger::bounded(32);
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            session,
        )
        .unwrap(),
    );
    let boundary_id = BoundaryId::from_bytes([0x94; 16]);
    let result = engine
        .call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary_id)
                .with_logger(logger.clone())
                .build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        BexExternalValue::String(boundary_id.to_wire_string().into())
    );
    assert_eq!(logger.drain_encoded_logs().logs.len(), 1);

    let after = std::fs::metadata(&sentinel).unwrap();
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"unchanged");
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    let entries = std::fs::read_dir(&store_root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, [std::ffi::OsString::from("sentinel")]);
    assert_eq!(
        bex_events::prof::consumer_thread_started(),
        consumer_started_before,
        "an off session must not start the profiling consumer thread"
    );
}

#[tokio::test]
async fn admitted_root_and_spawn_publish_one_stream_of_three_segments() {
    let _guard = PROFILER_TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let store_root = temp.path().join(".baml/profiles-v1");
    let session = new_session(&store_root);
    let registry = Arc::clone(session.boundary_registry().unwrap());
    let source = r#"
        function helper(x: int) -> int { x + 1 }
        function main() -> int {
            let pending = spawn { helper(40) };
            helper(await pending)
        }
    "#;
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            session,
        )
        .unwrap(),
    );
    let boundary_id = BoundaryId::from_bytes([0xA5; 16]);
    let result = engine
        .call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary_id)
                .build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result, BexExternalValue::Int(42));
    assert!(bex_events::prof::flush_and_join(Duration::from_secs(5)));

    assert!(registry.ready_handles().is_empty());

    // One stream; exactly 3 segment files (streams spec §9): meta 1 =
    // StreamStarted + EngineStarted + RootStarted, data 1 = one group, meta
    // 2 = RootEnded with a final data range.
    let streams = list_streams(&store_root).unwrap();
    assert_eq!(streams.len(), 1);
    let stream = streams[0];
    let directory = stream_directory(&store_root, stream);
    let mut segment_files: Vec<String> = ["meta", "data"]
        .iter()
        .flat_map(|plane| std::fs::read_dir(directory.join(plane)).unwrap())
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".bamlmeta") || name.ends_with(".bamldata"))
        .collect();
    segment_files.sort();
    assert_eq!(
        segment_files,
        [
            "00000000000000000001.bamldata",
            "00000000000000000001.bamlmeta",
            "00000000000000000002.bamlmeta",
        ]
    );

    let meta_one = decode_meta_segment(
        &std::fs::read(segment_path(&store_root, stream, Plane::Meta, 1)).unwrap(),
        stream.0,
    )
    .unwrap();
    assert_eq!(meta_one.records.len(), 3);
    assert!(matches!(
        meta_one.records[0],
        MetaRecord::StreamStarted { .. }
    ));
    assert!(matches!(
        meta_one.records[1],
        MetaRecord::EngineStarted { .. }
    ));
    assert!(matches!(
        meta_one.records[2],
        MetaRecord::RootStarted { runtime_id, .. } if runtime_id == boundary_id
    ));
    let MetaRecord::EngineStarted {
        function_table_cid, ..
    } = &meta_one.records[1]
    else {
        panic!("engine record expected");
    };
    assert!(
        function_table_cid.is_some(),
        "the engine's function table must reach the CAS"
    );

    let data_bytes = std::fs::read(segment_path(&store_root, stream, Plane::Data, 1)).unwrap();
    let data_one = decode_data_segment(&data_bytes, stream.0).unwrap();
    assert_eq!(data_one.groups.len(), 1, "one execution, one group");
    let facts = data_one.groups[0].decode_evidence().unwrap();
    let thread_starts = facts
        .iter()
        .filter(|fact| matches!(fact, EvidenceFact::ThreadStart(_)))
        .count();
    let thread_ends = facts
        .iter()
        .filter(|fact| matches!(fact, EvidenceFact::ThreadEnd(_)))
        .count();
    assert_eq!(thread_starts, 2, "root + spawned thread starts are durable");
    assert_eq!(thread_ends, 2, "root + spawned thread ends are durable");
    assert_eq!(
        facts
            .iter()
            .filter(|fact| matches!(fact, EvidenceFact::SpanStart(_)))
            .count(),
        1,
        "only the root span is selected"
    );
    assert_eq!(
        facts
            .iter()
            .filter(|fact| matches!(fact, EvidenceFact::SpanEnd(_)))
            .count(),
        1
    );

    let meta_two = decode_meta_segment(
        &std::fs::read(segment_path(&store_root, stream, Plane::Meta, 2)).unwrap(),
        stream.0,
    )
    .unwrap();
    assert_eq!(meta_two.records.len(), 1);
    assert!(matches!(
        meta_two.records[0],
        MetaRecord::RootEnded {
            data_first_seq: 1,
            data_last_seq: 1,
            data_segment_count: 1,
            flags: 0,
            ..
        }
    ));

    // Reader model: listed, complete, succeeded, values reachable.
    let profile = load_profile(&store_root, boundary_id);
    assert_eq!(profile.summary.status, ExecutionStatus::Succeeded);
    assert_eq!(profile.summary.index_state, IndexState::Complete);
    assert_eq!(profile.data_state, DataState::Complete);
    assert!(profile.summary.started_unix_ns.is_some());
    assert!(!profile.contexts.is_empty());
    assert!(!profile.spans.is_empty());
    let spawned = profile
        .threads
        .values()
        .find(|thread| {
            thread
                .start
                .as_ref()
                .is_some_and(|start| matches!(start.kind, ThreadStartKind::Spawn))
        })
        .expect("spawned thread lifecycle is durable");
    assert!(spawned.start.as_ref().unwrap().parent.is_some());
    assert!(spawned.end.is_some());

    let reader = bex_events::prof::backend::StreamReader::open(&store_root, stream).unwrap();
    let execution = reader.execution(profile.summary.id).unwrap();
    let cid = profile
        .spans
        .values()
        .flat_map(|span| [span.input, span.output])
        .find_map(|occurrence| match occurrence?.state {
            ValueState::Available { cid, .. } => Some(cid),
            ValueState::Lost(_) => None,
        })
        .expect("an input or output occurrence must reference CAS");
    assert_eq!(execution.read_value(cid).unwrap().cid, cid);
    let table = execution
        .function_table()
        .unwrap()
        .expect("durable function table");
    assert!(
        table
            .functions
            .iter()
            .any(|function| function.fqn == "main"
                || function.fqn.rsplit('.').next() == Some("main")),
        "function table must contain the entry function; fqns: {:?}",
        table
            .functions
            .iter()
            .map(|function| function.fqn.clone())
            .collect::<Vec<_>>()
    );

    // Damage: a missing data segment in range is typed and folds the rest —
    // never a hard error.
    std::fs::remove_file(segment_path(&store_root, stream, Plane::Data, 1)).unwrap();
    let damaged = reader
        .execution(profile.summary.id)
        .unwrap()
        .load()
        .unwrap();
    let DataState::Incomplete(issues) = damaged.data_state else {
        panic!("missing segment must mark the fold incomplete");
    };
    assert!(issues.iter().any(|issue| matches!(
        issue,
        bex_events::prof::backend::DataIssue::MissingDataSegment(1)
    )));
}

#[tokio::test]
async fn selected_throw_publishes_one_error_capture_and_terminal_link() {
    let _guard = PROFILER_TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let store_root = temp.path().join(".baml/profiles-v1");
    let session = new_session(&store_root);
    let source = r#"
        function fail() -> int throws string { throw "boom" }
        function main() -> int {
            let error_id = boundary.id().capture(inputs = false, output = false, error = true)
            let _ = fail($id = error_id) catch (e) { _ => 0 }
            7
        }
    "#;
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            session,
        )
        .unwrap(),
    );
    let boundary_id = BoundaryId::from_bytes([0xB6; 16]);
    let result = engine
        .call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary_id)
                .build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result, BexExternalValue::Int(7));
    assert!(bex_events::prof::flush_and_join(Duration::from_secs(5)));

    let profile = load_profile(&store_root, boundary_id);
    assert_eq!(
        profile.errors.len(),
        1,
        "one unwind must produce one error body"
    );
    assert_eq!(
        profile
            .spans
            .values()
            .filter(|span| span.terminal_error.is_some())
            .count(),
        1,
        "the selected terminated call must link to that unwind"
    );
}

#[tokio::test]
async fn one_unwind_fans_out_and_rethrow_gets_a_new_error_id() {
    let _guard = PROFILER_TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let store_root = temp.path().join(".baml/profiles-v1");
    let session = new_session(&store_root);
    let source = r#"
        function leaf() -> int throws string { throw "boom" }
        function selected_middle() -> int throws string { leaf() }
        function fanout_main() -> int {
            let error_id = boundary.id().capture(inputs = false, output = false, error = true)
            selected_middle($id = error_id)
        }
        function rethrow_main() -> int {
            let error_id = boundary.id().capture(inputs = false, output = false, error = true)
            selected_middle($id = error_id) catch (e) { _ => throw e }
        }
    "#;
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            session,
        )
        .unwrap(),
    );
    let fanout_boundary_id = BoundaryId::from_bytes([0xC7; 16]);
    let fanout_result = engine
        .call_function(
            "fanout_main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(fanout_boundary_id)
                .build(),
            true,
        )
        .await;
    assert!(
        fanout_result.is_err(),
        "the fan-out throw must escape the root"
    );
    let rethrow_boundary_id = BoundaryId::from_bytes([0xC8; 16]);
    let rethrow_result = engine
        .call_function(
            "rethrow_main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(rethrow_boundary_id)
                .build(),
            true,
        )
        .await;
    assert!(rethrow_result.is_err(), "the rethrow must escape the root");
    assert!(bex_events::prof::flush_and_join(Duration::from_secs(5)));

    let fanout = load_profile(&store_root, fanout_boundary_id);
    let fanout_captures = fanout.errors.values().copied().collect::<Vec<_>>();
    assert_eq!(fanout_captures.len(), 1);
    assert_eq!(fanout_captures[0].kind, ErrorUnwindKind::Fresh);
    assert_eq!(
        fanout
            .spans
            .values()
            .filter(|span| span
                .terminal_error
                .is_some_and(|terminal| terminal.target
                    == TerminalErrorTarget::Capture(fanout_captures[0].id)))
            .count(),
        2,
        "one unwind must link selected middle and root without copying twice"
    );

    let rethrow = load_profile(&store_root, rethrow_boundary_id);
    let mut captures = rethrow.errors.values().copied().collect::<Vec<_>>();
    captures.sort_by_key(|capture| capture.id.unwind_ordinal);
    assert_eq!(
        captures.len(),
        2,
        "fresh throw and rethrow are separate unwinds"
    );
    assert_eq!(captures[0].kind, ErrorUnwindKind::Fresh);
    assert_eq!(captures[1].kind, ErrorUnwindKind::Rethrow);
    assert_eq!(
        captures[0].id.unwind_ordinal + 1,
        captures[1].id.unwind_ordinal
    );

    let terminal_targets = rethrow
        .spans
        .values()
        .filter_map(|span| span.terminal_error.map(|terminal| terminal.target))
        .collect::<Vec<_>>();
    assert_eq!(terminal_targets.len(), 2);
    assert_eq!(
        terminal_targets
            .iter()
            .filter(|target| **target == TerminalErrorTarget::Capture(captures[0].id))
            .count(),
        1,
        "the first unwind terminates selected middle while root catches"
    );
    assert_eq!(
        terminal_targets
            .iter()
            .filter(|target| **target == TerminalErrorTarget::Capture(captures[1].id))
            .count(),
        1,
        "the root rethrow has its own terminal link"
    );
    // Both executions share the process stream.
    let _ = StreamId(fanout.summary.stream.0);
    assert_eq!(fanout.summary.stream, rethrow.summary.stream);
}

/// Standard `UUIDv7` program identities keep matching call paths from different
/// engine constructions in separate profiling contexts.
#[tokio::test]
async fn program_identity_separates_context_keys() {
    async fn run_once(
        store_root: &std::path::Path,
        source: &str,
        token: u8,
    ) -> (
        bex_events::ids::ProgramId,
        std::collections::BTreeSet<[u8; 32]>,
    ) {
        let session = new_session(store_root);
        let engine = Arc::new(
            BexEngine::new_with_profiler_session(
                compile_for_engine(source),
                Arc::new(sys_native::SysOps::native()),
                Vec::new(),
                session,
            )
            .unwrap(),
        );
        let boundary_id = BoundaryId::from_bytes([token; 16]);
        engine
            .call_function(
                "main",
                Vec::new(),
                FunctionCallContextBuilder::new(sys_types::CallId::next())
                    .with_boundary_id(boundary_id)
                    .build(),
                true,
            )
            .await
            .unwrap();
        assert!(bex_events::prof::flush_and_join(Duration::from_secs(5)));
        let profile = load_profile(store_root, boundary_id);
        let program_id = profile.summary.program_id.expect("engine record present");
        let keys = profile.contexts.keys().map(|key| key.0).collect();
        drop(engine);
        (program_id, keys)
    }

    let _guard = PROFILER_TEST_LOCK.lock().await;
    let assert_uuid_v7 = |id: bex_events::ids::ProgramId| {
        assert_eq!(id.0[6] >> 4, 7);
        assert_eq!(id.0[8] >> 6, 2);
    };
    let source = r#"
        function main() -> int { 41 + 1 }
    "#;
    let source_comment_flip = r#"
        function main() -> int { 41 + 1 } //
    "#;
    let temp_a = tempfile::TempDir::new().unwrap();
    let (id_a, keys_a) = run_once(&temp_a.path().join(".baml/profiles-v1"), source, 0xD1).await;
    let temp_b = tempfile::TempDir::new().unwrap();
    let (id_b, keys_b) = run_once(&temp_b.path().join(".baml/profiles-v1"), source, 0xD2).await;
    assert_uuid_v7(id_a);
    assert_uuid_v7(id_b);
    assert_ne!(id_a, id_b);
    assert_ne!(keys_a, keys_b);

    let temp_c = tempfile::TempDir::new().unwrap();
    let (id_c, keys_c) = run_once(
        &temp_c.path().join(".baml/profiles-v1"),
        source_comment_flip,
        0xD3,
    )
    .await;
    assert_uuid_v7(id_c);
    assert_ne!(id_a, id_c);
    assert_ne!(keys_a, keys_c);
}
