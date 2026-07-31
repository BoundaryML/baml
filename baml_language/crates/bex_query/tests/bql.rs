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
