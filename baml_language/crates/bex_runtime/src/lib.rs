//! Reusable compile-and-run runtime for BAML programs.
//!
//! `BexRuntime` wraps the compile + engine pipeline into an opaque facade
//! that any consumer (CFFI, WASM, tests, CLI) can use without reimplementing
//! the compile-and-run flow.

mod error;

use std::{
    collections::HashMap,
    fmt::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_compiler_emit::LoweringError;
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
pub use bex_engine::EngineError;
pub use bex_external_types::{BexExternalAdt, BexExternalValue, MediaKind, Ty};
use bex_heap::BexValue;
pub use error::RuntimeError;
pub use sys_types::SysOps;

/// An opaque runtime that compiles BAML source files and executes functions.
#[derive(Clone)]
pub struct BexRuntime {
    engine: Arc<BexEngine>,
}

impl BexRuntime {
    /// Compile source files and create an engine.
    ///
    /// # Arguments
    /// * `root_path` - Root path for BAML files
    /// * `src_files` - Map of filename to content
    /// * `env_vars` - Environment variables
    /// * `sys_ops` - System operations provider
    pub fn new(
        root_path: &str,
        src_files: &HashMap<String, String>,
        env_vars: HashMap<String, String>,
        sys_ops: SysOps,
    ) -> Result<Self, RuntimeError> {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new(root_path));

        for (filename, content) in src_files {
            db.add_or_update_file(&PathBuf::from(filename), content);
        }

        let bytecode = baml_compiler_emit::generate_project_bytecode(&db)
            .map_err(|e| render_lowering_error(&db, &e))?;

        let engine = BexEngine::new(bytecode, env_vars, sys_ops)?;

        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Execute a function by name.
    ///
    /// Calls `BexEngine::call_function`, then converts the result to a fully
    /// owned `BexExternalValue` with no heap references.
    pub async fn call_function(
        &self,
        function_name: &str,
        args: Vec<BexExternalValue>,
    ) -> Result<BexExternalValue, RuntimeError> {
        let result = self.engine.call_function(function_name, args).await?;

        // Ensure the returned value is fully owned (no Handle variants).
        self.engine
            .heap()
            .with_gc_protection(|protected| {
                BexValue::ExternalValue(&result).as_owned_but_very_slow(&protected)
            })
            .map_err(RuntimeError::from)
    }

    /// Get parameter names and types for a function.
    pub fn function_params(&self, name: &str) -> Option<Vec<(&str, &Ty)>> {
        self.engine.function_params(name)
    }
}

// ---------------------------------------------------------------------------
// Error rendering helpers (adapted from bridge_cffi/src/engine.rs)
// ---------------------------------------------------------------------------

/// Render a `LoweringError` with source context for better debugging.
fn render_lowering_error(db: &ProjectDatabase, error: &LoweringError) -> RuntimeError {
    let Some(span) = error.span() else {
        return RuntimeError::Compilation {
            message: error.to_string(),
        };
    };

    let file_id = span.file_id;
    let source_files = db.get_source_files();

    for source_file in source_files {
        if source_file.file_id(db) == file_id {
            let content = source_file.text(db);
            let file_path = source_file.path(db);

            let start = u32::from(span.range.start()) as usize;
            let end = u32::from(span.range.end()) as usize;

            let (line_num, col, context) = extract_source_context(content, start, end);

            return RuntimeError::Compilation {
                message: format!(
                    "{error}\n\n  --> {}:{line_num}:{col}\n\n{context}",
                    file_path.display(),
                ),
            };
        }
    }

    RuntimeError::Compilation {
        message: error.to_string(),
    }
}

/// Extract source context around a byte range.
///
/// Returns `(line_number, column, formatted_context)`.
fn extract_source_context(content: &str, start: usize, end: usize) -> (usize, usize, String) {
    let bytes = content.as_bytes();

    // Find line number and column for start position
    let mut line_num = 1;
    let mut line_start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if i >= start {
            break;
        }
        if byte == b'\n' {
            line_num += 1;
            line_start = i + 1;
        }
    }
    let col = start.saturating_sub(line_start) + 1;

    // Extract the line(s) containing the error
    let mut lines_to_show = Vec::new();
    let mut current_line_start = line_start;
    let mut current_line_num = line_num;

    // Find up to 3 lines of context
    for (i, &byte) in bytes.iter().enumerate().skip(line_start) {
        if byte == b'\n' || i == bytes.len() - 1 {
            let line_end = if byte == b'\n' { i } else { i + 1 };
            let line_content = &content[current_line_start..line_end];

            // Check if this line overlaps with the error span
            let line_overlaps = current_line_start < end && line_end > start;

            if line_overlaps || lines_to_show.len() < 3 {
                lines_to_show.push((current_line_num, line_content.to_string(), line_overlaps));
            }

            if lines_to_show.len() >= 5 || current_line_start > end {
                break;
            }

            current_line_start = i + 1;
            current_line_num += 1;
        }
    }

    // Format the context with line numbers and highlighting
    let mut formatted = String::new();
    let line_num_width = lines_to_show
        .iter()
        .map(|(n, _, _)| n.to_string().len())
        .max()
        .unwrap_or(1);

    for (num, line, is_error_line) in &lines_to_show {
        let prefix = if *is_error_line { ">" } else { " " };
        let _ = writeln!(formatted, "  {prefix} {num:>line_num_width$} | {line}",);

        // Add underline for error position
        if *is_error_line {
            let underline_start = if *num == line_num { col - 1 } else { 0 };
            let underline_len = if start < end {
                (end - start).min(line.len().saturating_sub(underline_start))
            } else {
                1
            };
            let _ = writeln!(
                formatted,
                "    {:>line_num_width$} | {}{}",
                "",
                " ".repeat(underline_start),
                "^".repeat(underline_len.max(1)),
            );
        }
    }

    (line_num, col, formatted)
}
