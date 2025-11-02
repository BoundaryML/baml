//! Core types used throughout the compiler.

use std::fmt;

use smol_str::SmolStr;
use text_size::{TextRange, TextSize};

/// Unique identifier for a source file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        write!(f, "FileId({})", self.0)
    }
}

/// A span in source code, tracking both file and position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file_id: FileId,
    pub range: TextRange,
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
}

/// An interned string - used for identifiers, keywords, etc.
pub type Name = SmolStr;

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

/// Base trait for all compiler diagnostics
pub trait Diagnostic: std::fmt::Debug {
    fn message(&self) -> String;
    fn span(&self) -> Option<Span>;
    fn severity(&self) -> Severity;
}
