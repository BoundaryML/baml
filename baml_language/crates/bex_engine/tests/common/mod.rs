//! Shared test utilities for `BexEngine` testing.
//!
//! This module provides common infrastructure for testing async execution
//! of BAML programs through `bex_engine`.
//!
//! # Do not add plain behavior tests here
//!
//! An [`EngineProgram`] with an empty `fs`, empty `inputs`, and
//! `expected: Ok(value)` is just a BAML `test` block written in Rust — and it
//! pays a full compile per test to run. Those belong in
//! `crates/baml_tests/baml_src/` instead; see
//! `baml_language/TEST_INSTRUCTIONS.md` ("Where does a new test go?").
//!
//! This harness earns its keep only when the test needs something BAML cannot
//! express: host argument injection via `inputs`, a virtual filesystem via
//! `fs`, cancellation tokens, GC/heap observation, wall-clock timing, or a
//! host-level `EngineError` (as opposed to a BAML-catchable throw).

// Allow dead code since not all test files use all utilities
#![allow(dead_code)]

use std::{io::Write, sync::Arc};

use baml_builtins2::{PromptAst as BuiltinPromptAst, PromptAstSimple};
use baml_db::testing::compile_source;
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
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            input.entry,
            input.inputs,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
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
        BexExternalValue::Instance {
            class_name, fields, ..
        } if class_name == "ai.Prompt" => {
            let data = fields
                .get("_data")
                .expect("PromptAst instance should contain _data");
            prompt_ast_to_string(data)
        }
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
                let BuiltinPromptAst::Message { content, .. } = other else {
                    unreachable!()
                };
                match content.as_ref() {
                    PromptAstSimple::String(s) => s.clone(),
                    _ => panic!("Expected string content in PromptAst message"),
                }
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
/// Returns the full rendered prompt string (input arg + output format), as
/// produced by the compiler-generated `<Fn>$render_prompt` companion.
pub(crate) async fn render_output_format(baml_types: &str, return_type: &str) -> String {
    let source = format!(
        r##"
{baml_types}

function TestFunc(input: string) -> {return_type} {{
    client: "openai/gpt-4o"
    prompt: `
        ${{input}}
        ${{ctx.output_format}}
    `
}}

function get_prompt() -> string {{
    TestFunc$render_prompt("test").text()
}}
"##
    );
    render_stable(&source).await
}

/// Like `render_output_format` but with custom kwargs on the output format.
///
/// `kwargs` is inserted into `ctx.output_format_with(...)` on the
/// standalone `prompt` tag path (the ai-world LLM-function prompt
/// only binds plain `ctx.output_format`), e.g. `render_null_as = "omit"`.
pub(crate) async fn render_output_format_with_opts(
    baml_types: &str,
    return_type: &str,
    kwargs: &str,
) -> String {
    let source = format!(
        r##"
{baml_types}

function get_prompt() -> string {{
    let cc = ai.ContextClient {{ name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }};
    let rt = reflect.Type.of<{return_type}>();
    let render_ctx = ai.Context {{
        client: cc,
        tags: {{}},
        output_format: ai.internal.render_output_format(rt),
        _output_format: ai.internal.build_output_format(rt),
    }};
    let render = ai.prompt`test
${{render_ctx.output_format_with({kwargs})}}`;
    render(render_ctx).text()
}}
"##
    );
    render_stable(&source).await
}

/// Run `get_prompt()` from `source` 3 times end-to-end (compile -> engine ->
/// render) and assert the rendered output is stable across independent engine
/// instances (i.e. deterministic — a `HashMap` where an `IndexMap` belongs would
/// flake here).
async fn render_stable(source: &str) -> String {
    let mut first_result: Option<String> = None;

    for i in 0..3 {
        let snapshot = compile_for_engine(source);
        let engine = Arc::new(
            BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
                .expect("Failed to create engine"),
        );

        let result = engine
            .call_function(
                "get_prompt",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .expect("render_prompt failed");

        let BexExternalValue::String(rendered) = result else {
            panic!("expected get_prompt() to return a string, got {result:?}");
        };
        let rendered = rendered.to_string();

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
