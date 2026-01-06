//! Incrementality testing infrastructure for the BAML compiler.
//!
//! This module provides utilities to verify that Salsa queries are properly
//! memoized and that editing files only recomputes the necessary queries.

mod scenarios;

use std::sync::{Arc, Mutex};

use baml_db::RootDatabase;
use salsa::{Database, Event, EventKind, Setter};

/// Test database wrapper with Salsa event logging for incrementality verification.
///
/// This wraps `RootDatabase` and captures all Salsa events, allowing tests to
/// verify which queries were executed (cache misses) vs which were cached (hits).
pub struct IncrementalTestDb {
    db: RootDatabase,
    events: Arc<Mutex<Vec<Event>>>,
}

impl IncrementalTestDb {
    /// Create a new test database with event logging enabled.
    pub fn new() -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let db = RootDatabase::new_with_event_callback({
            let events = events.clone();
            Box::new(move |event| {
                events.lock().unwrap().push(event);
            })
        });
        Self { db, events }
    }

    /// Get mutable access to the database for modifying inputs.
    pub fn db_mut(&mut self) -> &mut RootDatabase {
        &mut self.db
    }

    /// Get immutable access to the database for queries.
    pub fn db(&self) -> &RootDatabase {
        &self.db
    }

    /// Clear the event log.
    pub fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Execute a closure and return the names of all queries that were executed.
    ///
    /// This clears the event log before execution, runs the closure, then
    /// extracts all `WillExecute` events to get the list of query names.
    ///
    /// Returns: (closure_result, list_of_executed_query_names)
    pub fn log_executed<R>(&self, f: impl FnOnce(&RootDatabase) -> R) -> (R, Vec<String>) {
        self.clear_events();
        let result = f(&self.db);
        let executed = self.extract_executed_queries();
        (result, executed)
    }

    /// Extract the names of queries that were executed from the event log.
    fn extract_executed_queries(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::WillExecute { database_key } => {
                    let name = (&self.db as &dyn Database)
                        .ingredient_debug_name(database_key.ingredient_index());
                    Some(name.to_string())
                }
                _ => None,
            })
            .collect()
    }

    /// Execute a closure and assert that specific queries were executed a certain number of times.
    ///
    /// # Arguments
    ///
    /// * `f` - The closure to execute
    /// * `expected` - A slice of (query_name_substring, expected_count) pairs
    ///
    /// # Panics
    ///
    /// Panics if any expected count doesn't match the actual count.
    ///
    /// # Example
    ///
    /// ```ignore
    /// test_db.assert_executed(
    ///     |db| { baml_hir::file_items(db, file); },
    ///     &[
    ///         ("lex_file", 1),        // Should execute once
    ///         ("file_lowering", 0),  // Should NOT execute (cached)
    ///     ],
    /// );
    /// ```
    pub fn assert_executed<R>(
        &self,
        f: impl FnOnce(&RootDatabase) -> R,
        expected: &[(&str, usize)],
    ) -> R {
        let (result, executed) = self.log_executed(f);

        for (query_name, expected_count) in expected {
            let actual_count = executed.iter().filter(|s| s.contains(query_name)).count();
            assert_eq!(
                actual_count,
                *expected_count,
                "Query '{}' executed {} times, expected {} times.\n\
                 All executed queries:\n  {}",
                query_name,
                actual_count,
                expected_count,
                executed.join("\n  ")
            );
        }

        result
    }

    /// Execute a closure and assert that specific queries were NOT executed.
    ///
    /// This is a convenience method equivalent to `assert_executed` with count 0.
    pub fn assert_not_executed<R>(
        &self,
        f: impl FnOnce(&RootDatabase) -> R,
        query_names: &[&str],
    ) -> R {
        let expected: Vec<_> = query_names.iter().map(|&name| (name, 0)).collect();
        self.assert_executed(f, &expected)
    }

    /// Get the full list of executed query names from the last operation.
    ///
    /// Useful for debugging or for snapshot testing.
    pub fn last_executed(&self) -> Vec<String> {
        self.extract_executed_queries()
    }

    /// Get all events from the last operation (not just WillExecute).
    ///
    /// Useful for debugging the full Salsa behavior.
    pub fn all_events(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|e| format!("{:?}", e.kind))
            .collect()
    }
}

impl Default for IncrementalTestDb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use baml_db::baml_hir;

    use super::*;

    #[test]
    fn test_basic_event_logging() {
        let mut test_db = IncrementalTestDb::new();

        let file = test_db.db_mut().add_file(
            "test.baml",
            r#"
class Foo {
    name string
}
"#,
        );

        // First execution should run queries
        let (_, executed) = test_db.log_executed(|db| {
            let _ = baml_hir::file_items(db, file);
        });

        // Should have executed some queries
        assert!(!executed.is_empty(), "Expected some queries to execute");

        // Should include expected queries
        assert!(
            executed.iter().any(|s| s.contains("lex_file")),
            "Expected lex_file to execute. Got: {:?}",
            executed
        );
        assert!(
            executed.iter().any(|s| s.contains("parse_result")),
            "Expected parse_result to execute. Got: {:?}",
            executed
        );
    }

    #[test]
    fn test_caching_on_repeated_query() {
        let mut test_db = IncrementalTestDb::new();

        let file = test_db.db_mut().add_file(
            "test.baml",
            r#"
class Bar {
    value int
}
"#,
        );

        // First execution
        test_db.assert_executed(
            |db| {
                let _ = baml_hir::file_items(db, file);
            },
            &[("lex_file", 1), ("parse_result", 1), ("file_lowering", 1)],
        );

        // Second execution without any changes - everything should be cached
        test_db.assert_executed(
            |db| {
                let _ = baml_hir::file_items(db, file);
            },
            &[("lex_file", 0), ("parse_result", 0), ("file_lowering", 0)],
        );
    }

    #[test]
    fn test_editing_file_invalidates_queries() {
        let mut test_db = IncrementalTestDb::new();

        let file = test_db.db_mut().add_file(
            "test.baml",
            r#"
class Original {
    field string
}
"#,
        );

        // First execution
        let _ = test_db.log_executed(|db| {
            let _ = baml_hir::file_items(db, file);
        });

        // Modify the file
        file.set_text(test_db.db_mut()).to(r#"
class Modified {
    field string
}
"#
        .to_string());

        // After modification, queries should re-execute
        test_db.assert_executed(
            |db| {
                let _ = baml_hir::file_items(db, file);
            },
            &[
                ("lex_file", 1),      // Must re-lex
                ("parse_result", 1),  // Must re-parse
                ("file_lowering", 1), // Must rebuild item tree (class name changed)
            ],
        );
    }

    #[test]
    fn test_whitespace_only_change() {
        let mut test_db = IncrementalTestDb::new();

        let file = test_db.db_mut().add_file(
            "test.baml",
            r#"class Foo {
    name string
}"#,
        );

        // First execution
        let _ = test_db.log_executed(|db| {
            let _ = baml_hir::file_items(db, file);
        });

        // Add whitespace only
        file.set_text(test_db.db_mut()).to(r#"class Foo {
    name string
}

"#
        .to_string());

        // After whitespace change: lex and parse must re-run,
        // but ideally item_tree would be cached (if implementation is optimal)
        let (_, executed) = test_db.log_executed(|db| {
            let _ = baml_hir::file_items(db, file);
        });

        // At minimum, lex_file should re-execute
        assert!(
            executed.iter().any(|s| s.contains("lex_file")),
            "Expected lex_file to re-execute after whitespace change"
        );
    }
}
