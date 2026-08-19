//! The owner's mutation vocabulary.
//!
//! Every change to the source set is expressed as a batch of these and applied
//! by [`crate::state::GlobalState::apply`] on the owner thread. Notifications,
//! discovery results, and disk reloads all reduce to this enum; there are no
//! "refresh modes".

use std::path::PathBuf;

use baml_base::{Name, SourceRootKind};

/// A source root to add or replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSpec {
    pub path: PathBuf,
    pub package: Name,
    pub kind: SourceRootKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMutation {
    /// Add a root (or replace its file set) with the given on-disk files.
    /// Open-document overlays win over the listed text for their paths.
    UpsertRoot {
        spec: RootSpec,
        files: Vec<(PathBuf, String)>,
    },
    RemoveRoot {
        path: PathBuf,
    },
    /// An editor buffer's text (didOpen/didChange). Authoritative over disk
    /// while the document is open. `version` is `None` for non-editor
    /// writers (playground edits).
    SetOverlay {
        path: PathBuf,
        text: String,
        version: Option<i32>,
    },
    /// Text read from disk (watched-file change, post-close reload).
    /// Ignored while the path has an open overlay.
    SetDisk {
        path: PathBuf,
        text: String,
    },
    /// The file is gone from disk. Ignored while the path has an open
    /// overlay.
    RemoveFile {
        path: PathBuf,
    },
    /// didClose: the overlay is no longer authoritative. The text is left in
    /// place until a following `SetDisk`/`RemoveFile` reconciles it.
    CloseDocument {
        path: PathBuf,
    },
}
