//! Global BexEngine management.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use baml_compiler_emit::LoweringError;
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
use once_cell::sync::OnceCell;
use sys_native::SysOpsExt;
use sys_types::SysOps;
use tokio::runtime::Runtime;

use crate::error::BridgeError;

/// Global BexEngine instance. Uses RwLock to allow replacing the engine.
static ENGINE: RwLock<Option<Arc<BexEngine>>> = RwLock::new(None);

/// Global Tokio runtime for async execution.
static RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the global Tokio runtime.
pub fn get_runtime() -> &'static Arc<Runtime> {
    RUNTIME.get_or_init(|| Arc::new(Runtime::new().expect("Failed to create Tokio runtime")))
}

/// Get the global BexEngine, or error if not initialized.
pub fn get_engine() -> Result<Arc<BexEngine>, BridgeError> {
    ENGINE
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .clone()
        .ok_or(BridgeError::NotInitialized)
}

/// Initialize the global BexEngine from BAML source files.
///
/// If an engine is already initialized, it will be replaced with the new one.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
/// * `env_vars` - Environment variables
pub fn initialize_engine(
    root_path: &str,
    src_files: HashMap<String, String>,
    env_vars: HashMap<String, String>,
) -> Result<(), BridgeError> {
    // Create database
    let mut db = ProjectDatabase::new();

    // Set project root
    let root = Path::new(root_path);
    db.set_project_root(root);

    // Add all source files to the database
    // Note: We use just the filename (relative path) since the content is embedded.
    // Using the relative path produces more useful error messages than a potentially
    // incorrect absolute path constructed from root + filename.
    for (filename, content) in src_files {
        let file_path = PathBuf::from(&filename);
        db.add_or_update_file(&file_path, &content);
    }

    // Compile bytecode
    let bytecode = baml_compiler_emit::generate_project_bytecode(&db)
        .map_err(|e| render_lowering_error(&db, &e))?;

    // Create engine directly from bytecode (type metadata is on VM objects)
    let engine = BexEngine::new(bytecode, env_vars, SysOps::native())?;

    // Store in global (replacing any existing engine)
    let mut guard = ENGINE.write().map_err(|_| BridgeError::LockPoisoned)?;
    *guard = Some(Arc::new(engine));

    Ok(())
}

/// Render a LoweringError with source context for better debugging.
fn render_lowering_error(db: &ProjectDatabase, error: &LoweringError) -> BridgeError {
    // Get the span from the error
    let span = match error.span() {
        Some(s) => s,
        None => {
            return BridgeError::Compilation {
                message: error.to_string(),
            };
        }
    };

    // Try to get the source file content
    let file_id = span.file_id;
    let source_files = db.get_source_files();

    // Find the matching source file
    for source_file in source_files {
        if source_file.file_id(db) == file_id {
            let content = source_file.text(db);
            let file_path = source_file.path(db);

            let start = u32::from(span.range.start()) as usize;
            let end = u32::from(span.range.end()) as usize;

            // Extract a few lines of context around the error
            let (line_num, col, context) = extract_source_context(content, start, end);

            return BridgeError::Compilation {
                message: format!(
                    "{}\n\n  --> {}:{}:{}\n\n{}",
                    error,
                    file_path.display(),
                    line_num,
                    col,
                    context
                ),
            };
        }
    }

    // Fallback if we can't find the source
    BridgeError::Compilation {
        message: error.to_string(),
    }
}

/// Extract source context around a byte range, returning (line_number, column, formatted_context).
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
        formatted.push_str(&format!(
            "  {} {:>width$} | {}\n",
            prefix,
            num,
            line,
            width = line_num_width
        ));

        // Add underline for error position
        if *is_error_line {
            let underline_start = if *num == line_num { col - 1 } else { 0 };
            let underline_len = if start < end {
                (end - start).min(line.len().saturating_sub(underline_start))
            } else {
                1
            };
            formatted.push_str(&format!(
                "    {:>width$} | {}{}",
                "",
                " ".repeat(underline_start),
                "^".repeat(underline_len.max(1)),
                width = line_num_width
            ));
            formatted.push('\n');
        }
    }

    (line_num, col, formatted)
}
