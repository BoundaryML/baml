//! Captured-value edge cases on real recordings: omitted arguments, cycles,
//! missing and corrupt CAS blobs.
mod support;

use baml_query_btel::Status;
use serde_json::json;
use support::*;

/// The blob for a queried CAS ID, through the reader's own layout, so the
/// path follows the current blob format version.
fn blob_path(project: &std::path::Path, hex: &str) -> std::path::PathBuf {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    btel_reader::layout::SourceLayout::for_project(project).blob_path(
        btel_reader::SnapshotId::from_bytes(bytes.try_into().unwrap()),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn structured_comparisons_use_captured_content_across_blobs_and_queries() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob"), (1, "ann")]).await;
    let mut index = index(project.path());
    let equal = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Extract' AND args['customer'] = output['customer']",
    );
    assert_eq!(equal.rows, vec![vec![json!(3)]]);
    assert_eq!(equal.outcome.status, Status::Complete);
    assert!(equal.outcome.query.values.cas_loads > 0);
    let array = sql(
        &mut index,
        r#"SELECT COUNT(*) FROM calls WHERE fqn = 'user.Extract' AND args['customer']['tags'] = baml_value_json('["vip","bob"]')"#,
    );
    assert_eq!(array.rows, vec![vec![json!(1)]]);
    assert_eq!(array.outcome.status, Status::Complete);
    let join = sql(
        &mut index,
        "WITH x AS MATERIALIZED (SELECT call_id, output FROM calls WHERE fqn = 'user.Extract') SELECT COUNT(*) FROM x a JOIN x b ON a.output = b.output WHERE a.call_id < b.call_id",
    );
    assert_eq!(join.rows, vec![vec![json!(1)]]);
    assert_eq!(
        join.outcome.status,
        Status::Complete,
        "{:?}",
        join.outcome.diagnostics
    );
    assert_eq!(
        join.outcome.query.values.cas_loads, 2,
        "two distinct output blobs are hydrated once each"
    );
    let cycles = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Link' AND output = output",
    );
    assert_eq!(cycles.rows, vec![vec![json!(0)]]);
    assert_eq!(cycles.outcome.status, Status::Incomplete);
    assert_eq!(cycles.outcome.diagnostics[0].code, "comparison_cycle");

    let ids = sql(
        &mut index,
        "SELECT value_cas_id FROM calls WHERE fqn = 'user.Extract' LIMIT 1",
    );
    let id = ids.rows[0][0].as_str().unwrap();
    let path = blob_path(project.path(), id);
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let missing = sql(
        &mut index,
        "WITH x AS MATERIALIZED (SELECT output FROM calls WHERE fqn = 'user.Extract') SELECT COUNT(*) FROM x WHERE output = output",
    );
    assert_eq!(
        missing.outcome.status,
        Status::Incomplete,
        "even identical handles need evidence"
    );
    assert_eq!(missing.outcome.diagnostics[0].code, "cas_missing");
    std::fs::write(path, bytes).unwrap();
    let recovered = sql(
        &mut index,
        "WITH x AS MATERIALIZED (SELECT output FROM calls WHERE fqn = 'user.Extract') SELECT COUNT(*) FROM x WHERE output = output",
    );
    assert_eq!(recovered.rows, vec![vec![json!(3)]]);
    assert_eq!(recovered.outcome.status, Status::Complete);
    let metadata = sql(&mut index, "SELECT COUNT(*) FROM calls");
    assert_eq!(metadata.outcome.query.values.cas_loads, 0);

    let mut options = baml_query_btel::IndexOptions::default();
    options.values.render.max_depth = 0;
    let layout = btel_reader::layout::SourceLayout::for_project(project.path());
    let mut unrendered = baml_query_btel::Index::open(layout.clone(), options.clone()).unwrap();
    let independent = sql(
        &mut unrendered,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Extract' AND args['customer'] = output['customer']",
    );
    assert_eq!(independent.rows, vec![vec![json!(3)]]);
    assert_eq!(independent.outcome.status, Status::Complete);
    options.values.comparison.max_nodes = 1;
    let mut bounded = baml_query_btel::Index::open(layout, options).unwrap();
    let limited = sql(
        &mut bounded,
        "WITH x AS MATERIALIZED (SELECT output FROM calls WHERE fqn = 'user.Extract') SELECT COUNT(*) FROM x WHERE output = output",
    );
    assert_eq!(limited.outcome.status, Status::Incomplete);
    assert_eq!(limited.outcome.diagnostics[0].code, "comparison_limit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn omitted_arguments_and_cycles_have_defined_results() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(7, "bob")]).await;
    let mut index = index(project.path());
    let classify = sql(
        &mut index,
        "SELECT args['threshold'] AS threshold, args['threshold'] IS NULL AS absent, args
         FROM calls WHERE fqn = 'user.Classify'",
    );
    assert_eq!(
        classify.rows[0][0],
        json!(null),
        "omitted: absent, not a value"
    );
    assert_eq!(classify.rows[0][1], json!(1));
    assert_eq!(classify.rows[0][2]["threshold"], json!({"$omitted": true}));
    assert_eq!(classify.outcome.status, Status::Complete);

    let link = sql(
        &mut index,
        "SELECT output, output['next']['next']['next']['name'] AS far, args['node']['name'] AS name
         FROM calls WHERE fqn = 'user.Link'",
    );
    let output = &link.rows[0][0];
    assert_eq!(output["$class"], json!("Node"));
    assert!(
        output["$id"].is_u64(),
        "a cyclic object is labelled: {output}"
    );
    assert_eq!(output["next"], json!({"$ref": output["$id"]}));
    assert_eq!(
        link.rows[0][1],
        json!("bob"),
        "navigation follows the cycle"
    );
    assert_eq!(link.rows[0][2], json!("bob"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_and_corrupt_blobs_are_reported_not_answered() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob")]).await;
    let mut index = index(project.path());
    let ids = sql(
        &mut index,
        "SELECT args_cas_id, value_cas_id FROM calls WHERE fqn = 'user.Extract' ORDER BY started_at",
    );
    let (ann_args, bob_output) = (
        ids.rows[0][0].as_str().unwrap().to_owned(),
        ids.rows[1][1].as_str().unwrap().to_owned(),
    );
    std::fs::remove_file(blob_path(project.path(), &ann_args)).unwrap();
    let corrupt = blob_path(project.path(), &bob_output);
    let mut bytes = std::fs::read(&corrupt).unwrap();
    let at = bytes.windows(3).position(|w| w == b"bob").unwrap();
    bytes[at] = b'B';
    std::fs::write(&corrupt, bytes).unwrap();

    let result = sql(
        &mut index,
        "SELECT args['customer']['name'] AS who, output['customer']['name'] AS owner
         FROM calls WHERE fqn = 'user.Extract' ORDER BY started_at",
    );
    assert_eq!(
        result.rows[0][0],
        json!(null),
        "missing blob: no invented value"
    );
    assert_eq!(result.rows[0][1], json!("ann"));
    assert_eq!(result.rows[1][0], json!("bob"));
    assert_eq!(
        result.rows[1][1],
        json!(null),
        "corrupt blob: rejected by its id"
    );
    assert_eq!(result.outcome.status, Status::Incomplete);
    let codes: Vec<_> = result
        .outcome
        .diagnostics
        .iter()
        .map(|d| (d.code.as_str(), d.count))
        .collect();
    assert_eq!(codes, vec![("cas_id_mismatch", 1), ("cas_missing", 1)]);

    // Filters over unavailable values do not silently drop to "no match":
    // the outcome says the answer is incomplete.
    let filtered = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE args['customer']['name'] = 'ann' AND fqn = 'user.Extract'",
    );
    assert_eq!(filtered.rows[0][0], json!(0));
    assert_eq!(filtered.outcome.status, Status::Incomplete);
    // Metadata queries are unaffected and read no blob.
    let metadata = sql(&mut index, "SELECT COUNT(*) FROM calls");
    assert_eq!(metadata.outcome.status, Status::Complete);
    assert_eq!(metadata.outcome.query.values.cas_loads, 0);
}
