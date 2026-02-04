//! Test infrastructure for IDE features.
//!
//! Provides cursor-based testing where `<[CURSOR]` markers indicate
//! the cursor position (immediately to the LEFT of the marker).

use std::{collections::HashMap, path::PathBuf};

use baml_db::{Setter, SourceFile, baml_workspace::Project};
use baml_project::{ProjectDatabase, position::LineIndex};
use text_size::TextSize;

/// The cursor marker used in test sources.
/// The cursor position is immediately to the LEFT of this marker.
pub const CURSOR_MARKER: &str = "<[CURSOR]";

/// A test with cursor position information.
pub struct CursorTest {
    /// The test database.
    pub db: ProjectDatabase,
    /// The project.
    pub project: Project,
    /// Information about the cursor.
    pub cursor: Cursor,
}

/// Cursor position and context.
pub struct Cursor {
    /// The file containing the cursor.
    pub file: SourceFile,
    /// The byte offset of the cursor (position to the LEFT of marker).
    pub offset: TextSize,
    /// The source text (without marker).
    pub source: String,
}

impl CursorTest {
    /// Create a new cursor test from source with a `<[CURSOR]` marker.
    ///
    /// The source must contain exactly one `<[CURSOR]` marker.
    /// The cursor position is the byte offset immediately to the LEFT of the marker.
    pub fn new(source: &str) -> Self {
        Self::with_filename("test.baml", source)
    }

    /// Create a new cursor test with a specific filename.
    pub fn with_filename(filename: &str, source: &str) -> Self {
        let mut builder = CursorTestBuilder::default();
        builder.source(filename, source);
        builder.build()
    }

    /// Create a builder for multi-file tests.
    pub fn builder() -> CursorTestBuilder {
        CursorTestBuilder::default()
    }

    /// Get hover information at the cursor position.
    pub fn hover(&self) -> String {
        use crate::{MarkupKind, hover::hover};

        match hover(&self.db, self.cursor.file, self.project, self.cursor.offset) {
            Some(ranged_hover) => {
                let mut buf = String::new();

                // Plain text rendering
                buf.push_str(&ranged_hover.display(MarkupKind::PlainText));
                buf.push_str("\n---\n");

                // Markdown rendering
                buf.push_str(&ranged_hover.display(MarkupKind::Markdown));

                buf
            }
            None => "No hover content".to_string(),
        }
    }

    /// Get goto-definition result at the cursor position.
    ///
    /// Returns the location of the definition as a string in the format:
    /// - `"file.baml:line:col -> TARGET_NAME"` if definition found
    /// - `"No definition found"` if no definition found
    pub fn goto_definition(&self) -> String {
        use crate::goto_definition::goto_definition;

        let file_id = self.cursor.file.file_id(&self.db);
        match goto_definition(&self.db, file_id, self.cursor.offset) {
            Some(nav_target) => {
                let filename = self
                    .db
                    .file_id_to_path(nav_target.span.file_id)
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                // Get the source text for the target file to convert offset to line:col
                let source_files = self.db.get_source_files();
                let position_str = source_files
                    .iter()
                    .find(|f| f.file_id(&self.db) == nav_target.span.file_id)
                    .map(|source_file| {
                        let text = source_file.text(&self.db);
                        let line_index = LineIndex::new(text);
                        let start_offset: u32 = nav_target.span.range.start().into();
                        line_index
                            .offset_to_position(start_offset)
                            .map(|pos| format!("{}:{}", pos.line + 1, pos.character + 1))
                            .unwrap_or_else(|| "?:?".to_string())
                    })
                    .unwrap_or_else(|| "?:?".to_string());

                format!("{}:{} -> {}", filename, position_str, nav_target.name)
            }
            None => "No definition found".to_string(),
        }
    }

    /// Find all references to the symbol at the cursor position.
    ///
    /// Returns a list of locations where the symbol is referenced.
    pub fn find_all_references(&self) -> Vec<String> {
        use crate::find_references::find_all_references;

        let file_id = self.cursor.file.file_id(&self.db);
        let references = find_all_references(&self.db, file_id, self.cursor.offset);

        if references.is_empty() {
            vec!["No references found".to_string()]
        } else {
            references
                .into_iter()
                .map(|reference| {
                    // For simplicity, just return the filename
                    let filename = self
                        .db
                        .file_id_to_path(reference.span.file_id)
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    filename.to_string()
                })
                .collect()
        }
    }
}

/// Builder for cursor tests supporting multiple files.
#[derive(Default)]
pub struct CursorTestBuilder {
    sources: Vec<Source>,
}

struct Source {
    filename: String,
    content: String,
    cursor_offset: Option<TextSize>,
}

impl CursorTestBuilder {
    /// Add a source file to the test.
    ///
    /// The file may contain a `<[CURSOR]` marker. Only one file across
    /// all added sources may contain the marker.
    pub fn source(&mut self, filename: &str, content: &str) -> &mut Self {
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
    pub fn build(self) -> CursorTest {
        // Find the file with the cursor
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

        // Create database
        let mut db = ProjectDatabase::default();

        // Create project first
        let project = db.set_project_root(&PathBuf::from("/test"));

        // Add files to database
        let mut file_map: HashMap<String, SourceFile> = HashMap::new();
        for source in &self.sources {
            let path = PathBuf::from("/test").join(&source.filename);
            let file = db.add_file(path, &source.content);
            file_map.insert(source.filename.clone(), file);
        }

        // Update project with files
        let files: Vec<SourceFile> = file_map.values().copied().collect();
        project.set_files(&mut db).to(files);

        // Get cursor info
        let cursor_source = &self.sources[cursor_file_idx];
        let cursor_file = file_map[&cursor_source.filename];
        let cursor_offset = cursor_source.cursor_offset.unwrap();

        CursorTest {
            db,
            project,
            cursor: Cursor {
                file: cursor_file,
                offset: cursor_offset,
                source: cursor_source.content.clone(),
            },
        }
    }
}

/// Extract cursor marker from source, returning cleaned source and offset.
///
/// The cursor position is the byte offset of the character immediately
/// to the LEFT of the `<[CURSOR]` marker.
fn extract_cursor_marker(source: &str) -> (String, Option<TextSize>) {
    if let Some(marker_pos) = source.find(CURSOR_MARKER) {
        // The cursor is at the position where the marker starts
        // (i.e., to the LEFT of the marker)
        #[allow(clippy::cast_possible_truncation)]
        let cursor_offset = TextSize::from(marker_pos as u32);

        // Remove the marker from the source
        let mut clean = String::with_capacity(source.len() - CURSOR_MARKER.len());
        clean.push_str(&source[..marker_pos]);
        clean.push_str(&source[marker_pos + CURSOR_MARKER.len()..]);

        (clean, Some(cursor_offset))
    } else {
        (source.to_string(), None)
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
        assert_eq!(offset, Some(TextSize::from(9))); // Position after "Foo"
    }

    #[test]
    fn test_extract_cursor_no_marker() {
        let source = "class Foo {}";
        let (clean, offset) = extract_cursor_marker(source);

        assert_eq!(clean, "class Foo {}");
        assert_eq!(offset, None);
    }

    #[test]
    fn test_cursor_at_identifier_end() {
        // Cursor after "Person" - should hover over "Person"
        let source = "class Person<[CURSOR] { name string }";
        let (clean, offset) = extract_cursor_marker(source);

        assert_eq!(clean, "class Person { name string }");
        assert_eq!(offset, Some(TextSize::from(12))); // Right after "Person"
    }
}
