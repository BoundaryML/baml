//! Shared test utilities for `BexEngine` testing.
//!
//! This module provides common infrastructure for testing async execution
//! of BAML programs through `bex_engine`.

// Allow dead code since not all test files use all utilities
#![allow(dead_code)]

use std::{collections::HashMap, io::Write};

use baml_snapshot::BamlSnapshot;
use baml_tests::{bytecode::compile_source, vm::Value};
use bex_engine::{BexEngine, ResolvedValue};
use indexmap::IndexMap;
use tempfile::TempDir;

/// Test input for engine execution.
pub(crate) struct EngineProgram {
    /// Virtual filesystem: maps relative paths to file contents.
    /// Files are created in a temp directory before the test runs.
    /// Relative paths in `baml.fs.open()` are resolved against this directory.
    pub fs: IndexMap<&'static str, &'static str>,
    /// The BAML source code to compile and execute.
    pub source: &'static str,
    /// The function name to execute.
    pub function: &'static str,
    /// Expected result: Ok(value) for success, Err(message) for expected error.
    pub expected: Result<Value, &'static str>,
}

impl Default for EngineProgram {
    fn default() -> Self {
        Self {
            fs: IndexMap::new(),
            source: "",
            function: "main",
            expected: Ok(Value::Null),
        }
    }
}

/// Helper to create test inputs more ergonomically.
impl EngineProgram {
    pub(crate) fn new(source: &'static str) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }

    pub(crate) fn with_fs(mut self, fs: IndexMap<&'static str, &'static str>) -> Self {
        self.fs = fs;
        self
    }

    pub(crate) fn function(mut self, function: &'static str) -> Self {
        self.function = function;
        self
    }

    pub(crate) fn expect(mut self, expected: Value) -> Self {
        self.expected = Ok(expected);
        self
    }

    pub(crate) fn expect_error(mut self, message: &'static str) -> Self {
        self.expected = Err(message);
        self
    }
}

/// Compile BAML source code into a snapshot.
pub(crate) fn compile_for_engine(source: &str) -> BamlSnapshot {
    let program = compile_source(source);
    BamlSnapshot::new(program)
}

/// Set up the virtual filesystem for a test.
///
/// Creates a temp directory and writes all files from `fs` into it.
/// Returns the temp directory (kept alive for the test duration).
fn setup_virtual_fs(fs: &IndexMap<&'static str, &'static str>) -> anyhow::Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();

    // Write all files to the temp directory
    for (path, contents) in fs {
        let full_path = root.join(path);
        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&full_path)?;
        file.write_all(contents.as_bytes())?;
    }

    Ok(temp_dir)
}

/// Convert a `ResolvedValue` (from engine execution) to a test Value.
pub(crate) fn value_from_resolved(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::Null => Value::Null,
        ResolvedValue::Int(i) => Value::Int(*i),
        ResolvedValue::Float(f) => Value::Float(*f),
        ResolvedValue::Bool(b) => Value::Bool(*b),
        ResolvedValue::String(s) => Value::string(s),
        ResolvedValue::Array(arr) => Value::array(arr.iter().map(value_from_resolved).collect()),
        ResolvedValue::Map(map) => Value::map(
            map.iter()
                .map(|(k, v)| (k.clone(), value_from_resolved(v)))
                .collect(),
        ),
        ResolvedValue::ResourceId(id) => Value::string(&format!("<resource {id}>")),
    }
}

/// Assert that engine execution succeeds with the expected result.
pub(crate) async fn assert_engine_executes(input: EngineProgram) -> anyhow::Result<()> {
    // Set up virtual filesystem
    let temp_dir = setup_virtual_fs(&input.fs)?;
    let root_path = temp_dir.path().display().to_string();

    // Replace {ROOT} in source with actual temp directory path
    let source = input.source.replace("{ROOT}", &root_path);

    let snapshot = compile_for_engine(&source);
    let engine = BexEngine::new(snapshot, HashMap::new()).expect("Failed to create engine");

    let result = engine.call_function(input.function, &[]).await;

    match (result, input.expected) {
        (Ok(value), Ok(expected)) => {
            let actual = value_from_resolved(&value);
            assert_eq!(
                actual, expected,
                "Engine execution result mismatch for function '{}'",
                input.function
            );
        }
        (Err(e), Err(expected_msg)) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains(expected_msg),
                "Expected error containing '{expected_msg}', got: {error_msg}"
            );
        }
        (Ok(value), Err(expected_msg)) => {
            panic!("Expected error containing '{expected_msg}', but got success: {value:?}");
        }
        (Err(e), Ok(expected)) => {
            panic!("Expected success with {expected:?}, but got error: {e}");
        }
    }

    Ok(())
}

/// Assert that engine execution fails with an error containing the expected message.
pub(crate) async fn assert_engine_fails(input: EngineProgram) -> anyhow::Result<()> {
    assert_engine_executes(input).await
}
