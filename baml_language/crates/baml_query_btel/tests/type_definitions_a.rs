//! Design A's own risks for recorded type definitions: definitions that
//! arrive in later files than the captures naming them, and collection while
//! many definitions are pending.

mod support;

use std::time::Duration;

use btel_recorder::RecordingConfig;
use serde_json::{Value as Json, json};
use support::*;

fn tiny_files() -> RecordingConfig {
    RecordingConfig {
        target_bytes: std::num::NonZeroUsize::new(1).unwrap(),
        ..RecordingConfig::default()
    }
}

/// Definitions of the classes a cell names: every `definition` and
/// `$definition` in it, by id.
fn definitions(value: &Json, out: &mut Vec<(String, Json)>) {
    match value {
        Json::Object(map) => {
            for (key, item) in map {
                if key == "definition" || key == "$definition" {
                    let id = map
                        .get(if key == "definition" { "def" } else { "$def" })
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    out.push((id, item.clone()));
                }
                definitions(item, out);
            }
        }
        Json::Array(items) => items.iter().for_each(|item| definitions(item, out)),
        _ => {}
    }
}

const MANY: &str = r#"
function Use<T>(x: T) -> int { 1 }
function MakeAndUse(i: int) -> int {
    let b = reflect.class.builder("Temp");
    b.field("i", reflect.Type.of<int>());
    let t = b.build();
    type Temp = unreflect(t.as_type())
    Use<Temp[]>([])
}
function main(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        total = total + MakeAndUse(i);
        if (i % 5 == 0) { baml.sys.collect_garbage(); }
        i = i + 1;
    }
    total
}
"#;

/// Many runtime classes, each collected soon after the one capture naming
/// it, recorded into a file per batch: the run completes (no deadlock
/// between collection, the resolver and transport), every definition
/// survives, and definitions resolve across files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn collections_with_pending_definitions_lose_none_across_files() {
    let project = tempfile::tempdir().unwrap();
    let results = tokio::time::timeout(
        Duration::from_secs(120),
        record_program_with(
            project.path(),
            MANY,
            &["Use"],
            &[("main", 400)],
            tiny_files(),
        ),
    )
    .await
    .expect("no deadlock");
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let files = std::fs::read_dir(project.path().join(".baml/btel/recordings"))
        .map(Iterator::count)
        .unwrap_or(0);
    assert!(files > 0);
    let mut index = index(project.path());
    let uses = sql(
        &mut index,
        "SELECT type_args FROM spans WHERE span_name = 'user.Use'",
    );
    assert_eq!(uses.rows.len(), 400);
    assert_eq!(uses.outcome.status, baml_query_btel::Status::Complete);
    let temp = json!({"kind": "class", "name": "Temp", "fields": [{"name": "i", "schema": {"type": "int"}}]});
    let mut ids = std::collections::HashSet::new();
    for row in &uses.rows {
        let mut found = Vec::new();
        definitions(&row[0], &mut found);
        let [(id, definition)] = found.as_slice() else {
            panic!("one definition per cell: {}", row[0]);
        };
        assert_eq!(definition, &temp);
        ids.insert(id.clone());
    }
    // Each runtime class is its own declaration: 400 distinct identities.
    assert_eq!(ids.len(), 400);
    let count = |state: i64| -> i64 {
        index
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM type_def WHERE state = ?1",
                [state],
                |row| row.get(0),
            )
            .unwrap()
    };
    assert_eq!((count(2), count(1)), (400, 0), "recorded, unavailable");
}

const NESTED: &str = r#"
function Use<T>(x: T) -> int { 1 }
function MakeAndUse(inner: reflect.class.Type, i: int) -> int {
    let b = reflect.class.builder("Outer");
    b.field("inner", inner.as_type());
    b.field("i", reflect.Type.of<int>());
    let t = b.build();
    type Outer = unreflect(t.as_type())
    Use<Outer[]>([])
}
function main(n: int) -> int {
    let ib = reflect.class.builder("Inner");
    ib.field("x", reflect.Type.of<int>());
    let inner = ib.build();
    let total = 0;
    let i = 0;
    while (i < n) {
        total = total + MakeAndUse(inner, i);
        baml.sys.collect_garbage();
        i = i + 1;
    }
    total
}
"#;

/// A runtime class collected right after its one capture names another
/// runtime class that survives (and moves): the dying class's definition
/// names the survivor by its real name, and the survivor's own definition is
/// recorded too, in every cell.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dying_class_names_a_surviving_runtime_class_correctly() {
    let project = tempfile::tempdir().unwrap();
    let results = tokio::time::timeout(
        Duration::from_secs(120),
        record_program_with(
            project.path(),
            NESTED,
            &["Use"],
            &[("main", 60)],
            tiny_files(),
        ),
    )
    .await
    .expect("no deadlock");
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut index = index(project.path());
    let uses = sql(
        &mut index,
        "SELECT type_args FROM spans WHERE span_name = 'user.Use'",
    );
    assert_eq!(uses.rows.len(), 60);
    let inner = json!({"kind": "class", "name": "Inner", "fields": [{"name": "x", "schema": {"type": "int"}}]});
    let mut inner_ids = std::collections::HashSet::new();
    for row in &uses.rows {
        let outer = &row[0]["T"]["$type"]["item"];
        assert_eq!(outer["name"], json!("Outer"), "{}", row[0]);
        let field = &outer["definition"]["fields"][0];
        assert_eq!(field["name"], json!("inner"));
        let schema = &field["schema"];
        assert_eq!(
            schema["name"],
            json!("Inner"),
            "a stale head reads a tombstone: {schema}"
        );
        assert_eq!(schema["definition"], inner, "{schema}");
        inner_ids.insert(schema["def"].as_str().unwrap().to_owned());
    }
    assert_eq!(inner_ids.len(), 1, "one Inner throughout");
}
