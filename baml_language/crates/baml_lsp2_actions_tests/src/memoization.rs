//! Pins the B1 memoization contract: `file_annotations` and
//! `semantic_tokens` are Salsa tracked queries, so an unchanged file never
//! recomputes them and a source edit invalidates them. Assertions count
//! `WillExecute` events rather than inspecting results, so implementation
//! rewrites of either query keep these tests meaningful.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use baml_lsp2_actions::{annotations::file_annotations, tokens::semantic_tokens};
use baml_project::ProjectDatabase;

type EventLog = Arc<Mutex<Vec<salsa::Event>>>;

const SOURCE_V1: &str = r#"
function Echo(value: int) -> int {
    value
}

function Main() -> int {
    let result = Echo(1)
    result
}
"#;

const SOURCE_V2: &str = r#"
function Echo(value: string) -> string {
    value
}

function Main() -> string {
    let result = Echo("changed")
    result
}
"#;

fn test_db() -> (ProjectDatabase, EventLog, PathBuf) {
    let events = EventLog::default();
    let callback_events = Arc::clone(&events);
    let mut db = ProjectDatabase::new_with_event_callback(Box::new(move |event| {
        callback_events
            .lock()
            .expect("event log mutex poisoned")
            .push(event);
    }));
    db.set_project_root(Path::new("/test"));
    let path = PathBuf::from("/test/main.baml");
    db.add_or_update_file(&path, SOURCE_V1);
    (db, events, path)
}

fn clear_events(events: &EventLog) {
    events.lock().expect("event log mutex poisoned").clear();
}

fn query_execution_count(db: &ProjectDatabase, events: &EventLog, query_name: &str) -> usize {
    events
        .lock()
        .expect("event log mutex poisoned")
        .iter()
        .filter(|event| {
            let salsa::EventKind::WillExecute { database_key } = &event.kind else {
                return false;
            };
            let name =
                (db as &dyn salsa::Database).ingredient_debug_name(database_key.ingredient_index());
            name == query_name || name.ends_with(&format!("::{query_name}"))
        })
        .count()
}

#[test]
fn file_annotations_are_memoized_and_invalidated_by_source_edits() {
    let (mut db, events, path) = test_db();
    let file = db.get_file(&path).expect("test file should exist");

    clear_events(&events);
    let cold = file_annotations(&db, file).clone();
    assert_eq!(query_execution_count(&db, &events, "file_annotations"), 1);

    clear_events(&events);
    let warm = file_annotations(&db, file).clone();
    assert_eq!(warm, cold);
    assert_eq!(query_execution_count(&db, &events, "file_annotations"), 0);

    db.add_or_update_file(&path, SOURCE_V2);
    clear_events(&events);
    let changed = file_annotations(&db, file).clone();
    assert_ne!(changed, cold);
    assert_eq!(query_execution_count(&db, &events, "file_annotations"), 1);
}

#[test]
fn semantic_tokens_are_memoized_and_invalidated_by_source_edits() {
    let (mut db, events, path) = test_db();
    let file = db.get_file(&path).expect("test file should exist");

    clear_events(&events);
    let cold = semantic_tokens(&db, file).clone();
    assert_eq!(query_execution_count(&db, &events, "semantic_tokens"), 1);

    clear_events(&events);
    let warm = semantic_tokens(&db, file).clone();
    assert_eq!(warm, cold);
    assert_eq!(query_execution_count(&db, &events, "semantic_tokens"), 0);

    db.add_or_update_file(&path, SOURCE_V2);
    clear_events(&events);
    let changed = semantic_tokens(&db, file).clone();
    assert_ne!(changed, cold);
    assert_eq!(query_execution_count(&db, &events, "semantic_tokens"), 1);
}
