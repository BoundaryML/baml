//! Test-only builders over the root-aware database API, plus the
//! cursor-marker fixture machinery shared by every feature's tests.

use std::path::{Path, PathBuf};

use baml_base::{Name, SourceFile, SourceRoot, SourceRootKind};
use baml_db::{ProjectDatabase, SourceRootSpec};
use text_size::TextSize;

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

// ── Cursor fixture machinery ─────────────────────────────────────────────────

/// The cursor marker used in test sources: the cursor sits immediately to
/// the LEFT of the marker.
pub(crate) const CURSOR_MARKER: &str = "<[CURSOR]";

/// A fixture project whose sources carried exactly one [`CURSOR_MARKER`].
pub(crate) struct CursorTest {
    pub(crate) db: ProjectDatabase,
    pub(crate) cursor: Cursor,
}

/// Cursor position and context.
pub(crate) struct Cursor {
    pub(crate) file: SourceFile,
    pub(crate) offset: TextSize,
}

impl CursorTest {
    /// Create a new cursor test from source with a [`CURSOR_MARKER`].
    pub(crate) fn new(source: &str) -> Self {
        Self::with_filename("test.baml", source)
    }

    /// Create a new cursor test with a specific filename.
    pub(crate) fn with_filename(filename: &str, source: &str) -> Self {
        let mut builder = CursorTestBuilder::default();
        builder.source(filename, source);
        builder.build()
    }

    /// Create a builder for multi-file tests.
    pub(crate) fn builder() -> CursorTestBuilder {
        CursorTestBuilder::default()
    }

    /// Format a file + byte-range as `"filename:line:col"` for assertions.
    pub(crate) fn format_file_range(
        &self,
        file: SourceFile,
        range: text_size::TextRange,
    ) -> String {
        let filename = file
            .path(&self.db)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let text = file.text(&self.db);
        let offset: usize = range.start().into();
        let (line, col) = offset_to_line_col(text, offset);

        format!("{filename}:{line}:{col}")
    }

    /// [`Self::format_file_range`] plus ` -> TEXT` with the range's source text.
    pub(crate) fn format_file_range_with_text(
        &self,
        file: SourceFile,
        range: text_size::TextRange,
    ) -> String {
        let base = self.format_file_range(file, range);
        let text = file.text(&self.db);
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        format!("{base} -> {}", &text[start..end])
    }
}

/// Builder for cursor tests supporting multiple files.
#[derive(Default)]
pub(crate) struct CursorTestBuilder {
    sources: Vec<FixtureSource>,
}

struct FixtureSource {
    filename: String,
    content: String,
    cursor_offset: Option<TextSize>,
}

impl CursorTestBuilder {
    /// Add a source file to the test.
    pub(crate) fn source(&mut self, filename: &str, content: &str) -> &mut Self {
        let (clean_content, cursor_offset) = extract_cursor_marker(content);

        self.sources.push(FixtureSource {
            filename: filename.to_string(),
            content: clean_content,
            cursor_offset,
        });

        self
    }

    /// Build the cursor test.
    ///
    /// Panics unless exactly one source carried a cursor marker.
    pub(crate) fn build(self) -> CursorTest {
        let cursor_files: Vec<_> = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| s.cursor_offset.is_some())
            .collect();

        assert!(
            cursor_files.len() == 1,
            "Expected exactly one {CURSOR_MARKER} marker, found {} across {} files",
            cursor_files.len(),
            self.sources.len()
        );

        let (cursor_file_idx, _) = cursor_files[0];

        let mut db = ProjectDatabase::default();
        db.workspace(Path::new("/test"));

        let mut source_file_handles: Vec<SourceFile> = Vec::new();
        for source in &self.sources {
            let path = PathBuf::from("/test").join(&source.filename);
            source_file_handles.push(db.file(&path, &source.content));
        }

        let cursor_offset = self.sources[cursor_file_idx]
            .cursor_offset
            .unwrap_or_else(|| unreachable!("cursor_files filtered on Some"));
        let cursor_file = source_file_handles[cursor_file_idx];

        CursorTest {
            db,
            cursor: Cursor {
                file: cursor_file,
                offset: cursor_offset,
            },
        }
    }
}

/// Extract the cursor marker from source, returning cleaned source and offset.
fn extract_cursor_marker(source: &str) -> (String, Option<TextSize>) {
    if let Some(marker_pos) = source.find(CURSOR_MARKER) {
        let cursor_offset = TextSize::try_from(marker_pos)
            .unwrap_or_else(|_| unreachable!("fixture sources are far below 4 GiB"));

        let mut clean = String::with_capacity(source.len() - CURSOR_MARKER.len());
        clean.push_str(&source[..marker_pos]);
        clean.push_str(&source[marker_pos + CURSOR_MARKER.len()..]);

        (clean, Some(cursor_offset))
    } else {
        (source.to_string(), None)
    }
}

/// Convert a byte offset to a 1-indexed (line, column) pair.
pub(crate) fn offset_to_line_col(content: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(content.len());
    let before = &content[..clamped];
    let line = before.matches('\n').count() + 1;
    let last_newline = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let column = clamped - last_newline + 1;
    (line, column)
}

// ── Project fixture (no cursor) ──────────────────────────────────────────────

/// A test project with multiple BAML files for project-level IDE features.
pub(crate) struct ProjectTest {
    pub(crate) db: ProjectDatabase,
    pub(crate) files: Vec<SourceFile>,
}

impl ProjectTest {
    /// Create a builder for multi-file project tests.
    pub(crate) fn builder() -> ProjectTestBuilder {
        ProjectTestBuilder::default()
    }
}

/// Builder for project tests supporting multiple files (no cursor).
#[derive(Default)]
pub(crate) struct ProjectTestBuilder {
    sources: Vec<(String, String)>,
}

impl ProjectTestBuilder {
    /// Add a source file to the test project.
    pub(crate) fn source(&mut self, filename: &str, content: &str) -> &mut Self {
        self.sources
            .push((filename.to_string(), content.to_string()));
        self
    }

    /// Build the project test.
    pub(crate) fn build(self) -> ProjectTest {
        let mut db = ProjectDatabase::default();
        db.workspace(Path::new("/test"));

        let mut files: Vec<SourceFile> = Vec::new();
        for (filename, content) in &self.sources {
            let path = PathBuf::from("/test").join(filename);
            files.push(db.file(&path, content));
        }

        ProjectTest { db, files }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cursor_marker_removes_marker_and_records_offset() {
        let source = "class Foo<[CURSOR] {}";
        let (clean, offset) = extract_cursor_marker(source);

        assert_eq!(clean, "class Foo {}");
        assert_eq!(offset, Some(TextSize::from(9)));
    }

    #[test]
    fn extract_cursor_marker_without_marker_is_identity() {
        let source = "class Foo {}";
        let (clean, offset) = extract_cursor_marker(source);

        assert_eq!(clean, "class Foo {}");
        assert_eq!(offset, None);
    }
}
