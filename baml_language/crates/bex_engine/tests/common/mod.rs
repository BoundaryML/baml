//! Shared test utilities for `BexEngine` testing.
//!
//! This module provides common infrastructure for testing async execution
//! of BAML programs through `bex_engine`.

// Allow dead code since not all test files use all utilities
#![allow(dead_code)]

use std::{collections::HashMap, io::Write};

use baml_snapshot::BamlSnapshot;
use baml_tests::bytecode::compile_source_with_schema;
use bex_engine::{BexEngine, Snapshot, Ty, TypedSnapshot};
use indexmap::IndexMap;
use tempfile::TempDir;

/// Test input for engine execution (value-only checks).
pub(crate) struct EngineProgram {
    /// Virtual filesystem: maps relative paths to file contents.
    pub fs: IndexMap<&'static str, &'static str>,
    /// The BAML source code to compile and execute.
    pub source: &'static str,
    /// The function name to execute.
    pub entry: &'static str,
    /// Expected result: Ok(value) for success, Err(message) for expected error.
    pub expected: Result<Snapshot, &'static str>,
}

impl Default for EngineProgram {
    fn default() -> Self {
        Self {
            fs: IndexMap::new(),
            source: "",
            entry: "main",
            expected: Ok(Snapshot::Null),
        }
    }
}

/// Test input for engine execution with type checking.
pub(crate) struct TypedEngineProgram {
    /// Virtual filesystem: maps relative paths to file contents.
    pub fs: IndexMap<&'static str, &'static str>,
    /// The BAML source code to compile and execute.
    pub source: &'static str,
    /// The function name to execute.
    pub entry: &'static str,
    /// Expected result: `Ok(typed_value)` for success, Err(message) for expected error.
    pub expected: Result<TypedSnapshot, &'static str>,
}

impl Default for TypedEngineProgram {
    fn default() -> Self {
        Self {
            fs: IndexMap::new(),
            source: "",
            entry: "main",
            expected: Ok(TypedSnapshot::new(Snapshot::Null, Ty::Null)),
        }
    }
}

/// Compile BAML source code into a snapshot with schema populated.
pub(crate) fn compile_for_engine(source: &str) -> BamlSnapshot {
    compile_source_with_schema(source)
}

/// Set up the virtual filesystem for a test.
fn setup_virtual_fs(fs: &IndexMap<&'static str, &'static str>) -> anyhow::Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();

    for (path, contents) in fs {
        let full_path = root.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&full_path)?;
        file.write_all(contents.as_bytes())?;
    }

    Ok(temp_dir)
}

/// Assert that engine execution produces the expected value.
pub(crate) async fn assert_engine_executes(input: EngineProgram) -> anyhow::Result<()> {
    let temp_dir = setup_virtual_fs(&input.fs)?;
    let root_path = temp_dir.path().display().to_string();
    let source = input.source.replace("{ROOT}", &root_path);

    let snapshot = compile_for_engine(&source);
    let engine = BexEngine::new(snapshot, HashMap::new()).expect("Failed to create engine");

    let result = engine.call_function(input.entry, &[]).await;

    match (result, input.expected) {
        (Ok(typed_value), Ok(expected)) => {
            let typed_snapshot = engine
                .to_typed_snapshot(typed_value)
                .expect("Failed to convert result to snapshot");
            assert_eq!(
                typed_snapshot.value, expected,
                "Value mismatch for function '{}'",
                input.entry
            );
        }
        (Err(e), Err(expected_msg)) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains(expected_msg),
                "Expected error containing '{expected_msg}', got: {error_msg}"
            );
        }
        (Ok(typed_value), Err(expected_msg)) => {
            panic!(
                "Expected error containing '{expected_msg}', but got success: {:?}",
                typed_value.value
            );
        }
        (Err(e), Ok(expected)) => {
            panic!("Expected success with {expected:?}, but got error: {e}");
        }
    }

    Ok(())
}

/// Assert that engine execution produces the expected typed value (value + type).
pub(crate) async fn assert_engine_typed(input: TypedEngineProgram) -> anyhow::Result<()> {
    let temp_dir = setup_virtual_fs(&input.fs)?;
    let root_path = temp_dir.path().display().to_string();
    let source = input.source.replace("{ROOT}", &root_path);

    let snapshot = compile_for_engine(&source);
    let engine = BexEngine::new(snapshot, HashMap::new()).expect("Failed to create engine");

    let result = engine.call_function(input.entry, &[]).await;

    match (result, input.expected) {
        (Ok(typed_value), Ok(expected)) => {
            let typed_snapshot = engine
                .to_typed_snapshot(typed_value)
                .expect("Failed to convert result to snapshot");
            assert_eq!(
                typed_snapshot.value, expected.value,
                "Value mismatch for function '{}'",
                input.entry
            );
            assert_eq!(
                typed_snapshot.declared_type, expected.declared_type,
                "Type mismatch for function '{}': expected {:?}, got {:?}",
                input.entry, expected.declared_type, typed_snapshot.declared_type
            );
        }
        (Err(e), Err(expected_msg)) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains(expected_msg),
                "Expected error containing '{expected_msg}', got: {error_msg}"
            );
        }
        (Ok(typed_value), Err(expected_msg)) => {
            panic!(
                "Expected error containing '{expected_msg}', but got success: {:?}",
                typed_value.value
            );
        }
        (Err(e), Ok(expected)) => {
            panic!("Expected success with {expected:?}, but got error: {e}");
        }
    }

    Ok(())
}
