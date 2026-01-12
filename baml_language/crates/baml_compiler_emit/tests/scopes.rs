//! Compiler tests for local variable scoping.
//!
//! These tests verify that `locals_in_scope` tracking is correct
//! for debugging purposes.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use baml_base::{FileId, SourceFile};

/// Minimal test database for compilation tests.
#[salsa::db]
#[derive(Clone)]
struct TestDatabase {
    storage: salsa::Storage<Self>,
    next_file_id: Arc<AtomicU32>,
    project: Option<baml_workspace::Project>,
}

#[salsa::db]
impl salsa::Database for TestDatabase {}

#[salsa::db]
impl baml_workspace::Db for TestDatabase {
    fn project(&self) -> baml_workspace::Project {
        self.project.expect("project must be set before querying")
    }
}

#[salsa::db]
impl baml_compiler_hir::Db for TestDatabase {}

#[salsa::db]
impl baml_compiler_tir::Db for TestDatabase {}

#[salsa::db]
impl baml_compiler_mir::Db for TestDatabase {}

impl TestDatabase {
    fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            next_file_id: Arc::new(AtomicU32::new(0)),
            project: None,
        }
    }

    fn add_file(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceFile {
        let file_id = FileId::new(
            self.next_file_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        SourceFile::new(self, text.into(), path.into(), file_id)
    }

    fn set_project(&mut self, files: Vec<SourceFile>) {
        let project = baml_workspace::Project::new(self, PathBuf::new(), files);
        self.project = Some(project);
    }
}

/// Compile source and return the `locals_in_scope` for a given function.
fn get_locals_in_scope(source: &str, function_name: &str) -> Vec<Vec<String>> {
    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    db.set_project(vec![file]);

    let program = baml_compiler_emit::compile_files(&db, &[file])
        .expect("compile_files should succeed for valid test source");

    // Find the function
    let obj_idx = program
        .function_indices
        .get(function_name)
        .expect("function not found");

    let func = program
        .objects
        .get(*obj_idx)
        .and_then(|obj| {
            if let baml_vm_types::Object::Function(f) = obj {
                Some(f)
            } else {
                None
            }
        })
        .expect("expected Function object");

    func.locals_in_scope.clone()
}

#[test]
#[ignore = "MIR codegen does not yet track locals_in_scope"]
fn locals_in_scope() {
    let source = r#"
        function main() -> int {
            let x = 0;

            let a = {
                let y = 0;

                let b = {
                    let c = 1;
                    let d = 2;
                    [c, d]
                };
                let e = {
                    let f = 4;
                    let g = 5;
                    [f, g]
                };

                [b, e]
            };

            let h = {
                let z = 0;

                let i = {
                    let w = 0;
                    let j = 8;
                    [w, j]
                };

                [i]
            };

            0
        }
    "#;

    let locals_in_scope = get_locals_in_scope(source, "main");

    // Note: The new compiler creates an initial scope for the function itself,
    // then a separate scope for the function body. This is slightly different
    // from the old compiler which combined them.
    let expected: Vec<Vec<&str>> = vec![
        vec!["<fn main>"],                          // scope 0: function entry
        vec!["<fn main>", "x", "a", "h"],           // scope 1: function body
        vec!["<fn main>", "x", "y", "b", "e"],      // scope 2: first nested block (a = { ... })
        vec!["<fn main>", "x", "y", "c", "d"],      // scope 3: b = { ... }
        vec!["<fn main>", "x", "y", "b", "f", "g"], // scope 4: e = { ... }
        vec!["<fn main>", "x", "a", "z", "i"],      // scope 5: h = { ... }
        vec!["<fn main>", "x", "a", "z", "w", "j"], // scope 6: i = { ... }
    ];

    assert_eq!(
        locals_in_scope.len(),
        expected.len(),
        "Number of scopes mismatch"
    );

    for (i, (actual, expected)) in locals_in_scope.iter().zip(expected.iter()).enumerate() {
        let expected_strings: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(actual, &expected_strings, "Scope {i} mismatch");
    }
}
