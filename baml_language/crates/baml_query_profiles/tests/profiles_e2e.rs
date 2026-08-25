//! Phase-2 conformance (TASK/baml-query-scope.md §8): a real
//! `profiles-v1` store written through the producer session, read back
//! through SQL — catalog rows, snapshot determinism, discovery, and the
//! CAS-backed value resolver.

use std::path::Path;

use baml_query::value::{
    model::Value,
    resolver::{DecodeCaps, Resolved, ValueResolver as _},
};
use bex_prof_store::{
    ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid, ProgramId, ThreadRef},
    prof::{
        backend::{
            self, CodecVersion, DiskBudget, ExecutionEndStatus, FunctionCaptureClass,
            ProfilerConfig, ProfilerSession, PublishCasResult, RootAdmission, RootProfileIntent,
        },
        record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus},
    },
};

fn config(root: &Path, euid: ProcessEuid) -> ProfilerConfig {
    ProfilerConfig {
        enabled: true,
        store_root: root.to_owned(),
        process_memory_bytes: 32 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
        // Manual publication: the test drives the cycle explicitly.
        publish_interval: std::time::Duration::MAX,
        stream: Some(euid),
    }
}

/// Serializes the process-global profiler registry across tests.
///
/// `maintain_sessions` and `flush_sessions` drain EVERY live session, not
/// just the caller's engine, so two tests writing stores on parallel
/// harness threads would drain each other's pending work and trip the
/// progress assertion below. Scoping the drain per engine would be a
/// production API change for a test-only problem -- in production these
/// run on the single ring-consumer thread, where draining all sessions is
/// the intent.
static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Writes one completed execution (root thread, one selected root call)
/// into a fresh store and returns its root.
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
    emit(RawRecord::StartThread {
        flags: 0,
        thread_id: thread_ref.thread_id,
        parent_thread_id: BexThreadId(0),
        parent_call_id: BexCallId(0),
        ts_ticks: 10,
        name: b"",
    });
    emit(RawRecord::CallFunction {
        flags: backend::resolve_capture_plan(true, FunctionCaptureClass::Ordinary, None)
            .to_call_flags(),
        thread_id: thread_ref.thread_id,
        call_id: BexCallId(6),
        parent_call_id: BexCallId(0),
        function_id: bex_prof_store::ids::FunctionId(7),
        call_site: None,
        ts_ticks: 20,
    });
    emit(RawRecord::EndFunction {
        status: FunctionEndStatus::Ok,
        thread_id: thread_ref.thread_id,
        call_id: BexCallId(6),
        ts_ticks: 30,
    });
    emit(RawRecord::EndThread {
        status: ThreadEndStatus::Completed,
        thread_id: thread_ref.thread_id,
        ts_ticks: 40,
    });
    admission.completion.complete(ExecutionEndStatus::Succeeded);
    assert!(backend::maintain_sessions());
    let _ = backend::maintain_sessions();
    backend::flush_sessions();
    backend::unregister_engine_session(engine_id);
    thread_ref
}

async fn one_column(
    session: &baml_query::QuerySession,
    sql: &str,
) -> (Vec<Option<String>>, baml_query::QueryOutcome) {
    use datafusion::arrow::{array::Array as _, util::display::array_value_to_string};
    let mut run = session
        .execute(sql)
        .await
        .unwrap_or_else(|(err, _)| panic!("{sql}: {err}"));
    let mut out = Vec::new();
    while let Some(batch) = run.next_batch().await {
        let col = batch.column(0);
        for i in 0..col.len() {
            out.push(if col.is_null(i) {
                None
            } else {
                Some(array_value_to_string(col, i).expect("renderable cell"))
            });
        }
    }
    (out, run.finish())
}

#[tokio::test]
async fn store_rows_flow_through_sql_and_snapshots_are_deterministic() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join(".baml/profiles-v1");
    let euid = ProcessEuid([0xC1; 16]);
    let thread_ref = write_store(&root, euid, 1);
    let execution_id = bex_prof_store::ids::ExecutionId(thread_ref).encode();

    let session = baml_query_profiles::profiles_session(&root)
        .await
        .expect("bind");

    // Executions idiom: root threads, meta-plane truth.
    let (rows, outcome) = one_column(
        &session,
        "SELECT status FROM threads WHERE parent_thread_id IS NULL",
    )
    .await;
    assert_eq!(rows, vec![Some("succeeded".to_string())]);
    assert_eq!(
        outcome.result_state,
        baml_query::ResultState::Complete,
        "resident query is complete"
    );

    // The root row carries the execution columns.
    let (rows, _) = one_column(
        &session,
        &format!(
            "SELECT CAST(total_calls AS VARCHAR) FROM threads \
             WHERE execution_id = '{execution_id}' AND parent_thread_id IS NULL"
        ),
    )
    .await;
    assert_eq!(rows, vec![Some("1".to_string())]);

    // The retained root call, joined through the versioned alias.
    let (rows, _) = one_column(
        &session,
        "SELECT status FROM calls_v1 WHERE edge_kind = 'root'",
    )
    .await;
    assert_eq!(rows, vec![Some("ok".to_string())]);

    // Contexts aggregate the population.
    let (rows, _) = one_column(
        &session,
        "SELECT CAST(calls_started AS VARCHAR) FROM call_path_stats WHERE overflow_reason IS NULL",
    )
    .await;
    assert_eq!(rows, vec![Some("1".to_string())]);

    // Health is long-format and non-empty.
    let (rows, _) = one_column(
        &session,
        "SELECT metric FROM health WHERE metric = 'data_state'",
    )
    .await;
    assert_eq!(rows.len(), 1);

    // Discovery works end to end.
    let (tables, _) = one_column(&session, "SHOW TABLES").await;
    assert!(!tables.is_empty());

    // Deterministic bind: same store, same generation.
    let again = baml_query_profiles::profiles_session(&root)
        .await
        .expect("re-bind");
    assert_eq!(
        session.snapshot().generation,
        again.snapshot().generation,
        "unchanged store binds to the same generation"
    );
}

#[tokio::test]
async fn value_handles_resolve_from_the_cas() {
    use prost::Message as _;

    #[derive(Clone, PartialEq, prost::Message)]
    struct TestBody {
        #[prost(int64, tag = "4")]
        int_value: i64,
    }

    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join(".baml/profiles-v1");
    let euid = ProcessEuid([0xC2; 16]);
    let _ = write_store(&root, euid, 2);

    // Publish a codec-1 value body straight into the CAS.
    let body = TestBody { int_value: 7 }.encode_to_vec();
    let store = bex_prof_store::prof::backend::ProfilerStore::open_native(
        root.clone(),
        DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
        bex_prof_store::prof::backend::StreamId(ProcessEuid([0xC3; 16])),
    )
    .expect("store opens");
    let (cid, result) = store.publish_cas_object(CodecVersion(1), &body);
    assert!(matches!(
        result,
        PublishCasResult::Published | PublishCasResult::Reused
    ));
    drop(store);

    let resolver = baml_query_profiles::ProfilesResolver::new(root);
    let handle = {
        let mut handle = vec![0x01, 0x00, 0x01];
        handle.extend_from_slice(&cid.0);
        handle
    };
    let caps = DecodeCaps {
        max_bytes: 1 << 20,
        max_depth: 16,
    };
    let resolved = resolver.resolve_many(&[&handle], caps);
    assert!(
        matches!(&resolved[0], Resolved::Value(value) if **value == Value::Int(7)),
        "CAS handle resolves to the decoded value: {resolved:?}"
    );
    assert_eq!(resolver.canonical_cid(&handle), Some(cid.0));

    // Unavailable handles resolve to their typed reason.
    let lost = vec![0x00, 0xFE];
    let resolved = resolver.resolve_many(&[&lost], caps);
    assert!(matches!(
        resolved[0],
        Resolved::Unavailable(baml_query::outcome::UnavailableReason::NotCaptured)
    ));
}
