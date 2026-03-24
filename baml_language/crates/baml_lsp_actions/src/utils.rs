//! Shared utilities for LSP actions.
//!
//! NOTE: Stubbed — pending compiler2 LSP action reimplementation.

use baml_db::SourceFile;
use text_size::TextSize;

/// Find the function whose syntax range contains the given offset.
///
/// NOTE: Stubbed — returns None. Will be reimplemented with compiler2 HIR.
#[allow(unused_variables)]
pub fn find_function_at_position(
    _db: &dyn baml_db::baml_compiler2_hir::Db,
    _file: SourceFile,
    _position: TextSize,
) -> Option<()> {
    None
}
