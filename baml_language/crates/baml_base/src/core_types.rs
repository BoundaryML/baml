//! Core types used throughout the compiler.

use std::fmt;

use ariadne;
use smol_str::SmolStr;
use text_size::{TextRange, TextSize};

/// Unique identifier for a source file.
///
/// ## Bit layout
///
/// ```text
///   3 3 2 2 2 2 2 2 2 2 2 2 1 1 1 1 1 1 1 1 1 1
///   1 0 9 8 7 6 5 4 3 2 1 0 9 8 7 6 5 4 3 2 1 0 9 8 7 6 5 4 3 2 1 0
///  ├─┬─┬─┬─┼───────────────────────────────────────────────────────────┤
///  │ tag   │                    index (28 bits)                        │
///  └───────┴──────────────────────────────────────────────────────────-┘
/// ```
///
/// - **tag `0x0`** — real file (assigned by the host database)
/// - **tag `0x1`** — synthetic stream expansion of the origin file at `index`
/// - **tag `0xF`** — sentinel / fake (used by `Span::fake()` and `Span::default()`)
///
/// ## Why not `enum FileId { Real(u32), Stream(u32), Sentinel }`?
///
/// `FileId` is stored inside every `Span` (`file_id` + `TextRange` = 12 bytes).
/// An enum would widen `FileId` from 4 to 8 bytes (discriminant + alignment),
/// inflating `Span` from 12 to 16 bytes — a 33% increase across millions of spans.
///
/// ## Prior art
///
/// - **Roslyn** (C#): synthetic `SyntaxTree`s constructed with a virtual file path.
/// - **Clang**: bit 31 of `SourceLocation` distinguishes file vs macro-expansion locs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FileId(u32);

impl FileId {
    /// Create a `FileId` for a real source file.
    ///
    /// # Panics
    /// Panics if `id` uses the top 4 bits (reserved for tags).
    pub fn new(id: u32) -> Self {
        assert!(
            id & 0xF000_0000 == 0,
            "FileId::new({id}) exceeds 28-bit limit — top 4 bits are reserved"
        );
        FileId(id)
    }

    /// Sentinel value for fake/default spans. Not a real file.
    ///
    /// Bypasses the `new()` assert — the sentinel uses tag `0xF`.
    pub fn sentinel() -> FileId {
        FileId(u32::MAX)
    }

    /// Create a synthetic `FileId` for the stream expansion of `origin`.
    ///
    /// Sets tag `0x1` on the origin's index bits. Deterministic: same origin
    /// always produces the same synthetic id.
    pub fn stream_expansion(origin: FileId) -> FileId {
        debug_assert!(
            origin.0 & 0xF000_0000 == 0,
            "cannot expand a non-origin FileId"
        );
        FileId(origin.0 | 0x1000_0000)
    }

    /// Returns `true` if this `FileId` refers to a synthetic stream expansion file.
    pub fn is_stream_expansion(self) -> bool {
        self.0 & 0xF000_0000 == 0x1000_0000
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A span in source code, tracking both file and position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file_id: FileId,
    pub range: TextRange,
}

impl Default for Span {
    /// Creates a sentinel span that doesn't refer to any real file.
    ///
    /// Uses `u32::MAX` as the file ID to avoid conflicts with real files.
    fn default() -> Self {
        Self::fake()
    }
}

impl Span {
    pub fn new(file_id: FileId, range: TextRange) -> Self {
        Span { file_id, range }
    }

    pub fn at_offset(file_id: FileId, offset: TextSize) -> Self {
        Span {
            file_id,
            range: TextRange::empty(offset),
        }
    }

    /// Create a fake span for testing or when no real span is available.
    ///
    /// Uses a sentinel `FileId` (`u32::MAX`) that's unlikely to conflict with real files.
    pub fn fake() -> Self {
        Span {
            file_id: FileId::sentinel(),
            range: TextRange::empty(TextSize::new(0)),
        }
    }
}

impl ariadne::Span for Span {
    type SourceId = FileId;
    fn source(&self) -> &Self::SourceId {
        &self.file_id
    }
    fn start(&self) -> usize {
        let range = self.range.start().into()..self.range.end().into();
        range.start()
    }

    fn end(&self) -> usize {
        let range = self.range.start().into()..self.range.end().into();
        range.end()
    }
}

/// An interned string - used for identifiers, keywords, etc.
pub type Name = SmolStr;

/// The types of media we support
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Pdf,
    Generic, // could be any of the media types
}

impl fmt::Display for MediaKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaKind::Image => write!(f, "image"),
            MediaKind::Audio => write!(f, "audio"),
            MediaKind::Video => write!(f, "video"),
            MediaKind::Pdf => write!(f, "pdf"),
            MediaKind::Generic => write!(f, "image | audio | video | pdf"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Literal {
    Int(i64),
    Float(String),
    String(String),
    Bool(bool),
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(s) => write!(f, "\"{s}\""),
            Literal::Int(i) => write!(f, "{i}"),
            Literal::Float(s) => write!(f, "{s}"),
            Literal::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Module identifier (for multi-file support)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(u32);

impl ModuleId {
    pub fn new(id: u32) -> Self {
        ModuleId(id)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}
