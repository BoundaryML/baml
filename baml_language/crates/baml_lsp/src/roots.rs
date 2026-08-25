//! The owner's read view of the source-root table, shared with snapshots.
//!
//! Rebuilt (and `Arc`-swapped) whenever a root is added or removed; file
//! edits do not touch it. Snapshots hold an `Arc` so root membership and the
//! stdlib path mapping are table lookups with no filesystem access.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::{Name, SourceRoot, SourceRootKind};

/// The virtual prefix every embedded stdlib file path starts with.
pub const BUILTIN_PREFIX: &str = "<builtin>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootEntry {
    pub root: SourceRoot,
    pub path: PathBuf,
    pub package: Name,
    pub kind: SourceRootKind,
}

/// Roots in table order plus a longest-prefix index over their paths.
#[derive(Debug, Default)]
pub struct RootsView {
    entries: Vec<RootEntry>,
    /// Sorted by path for the longest-prefix walk.
    by_path: Vec<(PathBuf, usize)>,
    /// Where the stdlib stubs live on disk, if the host materialized them.
    stdlib_dir: Option<PathBuf>,
}

impl RootsView {
    pub fn new(entries: Vec<RootEntry>, stdlib_dir: Option<PathBuf>) -> Arc<Self> {
        let mut by_path: Vec<(PathBuf, usize)> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.path.clone(), index))
            .collect();
        by_path.sort();
        Arc::new(Self {
            entries,
            by_path,
            stdlib_dir,
        })
    }

    pub fn entries(&self) -> &[RootEntry] {
        &self.entries
    }

    pub fn iter_kind(&self, kind: SourceRootKind) -> impl Iterator<Item = &RootEntry> + '_ {
        self.entries.iter().filter(move |entry| entry.kind == kind)
    }

    pub fn workspace_roots(&self) -> impl Iterator<Item = &RootEntry> + '_ {
        self.iter_kind(SourceRootKind::Workspace)
    }

    /// The root whose path is the longest prefix of `path`, if any.
    ///
    /// `path` must already be in database form (canonical for real files,
    /// virtual `<builtin>/…` for stdlib) — see [`crate::paths`].
    pub fn root_for_path(&self, path: &Path) -> Option<&RootEntry> {
        // Every prefix of `path` sorts at or before `path`, and among the
        // prefixes of one path the longer sorts later — so walking backwards
        // from `path`'s insertion point meets the longest prefix first.
        let end = self
            .by_path
            .partition_point(|(root_path, _)| root_path.as_path() <= path);
        self.by_path[..end]
            .iter()
            .rev()
            .find(|(root_path, _)| path.starts_with(root_path))
            .map(|(_, index)| &self.entries[*index])
    }

    pub fn stdlib_dir(&self) -> Option<&Path> {
        self.stdlib_dir.as_deref()
    }

    /// Map a database path to the path a client can open.
    ///
    /// Stdlib files are stored under the virtual `<builtin>/<pkg>/…` prefix
    /// (a wire contract the emitted bytecode shares); when the host has a
    /// materialized copy of the stubs on disk, presentation swaps the prefix
    /// for that directory. Everything else is returned unchanged.
    pub fn to_presentation_path(&self, db_path: &Path) -> Option<PathBuf> {
        match db_path.strip_prefix(BUILTIN_PREFIX) {
            Ok(rest) => self.stdlib_dir.as_ref().map(|dir| dir.join(rest)),
            Err(_) => Some(db_path.to_path_buf()),
        }
    }

    /// Map a client path back to database form: a path under the
    /// materialized stdlib directory becomes its virtual `<builtin>/…`
    /// spelling. Everything else is returned unchanged.
    pub fn to_db_path(&self, presentation_path: &Path) -> PathBuf {
        match self
            .stdlib_dir
            .as_ref()
            .and_then(|dir| presentation_path.strip_prefix(dir).ok())
        {
            // Spelled with `/` by hand: the database stores virtual paths
            // byte-for-byte with forward slashes on every platform (the
            // `<builtin>/` wire contract), and `PathBuf` equality is
            // byte-wise — a `Path::join` here would use `\` on Windows and
            // miss every file-map lookup.
            Some(rest) => {
                let mut db_path = String::from(BUILTIN_PREFIX);
                for component in rest.components() {
                    db_path.push('/');
                    db_path.push_str(&component.as_os_str().to_string_lossy());
                }
                PathBuf::from(db_path)
            }
            None => presentation_path.to_path_buf(),
        }
    }
}
