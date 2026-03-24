//! Shared test utilities for `BexEngine` testing.
//!
//! This module provides common infrastructure for testing async execution
//! of BAML programs through `bex_engine`.

// Allow dead code since not all test files use all utilities
#![allow(dead_code)]

use std::{io::Write, sync::Arc};

use baml_builtins::{PromptAst as BuiltinPromptAst, PromptAstSimple};
use baml_tests::engine::compile_source;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_external_types::BexExternalAdt;
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
    let engine = BexEngine::new(snapshot, Arc::new(sys_types::SysOps::native()), None)
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

/// Extract the full rendered text from a `PromptAst` value.
pub(crate) fn prompt_ast_to_string(value: &BexExternalValue) -> String {
    match value {
        BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) => match ast.as_ref() {
            BuiltinPromptAst::Simple(s) => match s.as_ref() {
                PromptAstSimple::String(s) => s.clone(),
                _ => panic!("Expected string content in PromptAst"),
            },
            BuiltinPromptAst::Vec(messages) => {
                let mut parts = Vec::new();
                for msg in messages {
                    match msg.as_ref() {
                        BuiltinPromptAst::Message { content, .. } => match content.as_ref() {
                            PromptAstSimple::String(s) => parts.push(s.clone()),
                            _ => parts.push("<non-string>".to_string()),
                        },
                        _ => parts.push("<non-message>".to_string()),
                    }
                }
                parts.join("\n")
            }
            other @ BuiltinPromptAst::Message { .. } => {
                panic!("Unexpected PromptAst variant: {other:?}")
            }
        },
        other => panic!("Expected Adt(PromptAst), got {other:?}"),
    }
}

/// Compile BAML type/function declarations and render the output format.
///
/// `baml_types` should contain class/enum/type definitions.
/// `return_type` is the function's return type (e.g. "Tree", "`RecAliasOne`", "int").
///
/// Returns the full rendered prompt string (input arg + output format).
pub(crate) async fn render_output_format(baml_types: &str, return_type: &str) -> String {
    render_output_format_with_opts(baml_types, return_type, "").await
}

/// Like `render_output_format` but with custom kwargs on `ctx.output_format(...)`.
///
/// `kwargs` is inserted directly, e.g. `"prefix=null"` or `"map_style='angle'"`.
/// Pass `""` for default behavior.
///
/// Runs the full pipeline 3 times (compile -> engine -> render -> extract) to
/// verify that output format rendering is deterministic/stable across runs.
pub(crate) async fn render_output_format_with_opts(
    baml_types: &str,
    return_type: &str,
    kwargs: &str,
) -> String {
    let output_format_call = if kwargs.is_empty() {
        "ctx.output_format".to_string()
    } else {
        format!("ctx.output_format({kwargs})")
    };

    let source = format!(
        r##"
client TestClient {{
    provider openai
    options {{
        model "gpt-4"
    }}
}}

{baml_types}

function TestFunc(input: string) -> {return_type} {{
    client TestClient
    prompt #"
        {{{{ input }}}}
        {{{{ {output_format_call} }}}}
    "#
}}

function get_prompt() -> baml.llm.PromptAst {{
    let args = {{ "input": "test" }};
    baml.llm.render_prompt("TestFunc", args)
}}
"##
    );

    let mut first_result: Option<String> = None;

    // Run 3 times end-to-end to check that the rendered output is stable
    // (i.e. deterministic across independent engine instances).
    for i in 0..3 {
        let snapshot = compile_for_engine(&source);
        let engine = BexEngine::new(snapshot, Arc::new(sys_types::SysOps::native()), None)
            .expect("Failed to create engine");

        let result = engine
            .call_function(
                "get_prompt",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            )
            .await
            .expect("render_prompt failed");

        let rendered = prompt_ast_to_string(&result);

        match &first_result {
            None => first_result = Some(rendered),
            Some(expected) => {
                assert_eq!(
                    &rendered, expected,
                    "Output format changed between runs (run 0 vs run {i}). Is there a HashMap where there should be an IndexMap?"
                );
            }
        }
    }

    first_result.unwrap()
}
