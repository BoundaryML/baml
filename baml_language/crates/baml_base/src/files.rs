//! File management with Salsa 2022 API.
//!
//! Defines the core structures for accessing file contents and paths.

use std::path::PathBuf;

use crate::FileId;

/// Input structure representing a source file in the compilation.
///
/// This is a salsa input, which means it's the primary way to provide
/// source text to the compiler. The struct itself just stores an ID,
/// with the actual data stored in the salsa database.
#[salsa::input]
pub struct SourceFile {
    /// Source text for the file
    #[returns(ref)]
    pub text: String,

    /// File path (for diagnostics and error reporting)
    pub path: PathBuf,

    /// The FileId associated with this source file.
    ///
    /// Used to create lightweight Span values that can be embedded in tokens.
    /// This allows spans to identify their source file without carrying
    /// the full SourceFile reference (which is a Salsa-tracked entity).
    pub file_id: FileId,
}
