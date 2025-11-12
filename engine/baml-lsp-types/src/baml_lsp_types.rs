// some of the types used in the LSP and WASM.
// This is almost a direct copy of some of the types in baml-schema-wasm crate.
// but without the wasm-bindgen dependency.
// Wasm-bindgen causes strange issues with ruby builds so we'd rather not add that dependency if we can.

use baml_types::FunctionFlavor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BamlSpan {
    pub file_path: String,
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
}

impl Default for BamlSpan {
    fn default() -> Self {
        BamlSpan {
            file_path: "".to_string(),
            start: 0,
            end: 0,
            start_line: 0,
            end_line: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BamlParentFunction {
    pub start: usize,
    pub end: usize,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BamlParam {
    pub name: String,
    pub value: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BamlFunctionTestCasePair {
    pub name: String,
    pub inputs: Vec<BamlParam>,
    pub error: Option<String>,
    /// The span of `test TestFoo`
    pub span: BamlSpan,
    /// The span of `FnBar` in `functions [FnFoo, FnBar, FnBaz]`
    /// This is wired through to allow re-sorting these as the caller desires.
    pub function_name_span: Option<BamlSpan>,
    pub function: BamlParentFunction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BamlFunction {
    pub name: String,
    pub span: BamlSpan,
    pub function_type: FunctionFlavor,
    pub test_cases: Vec<BamlFunctionTestCasePair>,
    pub test_snippet: String,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BamlGeneratorConfig {
    pub output_type: String,
    pub version: String,
    pub span: BamlSpan,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlError {
    pub r#type: String,
    pub file_path: String,
    pub start_ch: usize,
    pub end_ch: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BamlDiagnosticError {
    pub errors: Vec<BamlError>,
    pub all_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolLocation {
    pub uri: String,
    pub start_line: usize,
    pub start_character: usize,
    pub end_line: usize,
    pub end_character: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    LLMFailure,
    ParseFailure,
    FinishReasonFailed,
    ConstraintsFailed,
    AssertFailed,
    UnableToRun,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SerializableOrchestratorNode {
    pub provider: String,
}

impl From<&baml_runtime::internal_baml_diagnostics::Span> for BamlSpan {
    fn from(span: &baml_runtime::internal_baml_diagnostics::Span) -> Self {
        let (start, end) = span.line_and_column();
        BamlSpan {
            file_path: span.file.path().to_string(),
            start: span.start,
            end: span.end,
            start_line: start.0,
            end_line: end.0,
        }
    }
}
