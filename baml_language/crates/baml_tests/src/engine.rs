//! Shared test utilities for BexEngine testing.
//!
//! This module provides common infrastructure for testing async execution
//! of BAML programs through `bex_engine`.
//!
//! # Contents
//!
//! - [`compile_for_engine`]: Compiles BAML source to engine-ready bytecode.
//! - [`assert_engine_executes`]: Test assertion helper for async execution.
//! - [`EngineProgram`]: Test input type.
//!
//! # Usage
//!
//! ```ignore
//! use baml_tests::engine::{EngineProgram, assert_engine_executes};
//! use baml_tests::vm::Value;
//! use indexmap::indexmap;
//!
//! #[tokio::test]
//! async fn test_fs_read() {
//!     assert_engine_executes(EngineProgram {
//!         fs: indexmap! {
//!             "hello.txt" => "Hello from BAML!",
//!         },
//!         source: r#"
//!             function main() -> string {
//!                 let file = baml.fs.open("hello.txt");
//!                 file.read()
//!             }
//!         "#,
//!         function: "main",
//!         expected: Ok(Value::string("Hello from BAML!")),
//!     }).await.unwrap();
//! }
//! ```
//!
//! Files in `fs` are written to a temp directory, and relative paths in
//! `baml.fs.open()` are resolved against that directory.

use std::{collections::HashMap, io::Write};

use baml_snapshot::BamlSnapshot;
use bex_engine::BexEngine;
use bex_vm::convert_program;
use indexmap::IndexMap;
use tempfile::TempDir;

use crate::{bytecode::compile_source, vm::Value};

/// Test input for engine execution.
pub struct EngineProgram {
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
    pub fn new(source: &'static str) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }

    pub fn with_fs(mut self, fs: IndexMap<&'static str, &'static str>) -> Self {
        self.fs = fs;
        self
    }

    pub fn function(mut self, function: &'static str) -> Self {
        self.function = function;
        self
    }

    pub fn expect(mut self, expected: Value) -> Self {
        self.expected = Ok(expected);
        self
    }

    pub fn expect_error(mut self, message: &'static str) -> Self {
        self.expected = Err(message);
        self
    }
}

/// Compile BAML source code into engine-ready bytecode.
pub fn compile_for_engine(source: &str) -> BamlSnapshot {
    let program = compile_source(source);
    let bytecode = convert_program(program).expect("convert_program should succeed");
    BamlSnapshot::new(bytecode)
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

/// Assert that engine execution succeeds with the expected result.
pub async fn assert_engine_executes(input: EngineProgram) -> anyhow::Result<()> {
    // Set up virtual filesystem
    let temp_dir = setup_virtual_fs(&input.fs)?;
    let root_path = temp_dir.path().display().to_string();

    // Replace {ROOT} in source with actual temp directory path
    let source = input.source.replace("{ROOT}", &root_path);

    let snapshot = compile_for_engine(&source);
    let engine = BexEngine::new(snapshot, HashMap::new());

    let result = engine.call_function(input.function, &[]).await;

    match (result, input.expected) {
        (Ok(value), Ok(expected)) => {
            let actual = Value::from_resolved(&value);
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
                "Expected error containing '{}', got: {}",
                expected_msg,
                error_msg
            );
        }
        (Ok(value), Err(expected_msg)) => {
            panic!(
                "Expected error containing '{}', but got success: {:?}",
                expected_msg, value
            );
        }
        (Err(e), Ok(expected)) => {
            panic!("Expected success with {:?}, but got error: {}", expected, e);
        }
    }

    Ok(())
}

/// Assert that engine execution fails with an error containing the expected message.
pub async fn assert_engine_fails(input: EngineProgram) -> anyhow::Result<()> {
    assert_engine_executes(input).await
}
