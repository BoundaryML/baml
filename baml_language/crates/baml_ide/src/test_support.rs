//! Test-only builders over the root-aware database API.

use std::path::Path;

use baml_base::{Name, SourceFile, SourceRoot, SourceRootKind};
use baml_db::{ProjectDatabase, SourceRootSpec};

/// Root-aware fixture builders for a fresh [`ProjectDatabase`].
pub(crate) trait TestDbExt {
    /// Load the stdlib and add the single `Workspace` root at `root`.
    fn workspace(&mut self, root: &Path) -> SourceRoot;
    /// Upsert `path` (which must lie under an existing root) with `text`.
    fn file(&mut self, path: &Path, text: &str) -> SourceFile;
}

impl TestDbExt for ProjectDatabase {
    fn workspace(&mut self, root: &Path) -> SourceRoot {
        self.ensure_stdlib_sources();
        self.add_source_root(SourceRootSpec {
            path: root.to_path_buf(),
            package: Name::new(baml_type::RESERVED_USER_PACKAGE),
            kind: SourceRootKind::Workspace,
        })
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"))
    }

    fn file(&mut self, path: &Path, text: &str) -> SourceFile {
        let root = self
            .source_root_for_path(path)
            .unwrap_or_else(|| unreachable!("test files live under the workspace root"));
        self.add_or_update_file_in(root, path, text)
    }
}
