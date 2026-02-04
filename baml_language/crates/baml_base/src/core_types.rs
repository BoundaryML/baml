//! Core types used throughout the compiler.

use std::fmt;

use ariadne;
use smol_str::SmolStr;
use text_size::{TextRange, TextSize};

/// Unique identifier for a source file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FileId(u32);

impl FileId {
    pub fn new(id: u32) -> Self {
        FileId(id)
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
            file_id: FileId::new(u32::MAX),
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
