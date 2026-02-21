//! Shared test utilities for `BexEngine` testing.
//!
//! This module provides common infrastructure for testing async execution
//! of BAML programs through `bex_engine`.

// Allow dead code since not all test files use all utilities
#![allow(dead_code)]

use std::{io::Write, sync::Arc};

use baml_tests::bytecode::compile_source;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm_types::Program;
use indexmap::IndexMap;
use sys_native::SysOpsExt;
use tempfile::TempDir;

/// Test input for engine execution.
pub(crate) struct EngineProgram {
    /// Virtual filesystem: maps relative paths to file contents.
    pub fs: IndexMap<&'static str, &'static str>,
    /// The BAML source code to compile and execute.
    pub source: &'static str,
    /// The function name to execute.
    pub entry: &'static str,
    /// Input arguments to pass to the function.
    pub inputs: Vec<BexExternalValue>,
    /// Expected result: Ok(value) for success, Err(message) for expected error.
    pub expected: Result<BexExternalValue, &'static str>,
}

impl Default for EngineProgram {
    fn default() -> Self {
        Self {
            fs: IndexMap::new(),
            source: "",
            entry: "main",
            inputs: Vec::new(),
            expected: Ok(BexExternalValue::Null),
        }
    }
}

/// Compile BAML source code into bytecode.
pub(crate) fn compile_for_engine(source: &str) -> Program {
    compile_source(source)
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
    let engine = BexEngine::new(snapshot, Arc::new(sys_types::SysOps::native()))
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            input.entry,
            input.inputs,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    match (result, input.expected) {
        (Ok(value), Ok(expected)) => {
            assert_eq!(
                value, expected,
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
        (Ok(value), Err(expected_msg)) => {
            panic!("Expected error containing '{expected_msg}', but got success: {value:?}");
        }
        (Err(e), Ok(expected)) => {
            panic!("Expected success with {expected:?}, but got error: {e}");
        }
    }

    Ok(())
}
