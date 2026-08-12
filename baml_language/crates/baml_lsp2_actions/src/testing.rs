//! Test infrastructure for IDE features (compiler2 pipeline).
//!
//! Provides cursor-based testing where `<[CURSOR]` markers indicate
//! the cursor position (immediately to the LEFT of the marker).

use std::{
    fmt::Write as _,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use baml_base::{FileId, SourceFile};
use baml_workspace::{Compiler2ExtraFiles, Project};
use salsa::Setter;
use text_size::TextSize;

use crate::{
    definition::{Location, definition_at},
    describe::SymbolDescription,
    usages::usages_at,
};

// ── Test database ────────────────────────────────────────────────────────────

/// Minimal Salsa database for testing `baml_lsp2_actions` queries.
///
/// Stores `Project` and `Compiler2ExtraFiles` as regular fields so the `Db`
/// trait impls can return them.
#[salsa::db]
pub(crate) struct TestDb {
    storage: salsa::Storage<TestDb>,
    next_file_id: AtomicU32,
    project: Option<Project>,
    extra: Option<Compiler2ExtraFiles>,
}

impl Default for TestDb {
    fn default() -> Self {
        Self {
            storage: salsa::Storage::default(),
            next_file_id: AtomicU32::new(0),
            project: None,
            extra: None,
        }
    }
}

impl Clone for TestDb {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            next_file_id: AtomicU32::new(self.next_file_id.load(Ordering::SeqCst)),
            project: self.project,
            extra: self.extra,
        }
    }
}

impl TestDb {
    fn add_file(&mut self, path: impl Into<PathBuf>, content: &str) -> SourceFile {
        let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
        SourceFile::new(self, content.to_string(), path.into(), file_id)
    }

    /// Initialize with builtins and a project root.
    fn init(&mut self) {
        // Load builtin files.
        let mut builtin_files = Vec::new();
        for builtin in baml_builtins2::ALL {
            let file = self.add_file(PathBuf::from(builtin.virtual_path()), builtin.contents);
            builtin_files.push(file);
        }

        let project = Project::new(self, PathBuf::from("/test"), Vec::new());
        self.project = Some(project);

        let extra = Compiler2ExtraFiles::new(self, builtin_files);
        self.extra = Some(extra);
    }
}

#[salsa::db]
impl salsa::Database for TestDb {}

#[salsa::db]
impl baml_workspace::Db for TestDb {
    fn project(&self) -> Project {
        self.project
            .expect("TestDb not initialized — call init() first")
    }
}

#[salsa::db]
impl baml_compiler2_hir::Db for TestDb {
    fn compiler2_extra_files(&self) -> Option<Compiler2ExtraFiles> {
        self.extra
    }
}

#[salsa::db]
impl baml_compiler2_ppir::Db for TestDb {}

#[salsa::db]
impl crate::Db for TestDb {}

// ── Cursor test infrastructure ───────────────────────────────────────────────

/// The cursor marker used in test sources.
pub(crate) const CURSOR_MARKER: &str = "<[CURSOR]";

/// A test with cursor position information.
pub(crate) struct CursorTest {
    pub(crate) db: TestDb,
    pub(crate) cursor: Cursor,
}

/// Cursor position and context.
pub(crate) struct Cursor {
    pub(crate) file: SourceFile,
    pub(crate) offset: TextSize,
}

impl CursorTest {
    /// Create a new cursor test from source with a `<[CURSOR]` marker.
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

    /// Get goto-definition result at the cursor position.
    pub(crate) fn goto_definition(&self) -> Option<Location> {
        definition_at(&self.db, self.cursor.file, self.cursor.offset)
    }

    /// Find all usages/references at the cursor position.
    pub(crate) fn find_all_usages(&self) -> Vec<Location> {
        usages_at(&self.db, self.cursor.file, self.cursor.offset)
    }

    /// Format a `Location` as `"filename:line:col"` for assertion messages.
    pub(crate) fn format_location(&self, loc: &Location) -> String {
        let filename = loc
            .file
            .path(&self.db)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let text = loc.file.text(&self.db);
        let offset: usize = loc.range.start().into();
        let (line, col) = offset_to_line_col(text, offset);

        format!("{filename}:{line}:{col}")
    }

    /// Format a `Location` as `"filename:line:col -> NAME"`.
    pub(crate) fn format_location_with_name(&self, loc: &Location) -> String {
        let base = self.format_location(loc);
        let text = loc.file.text(&self.db);
        let start: usize = loc.range.start().into();
        let end: usize = loc.range.end().into();
        let name = &text[start..end];
        format!("{base} -> {name}")
    }
}

/// Builder for cursor tests supporting multiple files.
#[derive(Default)]
pub(crate) struct CursorTestBuilder {
    sources: Vec<Source>,
}

struct Source {
    filename: String,
    content: String,
    cursor_offset: Option<TextSize>,
}

impl CursorTestBuilder {
    /// Add a source file to the test.
    pub(crate) fn source(&mut self, filename: &str, content: &str) -> &mut Self {
        let (clean_content, cursor_offset) = extract_cursor_marker(content);

        self.sources.push(Source {
            filename: filename.to_string(),
            content: clean_content,
            cursor_offset,
        });

        self
    }

    /// Build the cursor test.
    ///
    /// Panics if no cursor marker was found or if multiple markers exist.
    pub(crate) fn build(self) -> CursorTest {
        let cursor_files: Vec<_> = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| s.cursor_offset.is_some())
            .collect();

        assert!(
            cursor_files.len() == 1,
            "Expected exactly one <[CURSOR] marker, found {} across {} files",
            cursor_files.len(),
            self.sources.len()
        );

        let (cursor_file_idx, _) = cursor_files[0];

        // Create and initialize the database with builtins.
        let mut db = TestDb::default();
        db.init();

        // Add user files.
        let mut user_files: Vec<SourceFile> = Vec::new();
        let mut source_file_handles: Vec<SourceFile> = Vec::new();
        for source in &self.sources {
            let path = PathBuf::from("/test").join(&source.filename);
            let file = db.add_file(path, &source.content);
            user_files.push(file);
            source_file_handles.push(file);
        }

        // Update the project's file list.
        db.project.unwrap().set_files(&mut db).to(user_files);

        let cursor_offset = self.sources[cursor_file_idx].cursor_offset.unwrap();
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

/// Extract cursor marker from source, returning cleaned source and offset.
fn extract_cursor_marker(source: &str) -> (String, Option<TextSize>) {
    if let Some(marker_pos) = source.find(CURSOR_MARKER) {
        #[allow(clippy::cast_possible_truncation)]
        let cursor_offset = TextSize::from(marker_pos as u32);

        let mut clean = String::with_capacity(source.len() - CURSOR_MARKER.len());
        clean.push_str(&source[..marker_pos]);
        clean.push_str(&source[marker_pos + CURSOR_MARKER.len()..]);

        (clean, Some(cursor_offset))
    } else {
        (source.to_string(), None)
    }
}

/// Convert a byte offset to a 1-indexed (line, column) pair.
fn offset_to_line_col(content: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(content.len());
    let before = &content[..clamped];
    let line = before.matches('\n').count() + 1;
    let last_newline = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let column = clamped - last_newline + 1;
    (line, column)
}

// ── Project-level test infrastructure (no cursor needed) ────────────────────

/// A test project with multiple BAML files for project-level IDE features.
pub(crate) struct ProjectTest {
    pub(crate) db: TestDb,
    pub(crate) files: Vec<SourceFile>,
}

impl ProjectTest {
    /// Create a builder for multi-file project tests.
    pub(crate) fn builder() -> ProjectTestBuilder {
        ProjectTestBuilder::default()
    }

    /// Run `describe()` on a symbol name and return the results.
    pub(crate) fn describe(&self, name: &str) -> Vec<SymbolDescription> {
        crate::describe::describe(&self.db, &self.files, name)
    }

    /// Run `describe()` using the compiler2-visible file set.
    #[allow(dead_code)]
    pub(crate) fn describe_compiler2_visible(&self, name: &str) -> Vec<SymbolDescription> {
        let files = baml_compiler2_hir::compiler2_all_files(&self.db);
        crate::describe::describe(&self.db, &files, name)
    }

    /// Run `list_package_items()` for the user package and return the result.
    pub(crate) fn list_package_items_user(&self) -> Vec<crate::listing::ListingEntry> {
        let package_id =
            baml_compiler2_hir::package::PackageId::new(&self.db, baml_base::Name::new("user"));
        crate::listing::list_package_items(&self.db, package_id)
    }

    /// Run `list_namespace_items()` for a user namespace.
    pub(crate) fn list_namespace_items_user(
        &self,
        ns_segments: &[&str],
    ) -> Option<Vec<crate::listing::ListingEntry>> {
        let package_id =
            baml_compiler2_hir::package::PackageId::new(&self.db, baml_base::Name::new("user"));
        let ns_path: Vec<baml_base::Name> = ns_segments.iter().map(baml_base::Name::new).collect();
        crate::listing::list_namespace_items(&self.db, package_id, &ns_path)
    }

    /// Format a `ListingEntry` for snapshot comparison.
    pub(crate) fn format_listing_entry(&self, entry: &crate::listing::ListingEntry) -> String {
        let filename = entry
            .file
            .path(&self.db)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        format!(
            "{:<16} {:<32} {}:{}",
            entry.kind.as_str(),
            entry.fqn(),
            filename,
            entry.line,
        )
    }

    /// Format a `SymbolDescription` for snapshot comparison.
    pub(crate) fn format_description(&self, desc: &SymbolDescription) -> String {
        let mut out = String::new();
        let filename = desc
            .file
            .path(&self.db)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let text = desc.file.text(&self.db);
        let offset: usize = desc.name_span.start().into();
        let (line, _col) = offset_to_line_col(text, offset);

        let fqn = desc
            .canonical_fqn
            .as_deref()
            .map(|f| format!("  ({f})"))
            .unwrap_or_default();
        writeln!(
            out,
            "{} {}{}  {}:{}",
            desc.kind, desc.name, fqn, filename, line
        )
        .unwrap();
        if let Some(ref doc) = desc.docstring {
            for line in doc.lines() {
                if line.is_empty() {
                    writeln!(out, "///").unwrap();
                } else {
                    writeln!(out, "/// {line}").unwrap();
                }
            }
        }
        writeln!(out, "shape: {}", desc.shape).unwrap();
        if !desc.instance_methods.is_empty() {
            writeln!(out, "methods:").unwrap();
            for m in &desc.instance_methods {
                if let Some(doc) = &m.docstring {
                    writeln!(out, "  /// {doc}").unwrap();
                }
                writeln!(out, "  {}", m.signature).unwrap();
            }
        }
        if !desc.static_methods.is_empty() {
            writeln!(out, "static_methods:").unwrap();
            for m in &desc.static_methods {
                if let Some(doc) = &m.docstring {
                    writeln!(out, "  /// {doc}").unwrap();
                }
                writeln!(out, "  {}", m.signature).unwrap();
            }
        }
        if let Some(ref c) = desc.container {
            writeln!(out, "container: {}", c.name).unwrap();
        }
        if !desc.dependencies.is_empty() {
            out.push_str("deps:");
            for dep in &desc.dependencies {
                write!(out, " {}", dep.name).unwrap();
            }
            out.push('\n');
        }
        if !desc.references.is_empty() {
            writeln!(out, "refs: {}", desc.references.len()).unwrap();
            for r in &desc.references {
                let rfile = r
                    .file
                    .path(&self.db)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                writeln!(out, "  {}:{}  {}", rfile, r.line_number, r.line_text.trim()).unwrap();
            }
        }
        out
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
        let mut db = TestDb::default();
        db.init();

        let mut user_files: Vec<SourceFile> = Vec::new();
        for (filename, content) in &self.sources {
            let path = PathBuf::from("/test").join(filename);
            let file = db.add_file(path, content);
            user_files.push(file);
        }

        // Update the project's file list.
        db.project
            .unwrap()
            .set_files(&mut db)
            .to(user_files.clone());

        ProjectTest {
            db,
            files: user_files,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cursor_marker() {
        let source = "class Foo<[CURSOR] {}";
        let (clean, offset) = extract_cursor_marker(source);

        assert_eq!(clean, "class Foo {}");
        assert_eq!(offset, Some(TextSize::from(9)));
    }

    #[test]
    fn test_extract_cursor_no_marker() {
        let source = "class Foo {}";
        let (clean, offset) = extract_cursor_marker(source);

        assert_eq!(clean, "class Foo {}");
        assert_eq!(offset, None);
    }
}
