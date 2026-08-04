//! BQL v1 end-to-end: synthetic sessions written by the REAL `bex_events`
//! writer (same fixture as `tests/fold.rs`), plus a revision dictionary so
//! name-glob stages resolve fqns, queried through `bql::run`.

use bex_events::dict::{ensure_dict_written, pb};
use bex_events::ids::{BexCallId, BexThreadId, FunctionId};
use bex_events::prof::cct::CctEngine;
use bex_events::prof::cct::session::{FsyncService, SessionWriter};
use bex_events::prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus};
use bex_query::bql::{self, BqlTable, ColData};
use bex_query::{ObserveEngine, bqf1};

fn encode(records: &[RawRecord<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; MAX_RECORD_LEN];
    for rec in records {
        let len = rec.encode(&mut buf);
        out.extend_from_slice(&buf[..len]);
    }
    out
}

/// main(fn16) → { leaf(fn17) err, leaf(fn17) ok } on thread 1, fixed ticks.
fn program(base_ts: u64) -> Vec<u8> {
    let t = |d: u64| base_ts + d;
    encode(&[
        RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: t(0),
            name: b"",
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(16),
            call_site: None,
            ts_ticks: t(1_000),
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(17),
            call_site: None,
            ts_ticks: t(2_000),
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Errored,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            ts_ticks: t(3_000),
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(17),
            call_site: None,
            ts_ticks: t(4_000),
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
            ts_ticks: t(5_000),
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            ts_ticks: t(6_000),
        },
        RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(1),
            ts_ticks: t(7_000),
        },
    ])
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bex-bql-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write the revision dictionary (fn16 → app.main, fn17 → app.leaf) for the
/// `[9; 32]` revision and return its `baml_rev_1_...` string form.
fn write_dict(baml_dir: &std::path::Path) -> String {
    let dict = pb::RevisionDictionaryV1 {
        identity: Some(pb::IdentitySection {
            revision_id: vec![9u8; 32],
            ..Default::default()
        }),
        functions: Some(pb::FunctionSection {
            functions: vec![
                pb::FunctionDictRow {
                    function_id: 16,
                    fqn: "app.main".to_string(),
                    ..Default::default()
                },
                pb::FunctionDictRow {
                    function_id: 17,
                    fqn: "app.leaf".to_string(),
                    ..Default::default()
                },
            ],
        }),
        ..Default::default()
    };
    let path = ensure_dict_written(&baml_dir.join("dict"), &dict).unwrap();
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

fn write_session(baml_dir: &std::path::Path, base_ts: u64, close_as_epoch: bool) {
    let revision = write_dict(baml_dir);
    let fsync = FsyncService::start();
    let mut writer = SessionWriter::create(
        baml_dir,
        [7; 16],
        1,
        1_700_000_000_000_000_000,
        (3, 1, 1, 1),
        [9; 32],
        &revision,
        &fsync,
    )
    .unwrap();
    let mut engine = CctEngine::new(32);
    engine.consume(&program(base_ts), &mut |t| t);
    let flush = engine.flush_window();
    writer
        .write_window(&flush, engine.nodes(), base_ts, base_ts + 7_000, 8)
        .unwrap();
    if close_as_epoch {
        writer.close_epoch(engine.nodes(), base_ts + 7_000).unwrap();
    } else {
        writer.close(base_ts + 7_000, "test").unwrap();
    }
}

fn session_dir(baml_dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(baml_dir.join("sessions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

fn session_key(baml_dir: &std::path::Path) -> String {
    session_dir(baml_dir)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn col_u64<'t>(table: &'t BqlTable, name: &str) -> &'t [u64] {
    match table.column(name) {
        Some(ColData::U64(v)) => v,
        other => panic!("column {name} should be U64, got {other:?}"),
    }
}

fn col_str<'t>(table: &'t BqlTable, name: &str) -> &'t [String] {
    match table.column(name) {
        Some(ColData::Str(v)) => v,
        other => panic!("column {name} should be Str, got {other:?}"),
    }
}

#[test]
fn top_by_total_ns_orders_rows_with_values() {
    let baml = scratch("top");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let table = bql::run(&mut engine, Some(&key), "ctx() | top(5, by=total_ns)").unwrap();
    assert_eq!(table.rows(), 2);
    assert_eq!(col_str(&table, "fn"), ["app.main", "app.leaf"]);
    assert_eq!(col_u64(&table, "total_ns"), [5_000, 2_000]);
    assert_eq!(col_u64(&table, "self_ns"), [3_000, 2_000]);
    assert_eq!(col_u64(&table, "calls"), [1, 2]);
    assert_eq!(col_u64(&table, "errors"), [0, 1]);
    // ×4-stride bucket upper bounds: 5 µs → 16 µs, 1 µs → 4 µs.
    assert_eq!(col_u64(&table, "p50_ns"), [16_000, 4_000]);
    assert!(table.footer.sealed);
    assert!(!table.footer.torn);
    assert!(table.footer.first_ts_ns > 0);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn calls_glob_filters_by_name_into_stats() {
    let baml = scratch("stats");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let table = bql::run(
        &mut engine,
        Some(&key),
        "ctx() | calls(fn=\"*leaf*\") | stats()",
    )
    .unwrap();
    assert_eq!(table.rows(), 1);
    assert_eq!(col_u64(&table, "calls"), [2]);
    assert_eq!(col_u64(&table, "errors"), [1]);
    assert_eq!(col_u64(&table, "total_ns"), [2_000]);
    assert_eq!(col_u64(&table, "p50_ns"), [4_000]);
    assert_eq!(col_u64(&table, "p95_ns"), [4_000]);

    // A glob that matches nothing is an honest empty result, not an error.
    let table = bql::run(
        &mut engine,
        Some(&key),
        "ctx() | calls(fn=\"*nope*\") | stats()",
    )
    .unwrap();
    assert_eq!(col_u64(&table, "calls"), [0]);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn errors_then_top_by_errors() {
    let baml = scratch("errors");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let table = bql::run(
        &mut engine,
        Some(&key),
        "ctx() | errors() | top(3, by=errors)",
    )
    .unwrap();
    assert_eq!(table.rows(), 1, "only the leaf node errored");
    assert_eq!(col_str(&table, "fn"), ["app.leaf"]);
    assert_eq!(col_u64(&table, "errors"), [1]);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn where_calls_filters_rows() {
    let baml = scratch("where");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let table = bql::run(
        &mut engine,
        Some(&key),
        "ctx() | where(calls > 1) | top(10, by=calls)",
    )
    .unwrap();
    assert_eq!(table.rows(), 1);
    assert_eq!(col_str(&table, "fn"), ["app.leaf"]);
    assert_eq!(col_u64(&table, "calls"), [2]);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn where_duration_literal_runs() {
    let baml = scratch("dur");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    // Duration literal on a percentile metric: parses, plans, and runs.
    let table = bql::run(&mut engine, Some(&key), "ctx() | where(p95 > 10ms)").unwrap();
    assert_eq!(table.rows(), 0, "both p95 estimates are far below 10ms");
    assert!(
        table.footer.sealed,
        "empty is explained, footer still ships"
    );

    let table = bql::run(&mut engine, Some(&key), "ctx() | where(p95 < 10ms)").unwrap();
    assert_eq!(table.rows(), 2);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn runs_last_24h_on_empty_history_is_empty_with_footer() {
    let baml = scratch("runs-empty");
    let mut engine = ObserveEngine::new(baml.clone());

    let table = bql::run(&mut engine, None, "runs(last=24h)").unwrap();
    assert_eq!(table.rows(), 0);
    assert!(table.column("run").is_some());
    assert!(table.column("status").is_some());
    assert!(table.footer.sealed);
    assert!(!table.footer.torn);

    // The empty table still encodes to a frame that carries its footer.
    let frame = table.to_frame(11);
    let view = bqf1::decode_frame(&frame).unwrap();
    assert_eq!(view.kind, bqf1::FrameKind::BqlTable as u16);
    assert_eq!(view.nrows, 1, "meta row only");
    let meta = view.col_str(view.cols.len() - 1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&meta[0]).unwrap();
    assert_eq!(parsed["rows"], 0);
    assert_eq!(parsed["footer"]["sealed"], true);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn run_id_coerces_to_ctx_with_degraded_note() {
    let baml = scratch("coerce");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let query = format!("run(id=\"{key}\") | top(2, by=total_ns)");
    let table = bql::run(&mut engine, None, &query).unwrap();
    assert_eq!(table.rows(), 2);
    assert!(
        table
            .footer
            .degraded
            .iter()
            .any(|n| n.contains("implicit ctx()")),
        "§8.4: the RunSet→CtxSet coercion is noted: {:?}",
        table.footer.degraded
    );
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn typed_errors_parse_type_unknown_and_bad_arg() {
    let baml = scratch("errors-typed");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let err = bql::run(&mut engine, Some(&key), "nonsense(").unwrap_err();
    assert_eq!(err.code, "E_PARSE");

    let err = bql::run(&mut engine, Some(&key), "top(5) | ctx()").unwrap_err();
    assert_eq!(err.code, "E_TYPE");
    assert!(
        err.message.contains("top"),
        "names the stage: {}",
        err.message
    );

    let err = bql::run(&mut engine, Some(&key), "frobnicate()").unwrap_err();
    assert_eq!(err.code, "E_UNKNOWN_STAGE");

    let err = bql::run(&mut engine, Some(&key), "ctx() | top(0)").unwrap_err();
    assert_eq!(err.code, "E_BAD_ARG");

    // ctx() with no run in scope names the remedy.
    let err = bql::run(&mut engine, None, "ctx()").unwrap_err();
    assert_eq!(err.code, "E_BAD_ARG");
    assert!(!err.remedy.is_empty());
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn torn_segment_sets_footer_torn() {
    let baml = scratch("torn");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);

    // Truncate the sealed segment mid-block: the seal trailer (48 B) plus a
    // slice of the last block go missing.
    let cct_dir = session_dir(&baml).join("cct");
    let seg = std::fs::read_dir(&cct_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "bamlseg"))
        .unwrap();
    let bytes = std::fs::read(&seg).unwrap();
    assert!(bytes.len() > 200, "fixture segment is non-trivial");
    std::fs::write(&seg, &bytes[..bytes.len() - 53]).unwrap();

    let mut engine = ObserveEngine::new(baml.clone());
    let table = bql::run(&mut engine, Some(&key), "ctx() | top(5, by=total_ns)").unwrap();
    assert!(
        table.footer.torn,
        "§8.4 footer honesty: torn tail is reported"
    );
    assert!(!table.footer.sealed);
    let _ = std::fs::remove_dir_all(&baml);
}

#[test]
fn sort_limit_and_rollup_compose() {
    let baml = scratch("compose");
    write_session(&baml, 10_000, false);
    let key = session_key(&baml);
    let mut engine = ObserveEngine::new(baml.clone());

    let table = bql::run(
        &mut engine,
        Some(&key),
        "ctx() | rollup(by=fn) | sort(by=calls, asc) | limit(1)",
    )
    .unwrap();
    assert_eq!(table.rows(), 1);
    assert_eq!(
        col_str(&table, "fn"),
        ["app.main"],
        "asc: fewest calls first"
    );

    // limit() applies to an already-tabled result too.
    let table = bql::run(
        &mut engine,
        Some(&key),
        "ctx() | top(5, by=calls) | limit(1)",
    )
    .unwrap();
    assert_eq!(table.rows(), 1);
    assert_eq!(col_str(&table, "fn"), ["app.leaf"]);
    let _ = std::fs::remove_dir_all(&baml);
}

// ---------------------------------------------------------------------------
// Value stages (§8.2 ValueSet / §8.4 get / §8.5 showcase queries)
// ---------------------------------------------------------------------------

/// One synthetic boundary with captured values: two `app.leaf` call
/// input/output pairs (identical outputs → one CID). Call 10's rows carry
/// the capture-time `function_id` (17); call 14's rows carry it only with
/// `all_capture_ids` (else they rely on the raw firehose written under
/// `with_raw` — the pre-id fallback join). Returns (baml_dir, run_key).
fn write_value_run(
    dir_tag: &str,
    with_raw: bool,
    all_capture_ids: bool,
    output_scale: i64,
) -> (std::path::PathBuf, String) {
    use bex_events::ids::BoundaryId;
    use bex_events::prof::cct::meta::{MetaRecord, MetaWriter};
    use bex_events::run::TraceCallKey;
    use bex_events::store::canon::{self, CanonValue, Presence};
    use bex_events::value::{
        ByteValueArtifactSink, DagRef, ValueCapture, ValueCaptureKind, ValueCodec, ValueWriter,
    };

    let baml_dir =
        std::env::temp_dir().join(format!("bql-values-{dir_tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&baml_dir);
    std::fs::create_dir_all(&baml_dir).unwrap();

    // Session (dict + sealed segment) — reuses the tally fixture.
    write_session(&baml_dir, 1_000_000, false);
    let session_name = session_key(&baml_dir);
    // The dict write is idempotent; re-derive the revision stem so the
    // boundary meta names the REAL dictionary (fn-name resolution).
    let revision = write_dict(&baml_dir);

    // Raw firehose: CallFunction records joining calls 10/14 → fn 17.
    if with_raw {
        use bex_events::prof::cct::raw::{RAW_HEADER_LEN, RAW_MAGIC, RAW_VERSION};
        let raw_dir = baml_dir.join("sessions").join(&session_name).join("raw");
        std::fs::create_dir_all(&raw_dir).unwrap();
        let mut file = vec![0u8; RAW_HEADER_LEN];
        file[0..8].copy_from_slice(&RAW_MAGIC);
        file[8..10].copy_from_slice(&RAW_VERSION.to_le_bytes());
        file[10..12].copy_from_slice(&u16::try_from(RAW_HEADER_LEN).unwrap().to_le_bytes());
        file[16..32].copy_from_slice(&[7; 16]);
        file[32..40].copy_from_slice(&1u64.to_le_bytes());
        file[40] = 3; // clock kind
        file[41] = 1;
        file[48..56].copy_from_slice(&1u64.to_le_bytes());
        file[56..64].copy_from_slice(&1u64.to_le_bytes());
        let mut frame = Vec::new();
        for (call, function) in [(1u64, 16u32), (10, 17), (14, 17)] {
            let rec = RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(call),
                parent_call_id: BexCallId(0),
                function_id: FunctionId(function),
                ts_ticks: 1_000 + call,
                call_site: None,
            };
            let mut buf = [0u8; MAX_RECORD_LEN];
            let n = rec.encode(&mut buf);
            frame.extend_from_slice(&buf[..n]);
        }
        file.extend_from_slice(&u32::try_from(frame.len()).unwrap().to_le_bytes());
        file.extend_from_slice(&frame);
        std::fs::write(raw_dir.join("raw-000001.bamlprof"), file).unwrap();
    }

    // Boundary meta: begin → bound(session) → complete.
    let boundary_id = BoundaryId::from_bytes([0xAB; 16]);
    let run_name = format!("1700000000000-main-{}", boundary_id.to_wire_string());
    let run_dir = baml_dir.join("history").join(&run_name);
    std::fs::create_dir_all(run_dir.join("thread-1")).unwrap();
    let mut meta = MetaWriter::create(run_dir.join("boundary.bamlmeta")).unwrap();
    meta.append(&MetaRecord::BoundaryBegin {
        boundary_id: boundary_id.to_wire_string(),
        target: "main".to_string(),
        source: "test".to_string(),
        created_ms: 1_700_000_000_000,
        project_id: "p".to_string(),
        revision_id: revision.clone(),
        capture_defaults: "llm_boundary".to_string(),
    })
    .unwrap();
    meta.append(&MetaRecord::BoundaryBound {
        session_dir: format!(".baml/sessions/{session_name}"),
        first_seg_seq: 0,
        partition_id: 1,
        boundary_local_id: 1,
    })
    .unwrap();
    meta.append(&MetaRecord::BoundaryComplete {
        status: "succeeded".to_string(),
        completed_ms: 1_700_000_000_100,
        last_seg_seq: 0,
        counts: serde_json::json!({}),
        diagnostics: Vec::new(),
        dump_refs: Vec::new(),
    })
    .unwrap();

    // Values: two input/output pairs into the CAS + .bamlvalue records.
    let mut store = bex_events::store::Store::open(&baml_dir.join("store"), [7; 16]).unwrap();
    let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).unwrap();
    let call_key = |call: u64| TraceCallKey {
        process_euid: bex_events::ids::ProcessEuid([7; 16]),
        engine_id: bex_events::ids::EngineId(1),
        thread_id: BexThreadId(1),
        call_id: BexCallId(call),
    };
    let put = |value: &CanonValue,
               kind: ValueCaptureKind,
               call: u64,
               function_id: u32,
               store: &mut bex_events::store::Store,
               writer: &mut ValueWriter<ByteValueArtifactSink>| {
        let encoded = canon::encode(value);
        store.put_encoded(&encoded, 1).unwrap();
        writer
            .append_body_with_capture_dag_and_promotion(
                ValueCodec::CanonicalDag,
                Vec::new(),
                Some(ValueCapture {
                    kind,
                    call: call_key(call),
                    function_id,
                }),
                Some(DagRef {
                    root_cid: encoded.root_cid,
                    node_codec_version: canon::NODE_CODEC_VERSION,
                    logical_len: encoded.logical_len,
                }),
                None,
            )
            .unwrap();
    };
    let item = |name: &str| CanonValue::Class {
        definition_key: "class:app.Item".to_string(),
        fields: vec![
            (
                "name".to_string(),
                Presence::Value,
                Some(CanonValue::String(name.to_string())),
            ),
            (
                "quantity".to_string(),
                Presence::Value,
                Some(CanonValue::Int(2)),
            ),
        ],
    };
    // Capture-carried fn identity: call 10's two rows always carry fn 17
    // (the capture-time stamp); call 14's rows carry it only in the
    // `all_capture_ids` variant, else 0 (pre-id records — resolved through
    // the raw-join fallback when a raw firehose exists).
    let call_14_fid = if all_capture_ids { 17 } else { 0 };
    put(
        &item("pour over"),
        ValueCaptureKind::CallInput,
        10,
        17,
        &mut store,
        &mut writer,
    );
    put(
        &item("croissant"),
        ValueCaptureKind::CallInput,
        14,
        call_14_fid,
        &mut store,
        &mut writer,
    );
    // Identical outputs (scaled by the fixture knob) → one shared CID.
    let output = CanonValue::Float(3.5 * output_scale as f64);
    put(
        &output,
        ValueCaptureKind::CallOutput,
        10,
        17,
        &mut store,
        &mut writer,
    );
    put(
        &output,
        ValueCaptureKind::CallOutput,
        14,
        call_14_fid,
        &mut store,
        &mut writer,
    );
    store.seal_active().unwrap();
    std::fs::write(
        run_dir.join("thread-1").join("value-0.bamlvalue"),
        writer.into_sink().bytes(),
    )
    .unwrap();

    (baml_dir, run_name)
}

#[test]
fn values_lists_hydrates_and_joins_fn_names() {
    let (baml_dir, run) = write_value_run("full", true, false, 1);
    let mut engine = ObserveEngine::new(baml_dir.clone());

    // Listing without get: rows + cids, honest note about bodies.
    let table = bql::run(&mut engine, Some(&run), "values(role=[input, output])").unwrap();
    assert_eq!(table.rows(), 4);
    assert!(table.footer.sealed && !table.footer.torn);

    // The exact-source fn join names the calls; fn= filters work.
    let table = bql::run(
        &mut engine,
        Some(&run),
        "values(role=[input, output], fn=\"app.leaf\") | get(max_bytes=8kb)",
    )
    .unwrap();
    assert_eq!(table.rows(), 4, "all four captures are app.leaf calls");
    let body_col = table
        .columns
        .iter()
        .find(|(name, _)| name == "body")
        .map(|(_, c)| c)
        .unwrap();
    let ColData::Json(bodies) = body_col else {
        panic!("body is a Json column")
    };
    assert!(
        bodies
            .iter()
            .any(|b| b.contains("\"quantity\":2") && b.contains("pour over")),
        "input hydrates through the CAS: {bodies:?}"
    );
    assert!(
        bodies.iter().any(|b| b == "3.5"),
        "output hydrates: {bodies:?}"
    );
}

#[test]
fn values_resolve_fn_from_capture_ids_without_raw() {
    // No raw firehose at all — every row carries its capture-time
    // function_id, so fn names resolve through the dictionary alone.
    let (baml_dir, run) = write_value_run("capids", false, true, 1);
    let mut engine = ObserveEngine::new(baml_dir.clone());

    let table = bql::run(
        &mut engine,
        Some(&run),
        "values(role=[input, output], fn=\"app.leaf\") | get(max_bytes=8kb)",
    )
    .unwrap();
    assert_eq!(table.rows(), 4, "capture-carried ids name all four rows");
    assert!(
        !table
            .footer
            .degraded
            .iter()
            .any(|n| n.contains("no exact source")),
        "ids on every row = exact coverage, no degraded note: {:?}",
        table.footer.degraded
    );
    let fn_col = col_str(&table, "fn");
    assert!(
        fn_col.iter().all(|f| f == "app.leaf"),
        "dictionary resolves fn 17: {fn_col:?}"
    );
    let ColData::Json(bodies) = table.column("body").unwrap() else {
        panic!("body is a Json column")
    };
    assert!(
        bodies
            .iter()
            .any(|b| b.contains("\"quantity\":2") && b.contains("pour over")),
        "bodies hydrate without any raw file: {bodies:?}"
    );

    // instances(source=values) names functions from the capture ids too.
    let table = bql::run(&mut engine, Some(&run), "instances(source=values)").unwrap();
    assert_eq!(table.rows(), 2);
    let fn_col = col_str(&table, "fn");
    assert!(
        fn_col.iter().all(|f| f == "app.leaf"),
        "instances name fns without raw: {fn_col:?}"
    );
    let _ = std::fs::remove_dir_all(&baml_dir);
}

#[test]
fn values_without_exact_source_gates_fn_filter() {
    let (baml_dir, run) = write_value_run("noraw", false, false, 1);
    let mut engine = ObserveEngine::new(baml_dir.clone());

    // Listing works (with a degraded note), but fn= fails closed.
    let table = bql::run(&mut engine, Some(&run), "values()").unwrap();
    assert_eq!(table.rows(), 4);
    assert!(
        table
            .footer
            .degraded
            .iter()
            .any(|n| n.contains("no exact source")),
        "degraded notes name the gap: {:?}",
        table.footer.degraded
    );

    let err = bql::run(
        &mut engine,
        Some(&run),
        "values(fn=\"app.leaf\") | get(max_bytes=8kb)",
    )
    .unwrap_err();
    assert_eq!(err.code, "E_NO_EXACT_SOURCE");
    assert!(err.remedy.contains("BAML_PROFILE_RAW"), "{}", err.remedy);
}

#[test]
fn stats_by_cid_shows_dedupe_and_instances_lists_calls() {
    let (baml_dir, run) = write_value_run("dedup", true, false, 1);
    let mut engine = ObserveEngine::new(baml_dir.clone());

    let table = bql::run(
        &mut engine,
        Some(&run),
        "values(role=output) | stats(by=cid)",
    )
    .unwrap();
    assert_eq!(table.rows(), 1, "identical outputs share one CID");
    let ColData::U64(n) = &table.columns.iter().find(|(c, _)| c == "n").unwrap().1 else {
        panic!("n column")
    };
    assert_eq!(n[0], 2, "the dedupe view counts both captures");

    let table = bql::run(&mut engine, Some(&run), "instances(source=values)").unwrap();
    assert_eq!(table.rows(), 2, "two distinct calls captured");

    // No captures at all → the honest exact-source error.
    let empty = std::env::temp_dir().join(format!("bql-values-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&empty);
    write_session(&empty, 1_000_000, false);
    let run_dir = empty
        .join("history")
        .join("1700000000001-x-baml_id_1_AAAAAAAAAAAAAAAAAAAAAA");
    std::fs::create_dir_all(&run_dir).unwrap();
    let mut engine = ObserveEngine::new(empty.clone());
    let err = bql::run(
        &mut engine,
        Some("1700000000001-x-baml_id_1_AAAAAAAAAAAAAAAAAAAAAA"),
        "instances()",
    )
    .unwrap_err();
    assert_eq!(err.code, "E_NO_EXACT_SOURCE");
    let _ = std::fs::remove_dir_all(&empty);
    let _ = std::fs::remove_dir_all(&baml_dir);
}

#[test]
fn vdiff_matches_inputs_and_compares_outputs() {
    let (dir_a, run_a) = write_value_run("vda", true, false, 1);
    // Same inputs, different outputs (scale 2) in a SEPARATE root; copy
    // run B's artifacts into root A so one engine sees both runs.
    let (dir_b, run_b) = write_value_run("vdb", true, false, 2);
    let from = dir_b.join("history").join(&run_b);
    let to_name = format!("{}9", &run_b[..run_b.len() - 1]); // distinct key
    let to = dir_a.join("history").join(&to_name);
    std::fs::create_dir_all(to.join("thread-1")).unwrap();
    std::fs::copy(from.join("boundary.bamlmeta"), to.join("boundary.bamlmeta")).unwrap();
    std::fs::copy(
        from.join("thread-1/value-0.bamlvalue"),
        to.join("thread-1/value-0.bamlvalue"),
    )
    .unwrap();
    // Merge B's store packs so B's output CIDs resolve in root A.
    for entry in std::fs::read_dir(dir_b.join("store/packs")).unwrap() {
        let p = entry.unwrap().path();
        std::fs::copy(&p, dir_a.join("store/packs").join(p.file_name().unwrap())).unwrap();
    }

    let mut engine = ObserveEngine::new(dir_a.clone());
    let table = bql::run(
        &mut engine,
        None,
        &format!("vdiff(a=\"{run_a}\", b=\"{to_name}\")"),
    )
    .unwrap();
    assert_eq!(table.rows(), 2, "two input-matched calls");
    let ColData::U32(equal) = &table
        .columns
        .iter()
        .find(|(c, _)| c == "outputs_equal")
        .unwrap()
        .1
    else {
        panic!("outputs_equal column")
    };
    assert!(
        equal.iter().all(|&e| e == 0),
        "the fix changed every output"
    );
    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

#[test]
fn get_budget_elides_and_reports() {
    use bex_events::store::canon::{CanonValue, Presence};
    let (baml_dir, run) = write_value_run("budget", true, false, 1);
    // Add one oversized capture: a 64 KiB string field.
    {
        use bex_events::ids::BoundaryId;
        use bex_events::run::TraceCallKey;
        use bex_events::store::canon;
        use bex_events::value::{
            ByteValueArtifactSink, DagRef, ValueCapture, ValueCaptureKind, ValueCodec, ValueWriter,
        };
        let big = CanonValue::Class {
            definition_key: "class:app.Doc".to_string(),
            fields: vec![(
                "body".to_string(),
                Presence::Value,
                Some(CanonValue::String("x".repeat(64 * 1024))),
            )],
        };
        let encoded = canon::encode(&big);
        let mut store = bex_events::store::Store::open(&baml_dir.join("store"), [8; 16]).unwrap();
        store.put_encoded(&encoded, 2).unwrap();
        store.seal_active().unwrap();
        let boundary_id = BoundaryId::from_bytes([0xAB; 16]);
        let mut writer = ValueWriter::new(ByteValueArtifactSink::new(), boundary_id).unwrap();
        writer
            .append_body_with_capture_dag_and_promotion(
                ValueCodec::CanonicalDag,
                Vec::new(),
                Some(ValueCapture {
                    kind: ValueCaptureKind::CallInput,
                    call: TraceCallKey {
                        process_euid: bex_events::ids::ProcessEuid([7; 16]),
                        engine_id: bex_events::ids::EngineId(1),
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(10),
                    },
                    function_id: 17,
                }),
                Some(DagRef {
                    root_cid: encoded.root_cid,
                    node_codec_version: canon::NODE_CODEC_VERSION,
                    logical_len: encoded.logical_len,
                }),
                None,
            )
            .unwrap();
        let run_dir = baml_dir.join("history").join(&run);
        std::fs::write(
            run_dir.join("thread-1").join("value-1.bamlvalue"),
            writer.into_sink().bytes(),
        )
        .unwrap();
    }

    let mut engine = ObserveEngine::new(baml_dir.clone());
    let table = bql::run(
        &mut engine,
        Some(&run),
        "values(role=input) | get(max_bytes=1kb)",
    )
    .unwrap();
    let ColData::Json(bodies) = &table.columns.iter().find(|(c, _)| c == "body").unwrap().1 else {
        panic!("body column")
    };
    assert!(
        bodies.iter().any(|b| b.contains("$elided")),
        "oversized subtree elided whole: {bodies:?}"
    );
    assert!(
        table.footer.degraded.iter().any(|n| n.contains("elided")),
        "elision is footer-visible: {:?}",
        table.footer.degraded
    );
    let _ = std::fs::remove_dir_all(&baml_dir);
}
