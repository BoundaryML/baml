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
    btel_reader::layout::SourceLayout::for_project(project)
        .blob_path(btel_reader::CasId::from_bytes(bytes.try_into().unwrap()))
}

/// Captured inputs and value CAS ids of a function's calls, oldest first.
/// The public tables expose values, not their storage.
fn cas_ids(index: &mut baml_query_btel::Index, fqn: &str) -> Vec<(Option<String>, Option<String>)> {
    index.refresh().unwrap();
    index
        .connection()
        .prepare(
            "SELECT lower(hex(c.inputs_cas)), lower(hex(c.value_cas)) FROM call c
             JOIN call_path p ON p.rec = c.rec AND p.call_path_id = c.call_path_id
             JOIN function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
             WHERE f.fqn = ?1 ORDER BY c.entered_ticks",
        )
        .unwrap()
        .query_map([fqn], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn structured_comparisons_use_captured_content_across_blobs_and_queries() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob"), (1, "ann")]).await;
    let mut index = index(project.path());
    let equal = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Extract' AND input_args['customer'] = output_value['customer']",
    );
    assert_eq!(equal.rows, vec![vec![json!(3)]]);
    assert_eq!(equal.outcome.status, Status::Complete);
    assert!(equal.outcome.query.values.cas_loads > 0);
    let array = sql(
        &mut index,
        r#"SELECT COUNT(*) FROM spans WHERE span_name = 'user.Extract' AND input_args['customer']['tags'] = baml_value_json('["vip","bob"]')"#,
    );
    assert_eq!(array.rows, vec![vec![json!(1)]]);
    assert_eq!(array.outcome.status, Status::Complete);
    let join = sql(
        &mut index,
        "WITH x AS MATERIALIZED (SELECT span_id, output_value FROM spans WHERE span_name = 'user.Extract') SELECT COUNT(*) FROM x a JOIN x b ON a.output_value = b.output_value WHERE a.span_id < b.span_id",
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
        "two distinct output_value blobs are hydrated once each"
    );
    let cycles = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Link' AND output_value = output_value",
    );
    assert_eq!(cycles.rows, vec![vec![json!(0)]]);
    assert_eq!(cycles.outcome.status, Status::Incomplete);
    assert_eq!(cycles.outcome.diagnostics[0].code, "comparison_cycle");

    let ids = cas_ids(&mut index, "user.Extract");
    let path = blob_path(project.path(), ids[0].1.as_deref().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let missing = sql(
        &mut index,
        "WITH x AS MATERIALIZED (SELECT output_value FROM spans WHERE span_name = 'user.Extract') SELECT COUNT(*) FROM x WHERE output_value = output_value",
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
        "WITH x AS MATERIALIZED (SELECT output_value FROM spans WHERE span_name = 'user.Extract') SELECT COUNT(*) FROM x WHERE output_value = output_value",
    );
    assert_eq!(recovered.rows, vec![vec![json!(3)]]);
    assert_eq!(recovered.outcome.status, Status::Complete);
    let metadata = sql(&mut index, "SELECT COUNT(*) FROM spans");
    assert_eq!(metadata.outcome.query.values.cas_loads, 0);

    let mut options = baml_query_btel::IndexOptions::default();
    options.values.render.max_depth = 0;
    let layout = btel_reader::layout::SourceLayout::for_project(project.path());
    let mut unrendered = baml_query_btel::Index::open(layout.clone(), options.clone()).unwrap();
    let independent = sql(
        &mut unrendered,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Extract' AND input_args['customer'] = output_value['customer']",
    );
    assert_eq!(independent.rows, vec![vec![json!(3)]]);
    assert_eq!(independent.outcome.status, Status::Complete);
    options.values.comparison.max_nodes = 1;
    let mut bounded = baml_query_btel::Index::open(layout, options).unwrap();
    let limited = sql(
        &mut bounded,
        "WITH x AS MATERIALIZED (SELECT output_value FROM spans WHERE span_name = 'user.Extract') SELECT COUNT(*) FROM x WHERE output_value = output_value",
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
        "SELECT input_args['threshold'] AS threshold, input_args['threshold'] IS NULL AS absent, input_args
         FROM spans WHERE span_name = 'user.Classify'",
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
        "SELECT output_value, output_value['next']['next']['next']['name'] AS far, input_args['node']['name'] AS name
         FROM spans WHERE span_name = 'user.Link'",
    );
    let output_value = &link.rows[0][0];
    assert_eq!(output_value["$class"], json!("Node"));
    assert!(
        output_value["$id"].is_u64(),
        "a cyclic object is labelled: {output_value}"
    );
    assert_eq!(output_value["next"], json!({"$ref": output_value["$id"]}));
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
    let ids = cas_ids(&mut index, "user.Extract");
    let (ann_args, bob_output) = (ids[0].0.clone().unwrap(), ids[1].1.clone().unwrap());
    std::fs::remove_file(blob_path(project.path(), &ann_args)).unwrap();
    let corrupt = blob_path(project.path(), &bob_output);
    let mut bytes = std::fs::read(&corrupt).unwrap();
    let at = bytes.windows(3).position(|w| w == b"bob").unwrap();
    bytes[at] = b'B';
    std::fs::write(&corrupt, bytes).unwrap();

    let result = sql(
        &mut index,
        "SELECT input_args['customer']['name'] AS who, output_value['customer']['name'] AS owner
         FROM spans WHERE span_name = 'user.Extract' ORDER BY start_time",
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
        "SELECT COUNT(*) FROM spans WHERE input_args['customer']['name'] = 'ann' AND span_name = 'user.Extract'",
    );
    assert_eq!(filtered.rows[0][0], json!(0));
    assert_eq!(filtered.outcome.status, Status::Incomplete);
    // Metadata queries are unaffected and read no blob.
    let metadata = sql(&mut index, "SELECT COUNT(*) FROM spans");
    assert_eq!(metadata.outcome.status, Status::Complete);
    assert_eq!(metadata.outcome.query.values.cas_loads, 0);
}

/// The size of every blob in the project's CAS, smallest first.
fn blob_sizes(project: &std::path::Path) -> Vec<u64> {
    let mut sizes = Vec::new();
    let mut pending = vec![btel_reader::layout::SourceLayout::for_project(project).cas];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                sizes.push(metadata.len());
            }
        }
    }
    sizes.sort_unstable();
    sizes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_strings_are_stored_once_and_queries_read_through_them() {
    let project = tempfile::tempdir().unwrap();
    let source = r#"
        function Echo(parts: string[]) -> string[] { parts }
        function main(n: int) -> int {
            let chunk = "x";
            let i = 0;
            while (i < 15) {
                chunk = chunk + chunk;
                i = i + 1;
            }
            let last = if (n == 0) { "c" } else { "d" };
            Echo([chunk + "a", chunk + "b", chunk + last]);
            n
        }
    "#;
    let results = record_program(
        project.path(),
        source,
        &["Echo"],
        &[("main", 0), ("main", 1)],
    )
    .await;
    assert_eq!(
        results,
        vec![
            Ok(bex_engine::BexExternalValue::Int(0)),
            Ok(bex_engine::BexExternalValue::Int(1)),
        ]
    );
    // Four distinct 32 KiB strings, each stored once, and per call a small
    // inputs root and a small output root that name them.
    let sizes = blob_sizes(project.path());
    let (small, large): (Vec<u64>, Vec<u64>) = sizes.iter().partition(|size| **size < 1024);
    assert_eq!(small.len(), 4, "{sizes:?}");
    assert_eq!(large.len(), 4, "{sizes:?}");
    assert!(large.iter().all(|size| (32 << 10..33 << 10).contains(size)));

    let mut index = index(project.path());
    let result = sql(
        &mut index,
        "SELECT length(input_args['parts'][0]) AS first, output_value[2] LIKE '%c' AS third_is_c,
           output_value = input_args['parts'] AS echoed
         FROM spans WHERE span_name = 'user.Echo' ORDER BY start_time",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![json!((32 << 10) + 1), json!(1), json!(1)],
            vec![json!((32 << 10) + 1), json!(0), json!(1)],
        ]
    );
    assert_eq!(
        result.outcome.status,
        Status::Complete,
        "{:?}",
        result.outcome.diagnostics
    );
}
