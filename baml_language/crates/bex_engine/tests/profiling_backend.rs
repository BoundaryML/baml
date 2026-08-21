mod common;

use std::{fmt::Write as _, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, logger::TraceLogger};
use bex_events::{
    ids::BoundaryId,
    prof::backend::{
        DiskBudget, DurableRunReader, ErrorUnwindKind, EvidenceFact, ProfilerConfig,
        ProfilerSession, RunReadError, SegmentKind, TerminalErrorTarget, ValueState,
        decode_evidence_segment,
    },
};
use common::compile_for_engine;
use sys_native::SysOpsExt;

static PROFILER_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        enabled: false,
        store_root: store_root.clone(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 1,
            minimum_free_bytes: u64::MAX,
        },
    });
    assert!(diagnostic.is_none());
    assert!(session.boundary_registry().is_none());
    assert!(session.memory().is_none());

    let source = r#"
        function helper(x: int) -> int { x + 1 }
        function fail() -> int throws string { throw "expected" }
        function main() -> string throws unknown {
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

fn read_evidence(store_root: &std::path::Path, boundary_id: BoundaryId) -> Vec<EvidenceFact> {
    let mut run_name = String::with_capacity(32);
    for byte in boundary_id.as_bytes() {
        write!(&mut run_name, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let mut paths = std::fs::read_dir(store_root.join("runs").join(run_name).join("evidence"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|path| {
            decode_evidence_segment(&std::fs::read(path).unwrap())
                .unwrap()
                .facts
        })
        .collect()
}

#[tokio::test]
async fn admitted_root_and_spawn_publish_one_sealed_segmented_run() {
    let _guard = PROFILER_TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let store_root = temp.path().join(".baml/profiles-v1");
    let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: true,
        store_root: store_root.clone(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
    });
    assert!(diagnostic.is_none(), "{diagnostic:?}");
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
    let run = store_root.join("runs").join("a5".repeat(16));
    assert!(run.join("run.meta").is_file());
    assert!(run.join("cct/00000000000000000001.bamlcct").is_file());
    assert!(
        run.join("evidence/00000000000000000001.bamlspans")
            .is_file()
    );
    assert!(run.join("run.end").is_file());
    let cas_objects = std::fs::read_dir(store_root.join("cas/sha256"))
        .unwrap()
        .flat_map(|prefix| std::fs::read_dir(prefix.unwrap().path()).unwrap())
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "bamlvalue")
        })
        .count();
    assert!(cas_objects >= 2, "root input and output must reach the CAS");

    let reader = DurableRunReader::open(&store_root, boundary_id).unwrap();
    let loaded = reader.load().unwrap();
    assert!(loaded.end.is_some(), "sealed reader must expose run.end");
    assert!(!loaded.contexts.is_empty());
    assert!(!loaded.spans.is_empty());
    let cid = loaded
        .spans
        .values()
        .flat_map(|span| [span.input, span.output])
        .find_map(|occurrence| match occurrence?.state {
            ValueState::Available { cid, .. } => Some(cid),
            ValueState::Lost(_) => None,
        })
        .expect("an input or output occurrence must reference CAS");
    assert_eq!(reader.read_value(cid).unwrap().cid, cid);

    std::fs::remove_file(run.join("evidence/00000000000000000001.bamlspans")).unwrap();
    assert!(matches!(
        DurableRunReader::open(&store_root, boundary_id)
            .unwrap()
            .load(),
        Err(RunReadError::MissingSegment {
            kind: SegmentKind::Evidence,
            sequence: 1
        })
    ));
}

#[tokio::test]
async fn selected_throw_publishes_one_error_capture_and_terminal_link() {
    let _guard = PROFILER_TEST_LOCK.lock().await;
    let temp = tempfile::TempDir::new().unwrap();
    let store_root = temp.path().join(".baml/profiles-v1");
    let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: true,
        store_root: store_root.clone(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
    });
    assert!(diagnostic.is_none(), "{diagnostic:?}");
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

    let facts = read_evidence(&store_root, boundary_id);
    assert_eq!(
        facts
            .iter()
            .filter(|fact| matches!(fact, EvidenceFact::ErrorCapture(_)))
            .count(),
        1,
        "one unwind must produce one error body"
    );
    assert_eq!(
        facts
            .iter()
            .filter(|fact| matches!(fact, EvidenceFact::TerminalErrorRef(_)))
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
    let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: true,
        store_root: store_root.clone(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
    });
    assert!(diagnostic.is_none(), "{diagnostic:?}");
    let source = r#"
        function leaf() -> int throws string { throw "boom" }
        function selected_middle() -> int throws string { leaf() }
        function fanout_main() -> int throws unknown {
            let error_id = boundary.id().capture(inputs = false, output = false, error = true)
            selected_middle($id = error_id)
        }
        function rethrow_main() -> int throws unknown {
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

    let fanout_facts = read_evidence(&store_root, fanout_boundary_id);
    let fanout_captures = fanout_facts
        .iter()
        .filter_map(|fact| match fact {
            EvidenceFact::ErrorCapture(capture) => Some(*capture),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fanout_captures.len(), 1);
    assert_eq!(fanout_captures[0].kind, ErrorUnwindKind::Fresh);
    assert_eq!(
        fanout_facts
            .iter()
            .filter(|fact| matches!(
                fact,
                EvidenceFact::TerminalErrorRef(terminal)
                    if terminal.target == TerminalErrorTarget::Capture(fanout_captures[0].id)
            ))
            .count(),
        2,
        "one unwind must link selected middle and root without copying twice"
    );

    let facts = read_evidence(&store_root, rethrow_boundary_id);
    let mut captures = facts
        .iter()
        .filter_map(|fact| match fact {
            EvidenceFact::ErrorCapture(capture) => Some(*capture),
            _ => None,
        })
        .collect::<Vec<_>>();
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

    let terminal_targets = facts
        .iter()
        .filter_map(|fact| match fact {
            EvidenceFact::TerminalErrorRef(terminal) => Some(terminal.target),
            _ => None,
        })
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
}
