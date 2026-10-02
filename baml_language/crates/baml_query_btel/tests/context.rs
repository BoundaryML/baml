//! Context queries against engine-produced files, including delayed CAS publication.
mod support;

use baml_query_btel::{Index, IndexOptions, QueryRequest};
use btel_reader::layout::SourceLayout;
use prost::Message as _;
use serde_json::json;
use support::*;

const SOURCE: &str = include_str!("../../baml_tests/baml_src/ns_trace_context/query.baml");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn context_queries_are_lazy_and_recover_without_new_recordings() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(project.path(), SOURCE, &[], &[("query_context_main", 7)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(14))]);
    let layout = SourceLayout::for_project(project.path());
    let mut files = Vec::new();
    for recording in std::fs::read_dir(&layout.recordings).unwrap() {
        for file in std::fs::read_dir(recording.unwrap().path()).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "btel") {
                files.push(
                    btel_recorder::proto::RecordingFile::decode(
                        std::fs::read(path).unwrap().as_slice(),
                    )
                    .unwrap(),
                );
            }
        }
    }
    let mut saved = std::collections::BTreeMap::new();
    for section in files
        .iter()
        .flat_map(|f| f.spans.iter().flat_map(|s| &s.sections))
    {
        if let btel_reader::context::ContextReference::Snapshot(id) =
            btel_reader::context::reference(section)
        {
            let path = layout.blob_path(id);
            saved
                .entry(path.clone())
                .or_insert_with(|| std::fs::read(&path).unwrap());
        }
    }
    assert!(!saved.is_empty());
    for path in saved.keys() {
        std::fs::remove_file(path).unwrap();
    }
    let mut index = index(project.path());
    let identity_sql = "SELECT COUNT(*) FROM spans WHERE span_name = 'user.query_context_leaf'
         AND context_distinct_id = 'query-user'";
    let missing = sql(&mut index, identity_sql);
    assert_eq!(missing.rows, vec![vec![json!(0)]]);
    let unavailable = sql(
        &mut index,
        "SELECT DISTINCT baml_kind(context_metadata) FROM spans
         WHERE span_name = 'user.query_context_leaf'",
    );
    assert_eq!(unavailable.rows, vec![vec![json!("unavailable")]]);
    for (path, bytes) in saved {
        std::fs::write(path, bytes).unwrap();
    }
    let refresh = index.refresh().unwrap();
    assert_eq!(refresh.files_applied, 0);
    let recovered = index
        .query(&QueryRequest {
            sql: identity_sql.into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(recovered.rows, vec![vec![json!(2)]]);
    assert_eq!(recovered.outcome.query.values.cas_loads, 0);
    let plan: Vec<String> = index
        .connection()
        .prepare(
            "EXPLAIN QUERY PLAN SELECT span_id FROM spans WHERE context_distinct_id = 'query-user'",
        )
        .unwrap()
        .query_map([], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter().any(|line| line.contains("context_by_identity")),
        "{plan:?}"
    );
    for relation in ["spans", "span_announcements"] {
        let metadata = sql(
            &mut index,
            &format!(
                "SELECT context_metadata['root'], context_metadata['phase'],
               context_metadata['count'], context_metadata['ratio'], context_metadata['enabled']
             FROM {relation} WHERE span_name = 'user.query_context_leaf'"
            ),
        );
        assert_eq!(
            metadata.rows,
            vec![
                vec![
                    json!("inherited"),
                    json!("entry"),
                    json!(42),
                    json!(1.5),
                    json!(true)
                ];
                2
            ]
        );
        assert_eq!(metadata.outcome.query.values.cas_loads, 1);
    }
    let future = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_type = 'future'
         AND context_distinct_id = 'query-user' AND context_metadata['root'] = 'inherited'",
    );
    assert_eq!(future.rows, vec![vec![json!(1)]]);

    let expected = sql(
        &mut index,
        "SELECT span_id, context_distinct_id, baml_kind(context_metadata), context_metadata
         FROM spans ORDER BY span_id",
    )
    .rows;
    drop(index);
    std::fs::remove_file(layout.root.join("query.sqlite")).unwrap();
    let mut options = IndexOptions::default();
    options.refresh.fresh_bytes = 0;
    options.refresh.batch_files = 1;
    options.refresh.batch_bytes = 1;
    let mut incremental = Index::open(layout, options).unwrap();
    assert_eq!(
        sql(
            &mut incremental,
            "SELECT span_id, context_distinct_id, baml_kind(context_metadata), context_metadata
         FROM spans ORDER BY span_id"
        )
        .rows,
        expected
    );
}
