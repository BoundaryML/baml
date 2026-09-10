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

mod codec_fixture {
    use prost::Message;

    #[derive(Clone, PartialEq, Message)]
    pub(super) struct Value {
        #[prost(oneof = "Kind", tags = "3, 4, 7, 11, 12")]
        pub kind: Option<Kind>,
    }

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub(super) enum Kind {
        #[prost(string, tag = "3")]
        String(String),
        #[prost(int64, tag = "4")]
        Int(i64),
        #[prost(message, tag = "7")]
        Class(Class),
        #[prost(message, tag = "11")]
        List(List),
        #[prost(message, tag = "12")]
        Map(Map),
    }

    #[derive(Clone, PartialEq, Message)]
    pub(super) struct Entry {
        #[prost(string, tag = "1")]
        key: String,
        #[prost(message, optional, tag = "2")]
        value: Option<Value>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub(super) struct Map {
        #[prost(message, repeated, tag = "3")]
        entries: Vec<Entry>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub(super) struct Class {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(message, repeated, tag = "2")]
        fields: Vec<Entry>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub(super) struct List {
        #[prost(message, repeated, tag = "2")]
        items: Vec<Value>,
    }

    fn value(kind: Kind) -> Value {
        Value { kind: Some(kind) }
    }

    fn entry(key: &str, kind: Kind) -> Entry {
        Entry {
            key: key.into(),
            value: Some(value(kind)),
        }
    }

    pub(super) fn context() -> Vec<u8> {
        value(Kind::Map(Map {
            entries: vec![
                entry("org_id", Kind::String("acme".into())),
                entry("attempt", Kind::Int(3)),
            ],
        }))
        .encode_to_vec()
    }

    pub(super) fn empty_context() -> Vec<u8> {
        value(Kind::Map(Map { entries: vec![] })).encode_to_vec()
    }

    pub(super) fn data() -> Vec<u8> {
        value(Kind::Map(Map {
            entries: vec![entry(
                "result",
                Kind::Class(Class {
                    name: "Result".into(),
                    fields: vec![entry(
                        "scores",
                        Kind::List(List {
                            items: vec![value(Kind::Int(7)), value(Kind::Int(9))],
                        }),
                    )],
                }),
            )],
        }))
        .encode_to_vec()
    }
}

/// Writes one completed execution (root thread, one selected root call)
/// into a fresh store and returns its root.
fn write_store(root: &Path, euid: ProcessEuid, engine: u64) -> ThreadRef {
    write_store_with(root, euid, engine, |_, _, _, _| {})
}

fn write_store_with(
    root: &Path,
    euid: ProcessEuid,
    engine: u64,
    evidence: impl FnOnce(&ProfilerSession, backend::ExecutionHandle, ThreadRef, &dyn Fn(RawRecord<'_>)),
) -> ThreadRef {
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
    evidence(
        &session,
        admission.completion.lease().handle(),
        thread_ref,
        &emit,
    );
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

#[tokio::test]
async fn logs_and_call_scope_use_native_values_without_parent_joins() {
    use backend::{EncodedCallScope, EncodedLog, LogEvent, ValueLossReason, ValueState};
    use bex_prof_store::ids::CallRef;

    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("profiles");
    let lost = ValueState::Lost(ValueLossReason::CopyFailed);
    let execution = write_store_with(
        &root,
        ProcessEuid([0xD1; 16]),
        31,
        |session, handle, thread, emit| {
            let call = CallRef {
                process_euid: thread.process_euid,
                engine_id: thread.engine_id,
                thread_id: thread.thread_id,
                call_id: BexCallId(6),
            };
            let mut payload_id = None;
            assert!(session.publish_call_scope(
                handle,
                EncodedCallScope {
                    call_ref: call,
                    distinct_id: Some("customer-1".into()),
                    context: Ok(codec_fixture::context()),
                },
                session.reserve_log_work().unwrap(),
                |id| {
                    payload_id = Some(id);
                    true
                },
            ));
            emit(RawRecord::CallScope {
                payload_id: payload_id.unwrap(),
            });

            for ordinal in 0..4 {
                let mut payload_id = None;
                assert!(session.publish_log(
                    handle,
                    EncodedLog {
                        event: LogEvent {
                            call_ref: if ordinal == 1 {
                                None
                            } else {
                                Some(CallRef {
                                    call_id: BexCallId(99),
                                    ..call
                                })
                            },
                            timestamp_ms: if ordinal == 3 {
                                u64::MAX
                            } else {
                                1_700_000_000_000 + ordinal
                            },
                            level: Some("info".into()),
                            source: None,
                            source_column: None,
                            message_preview: None,
                            event_name: (ordinal != 1).then(|| "checkout".into()),
                            distinct_id: (ordinal < 2).then(|| "customer-1".into()),
                            context: lost,
                            data: lost,
                        },
                        context: match ordinal {
                            0 | 1 => Some(codec_fixture::context()),
                            2 => Some(codec_fixture::empty_context()),
                            _ => None,
                        },
                        data: (ordinal != 3).then(codec_fixture::data),
                    },
                    session.reserve_log_work().unwrap(),
                    |id| {
                        payload_id = Some(id);
                        true
                    },
                ));
                emit(RawRecord::Log {
                    payload_id: payload_id.unwrap(),
                });
            }
        },
    );
    let session = baml_query_profiles::profiles_session(&root).await.unwrap();
    for (sql, expected) in [
        ("SELECT COUNT(*) FROM logs", "4"),
        (
            "SELECT COUNT(*) FROM logs_v1 WHERE distinct_id = 'customer-1'",
            "2",
        ),
        (
            "SELECT COUNT(*) FROM logs WHERE event_name = 'checkout'",
            "3",
        ),
        ("SELECT COUNT(*) FROM logs WHERE event_name IS NULL", "1"),
        (
            "SELECT COUNT(*) FROM calls WHERE distinct_id = 'customer-1' AND context['org_id'] = 'acme' AND context['attempt'] >= 3",
            "1",
        ),
        (
            "SELECT COUNT(*) FROM logs WHERE context['org_id'] = 'acme' AND context['attempt'] > 2",
            "2",
        ),
        (
            "SELECT COUNT(*) FROM logs WHERE data['result']['scores'][0] = 7 AND data['result']['scores'][1] = 9",
            "3",
        ),
        ("SELECT COUNT(*) FROM logs WHERE call_id IS NULL", "1"),
        (
            "SELECT COUNT(*) FROM logs l LEFT JOIN calls c ON l.execution_id = c.execution_id AND l.call_id = c.call_id WHERE c.call_id IS NULL",
            "4",
        ),
        ("SELECT COUNT(*) FROM logs WHERE timestamp IS NULL", "1"),
        (
            "SELECT COUNT(*) FROM (SELECT execution_id, log_id FROM logs GROUP BY execution_id, log_id)",
            "4",
        ),
        (
            "SELECT COUNT(*) FROM logs WHERE distinct_id = 'customer-1' GROUP BY distinct_id",
            "2",
        ),
        (
            "SELECT COUNT(*) FROM logs WHERE execution_id = 'not-this-execution'",
            "0",
        ),
    ] {
        let (rows, _) = one_column(&session, sql).await;
        assert_eq!(rows, vec![Some(expected.into())], "{sql}");
    }
    let execution_id = bex_prof_store::ids::ExecutionId(execution).encode();
    let (rows, _) = one_column(
        &session,
        &format!("SELECT COUNT(*) FROM logs WHERE execution_id = '{execution_id}'"),
    )
    .await;
    assert_eq!(rows, vec![Some("4".into())]);

    let (_, outcome) = one_column(&session, "SELECT data FROM logs WHERE log_id = 3").await;
    assert_ne!(outcome.result_state, baml_query::ResultState::Complete);
    let (_, outcome) = one_column(&session, "SELECT context FROM logs WHERE log_id = 2").await;
    assert_eq!(
        outcome.result_state,
        baml_query::ResultState::Complete,
        "{outcome:?}"
    );
    let (_, outcome) = one_column(&session, "SELECT context FROM logs WHERE log_id = 3").await;
    assert_ne!(outcome.result_state, baml_query::ResultState::Complete);

    let other = temp.path().join("other");
    write_store(&other, ProcessEuid([0xD2; 16]), 32);
    let other_session = baml_query_profiles::profiles_session(&other).await.unwrap();
    let (rows, _) = one_column(&other_session, "SELECT COUNT(*) FROM logs").await;
    assert_eq!(rows, vec![Some("0".into())]);
    let (rows, _) = one_column(&other_session, "SELECT distinct_id FROM calls").await;
    assert_eq!(rows, vec![None]);
    let (_, outcome) = one_column(&other_session, "SELECT context FROM calls").await;
    assert_ne!(outcome.result_state, baml_query::ResultState::Complete);

    let scopes = temp.path().join("scope-states");
    for (engine, context) in [
        (33, Ok(codec_fixture::empty_context())),
        (34, Err(ValueLossReason::CopyFailed)),
    ] {
        let execution = write_store_with(
            &scopes,
            ProcessEuid([u8::try_from(engine).unwrap(); 16]),
            engine,
            |session, handle, thread, emit| {
                let mut payload_id = None;
                assert!(session.publish_call_scope(
                    handle,
                    EncodedCallScope {
                        call_ref: CallRef {
                            process_euid: thread.process_euid,
                            engine_id: thread.engine_id,
                            thread_id: thread.thread_id,
                            call_id: BexCallId(6),
                        },
                        distinct_id: None,
                        context,
                    },
                    session.reserve_log_work().unwrap(),
                    |id| {
                        payload_id = Some(id);
                        true
                    },
                ));
                emit(RawRecord::CallScope {
                    payload_id: payload_id.unwrap(),
                });
            },
        );
        let session = baml_query_profiles::profiles_session(&scopes)
            .await
            .unwrap();
        let execution_id = bex_prof_store::ids::ExecutionId(execution).encode();
        let (rows, outcome) = one_column(&session, &format!(
            "SELECT context FROM calls WHERE distinct_id IS NULL AND execution_id = '{execution_id}'"
        )).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            outcome.result_state,
            if engine == 33 {
                baml_query::ResultState::Complete
            } else {
                baml_query::ResultState::Incomplete
            },
        );
    }
}
