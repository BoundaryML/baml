//! Parser for inline assertion test files.
//!
//! Test files are valid BAML syntax with special comments for assertions.
//!
//! # Format
//!
//! Test files have two sections separated by `//----`:
//!
//! 1. **Source section** (before `//----`): Contains BAML code with cursor markers
//! 2. **Expectations section** (after `//----`): Contains expected diagnostics and hover output
//!
//! ## Virtual Files
//!
//! Multiple virtual files can be defined using `// file: <filename>` markers:
//!
//! ```text
//! // file: types.baml
//! class Foo { ... }
//!
//! // file: main.baml
//! let x = Foo(...)
//! ```
//!
//! ## Cursor Markers
//!
//! Use `<[CURSOR]` to mark cursor position for hover tests:
//!
//! ```text
//! class Person<[CURSOR] {
//!     name string
//! }
//! ```

use std::collections::HashMap;

/// A parsed inline test file.
#[derive(Debug)]
pub struct ParsedTestFile {
    /// Virtual files extracted from the test (filename -> content).
    /// Content excludes the `// file:` marker line and any cursor markers.
    pub files: HashMap<String, VirtualFile>,
    /// Cursor markers found in the source.
    pub cursor_markers: Vec<CursorMarker>,
    /// Expected diagnostics section (raw text after `//- diagnostics`).
    pub expected_diagnostics: String,
    /// Expected hover section (raw text after `//- on_hover expressions`).
    /// None if section is omitted.
    pub expected_hovers: Option<String>,
    /// Expected completions section (after `//- completions`).
    /// None if section is omitted.
    pub expected_completions: Option<CompletionExpectation>,
    /// Expected inlay hints section (after `//- inlay_hints`).
    /// None if section is omitted.
    pub expected_inlay_hints: Option<String>,
    /// User comments in the diagnostics section that should be preserved.
    /// Lines starting with `// (` are treated as preserved comments.
    pub diagnostics_comments: Vec<String>,
    /// User comments in the hovers section that should be preserved.
    pub hovers_comments: Vec<String>,
    /// User comments in the completions section that should be preserved.
    pub completions_comments: Vec<String>,
    /// User comments in the inlay hints section that should be preserved.
    pub inlay_hints_comments: Vec<String>,
}

/// Expected completions for a cursor position.
#[derive(Debug, Clone, Default)]
pub struct CompletionExpectation {
    /// Completion labels that MUST be present.
    pub should_contain: Vec<String>,
}

/// A virtual file within the test.
#[derive(Debug, Clone)]
pub struct VirtualFile {
    /// The filename (e.g., "main.baml").
    pub name: String,
    /// The file content (without the `// file:` marker).
    pub content: String,
    /// Line offset in the original test file where this virtual file starts.
    /// Used for mapping positions back to test file for error messages.
    pub start_line: usize,
}

/// A cursor position marker found in source.
#[derive(Debug, Clone)]
pub struct CursorMarker {
    /// The virtual file containing the cursor.
    pub file: String,
    /// The byte offset within the file (position to LEFT of marker).
    pub offset: usize,
    /// Line number within the virtual file (0-indexed).
    pub line: usize,
    /// Column number within the line (0-indexed).
    pub column: usize,
}

/// The cursor marker used in test sources.
pub const CURSOR_MARKER: &str = "<[CURSOR]";

/// Parse a test file into its components.
pub fn parse_test_file(content: &str, default_filename: &str) -> ParsedTestFile {
    // Split on `//----` to separate source from expectations
    let (source, expectations_raw) = split_on_separator(content);

    // Parse source section for `// file:` markers and cursor markers
    let (files, cursor_markers) = parse_source_section(source, default_filename);

    // Parse expectations section
    let expectations = parse_expectations_section(expectations_raw);

    ParsedTestFile {
        files,
        cursor_markers,
        expected_diagnostics: expectations.diagnostics,
        expected_hovers: expectations.hovers,
        expected_completions: expectations.completions,
        expected_inlay_hints: expectations.inlay_hints,
        diagnostics_comments: expectations.diagnostics_comments,
        hovers_comments: expectations.hovers_comments,
        completions_comments: expectations.completions_comments,
        inlay_hints_comments: expectations.inlay_hints_comments,
    }
}

/// Split the content on `//----` separator.
/// Returns (source_section, expectations_section).
fn split_on_separator(content: &str) -> (&str, &str) {
    if let Some(idx) = content.find("//----") {
        let source = &content[..idx];
        // Skip the "//----" line and any trailing newline
        let rest = &content[idx..];
        let expectations = rest
            .lines()
            .skip(1) // Skip the //---- line itself
            .collect::<Vec<_>>()
            .join("\n");
        // We need to return a &str but we created a String, so leak it for now
        // This is fine since tests are short-lived
        let expectations: &str = Box::leak(expectations.into_boxed_str());
        (source.trim_end(), expectations)
    } else {
        // No separator - entire content is source, no expectations
        (content, "")
    }
}

/// Parse the source section (before `//----`).
fn parse_source_section(
    source: &str,
    default_filename: &str,
) -> (HashMap<String, VirtualFile>, Vec<CursorMarker>) {
    let mut files: HashMap<String, VirtualFile> = HashMap::new();

    let mut current_filename = default_filename.to_string();
    let mut current_content = String::new();
    let mut current_start_line: usize = 0;

    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        // Check for `// file:` marker
        if let Some(filename) = parse_file_marker(line) {
            // Save the current file if it has content
            if !current_content.is_empty() {
                files.insert(
                    current_filename.clone(),
                    VirtualFile {
                        name: current_filename.clone(),
                        content: current_content.trim_end().to_string(),
                        start_line: current_start_line,
                    },
                );
            }

            // Start a new file
            current_filename = filename;
            current_content = String::new();
            current_start_line = i + 1; // Next line is the start
            continue;
        }

        // Add line to current file content
        if !current_content.is_empty() {
            current_content.push('\n');
        }
        current_content.push_str(line);
    }

    // Don't forget the last file
    if !current_content.is_empty() {
        files.insert(
            current_filename.clone(),
            VirtualFile {
                name: current_filename,
                content: current_content.trim_end().to_string(),
                start_line: current_start_line,
            },
        );
    } else if files.is_empty() {
        // Empty source - add an empty default file
        files.insert(
            default_filename.to_string(),
            VirtualFile {
                name: default_filename.to_string(),
                content: String::new(),
                start_line: 0,
            },
        );
    }

    // Extract cursor markers from each file and update the content
    let mut cursor_markers: Vec<CursorMarker> = Vec::new();
    for (filename, vfile) in files.iter_mut() {
        if let Some((clean_content, marker)) = extract_cursor_from_content(&vfile.content, filename)
        {
            vfile.content = clean_content;
            cursor_markers.push(marker);
        }
    }

    (files, cursor_markers)
}

/// Extract cursor marker from file content.
fn extract_cursor_from_content(content: &str, filename: &str) -> Option<(String, CursorMarker)> {
    let marker_pos = content.find(CURSOR_MARKER)?;

    // Calculate line and column
    let before_marker = &content[..marker_pos];
    let line = before_marker.matches('\n').count();
    let last_newline = before_marker.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let column = marker_pos - last_newline;

    // Remove marker from content
    let mut clean = String::with_capacity(content.len() - CURSOR_MARKER.len());
    clean.push_str(&content[..marker_pos]);
    clean.push_str(&content[marker_pos + CURSOR_MARKER.len()..]);

    Some((
        clean,
        CursorMarker {
            file: filename.to_string(),
            offset: marker_pos,
            line,
            column,
        },
    ))
}

/// Parse a `// file: <filename>` marker.
fn parse_file_marker(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("// file:") {
        let filename = trimmed.strip_prefix("// file:")?.trim();
        if !filename.is_empty() {
            return Some(filename.to_string());
        }
    }
    None
}

/// Check if a line is a preserved user comment.
/// Preserved comments start with `// (` and are kept during UPDATE_EXPECT.
pub fn is_preserved_comment(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("// (")
}

/// Parsed expectations from the test file.
struct ParsedExpectations {
    diagnostics: String,
    hovers: Option<String>,
    completions: Option<CompletionExpectation>,
    inlay_hints: Option<String>,
    diagnostics_comments: Vec<String>,
    hovers_comments: Vec<String>,
    completions_comments: Vec<String>,
    inlay_hints_comments: Vec<String>,
}

/// Parse the expectations section (after `//----`).
fn parse_expectations_section(section: &str) -> ParsedExpectations {
    if section.is_empty() {
        return ParsedExpectations {
            diagnostics: String::new(),
            hovers: None,
            completions: None,
            inlay_hints: None,
            diagnostics_comments: Vec::new(),
            hovers_comments: Vec::new(),
            completions_comments: Vec::new(),
            inlay_hints_comments: Vec::new(),
        };
    }

    let mut diagnostics = String::new();
    let mut hovers: Option<String> = None;
    let mut completions: Option<CompletionExpectation> = None;
    let mut inlay_hints: Option<String> = None;
    let mut diagnostics_comments: Vec<String> = Vec::new();
    let mut hovers_comments: Vec<String> = Vec::new();
    let mut completions_comments: Vec<String> = Vec::new();
    let mut inlay_hints_comments: Vec<String> = Vec::new();

    // Find the subsection markers
    let lines: Vec<&str> = section.lines().collect();
    let mut current_section: Option<&str> = None;
    let mut current_content = String::new();
    let mut current_comments: Vec<String> = Vec::new();

    for line in lines {
        let trimmed = line.trim();

        // Check for section markers
        if trimmed == "//- diagnostics" || trimmed.starts_with("//- diagnostics ") {
            // Save previous section if any
            if let Some(section_name) = current_section {
                save_section_with_comments(
                    section_name,
                    &current_content,
                    &current_comments,
                    &mut diagnostics,
                    &mut hovers,
                    &mut completions,
                    &mut inlay_hints,
                    &mut diagnostics_comments,
                    &mut hovers_comments,
                    &mut completions_comments,
                    &mut inlay_hints_comments,
                );
            }
            current_section = Some("diagnostics");
            current_content = String::new();
            current_comments = Vec::new();
            continue;
        }

        if trimmed == "//- on_hover expressions" {
            // Save previous section if any
            if let Some(section_name) = current_section {
                save_section_with_comments(
                    section_name,
                    &current_content,
                    &current_comments,
                    &mut diagnostics,
                    &mut hovers,
                    &mut completions,
                    &mut inlay_hints,
                    &mut diagnostics_comments,
                    &mut hovers_comments,
                    &mut completions_comments,
                    &mut inlay_hints_comments,
                );
            }
            current_section = Some("hovers");
            current_content = String::new();
            current_comments = Vec::new();
            continue;
        }

        if trimmed == "//- completions" {
            // Save previous section if any
            if let Some(section_name) = current_section {
                save_section_with_comments(
                    section_name,
                    &current_content,
                    &current_comments,
                    &mut diagnostics,
                    &mut hovers,
                    &mut completions,
                    &mut inlay_hints,
                    &mut diagnostics_comments,
                    &mut hovers_comments,
                    &mut completions_comments,
                    &mut inlay_hints_comments,
                );
            }
            current_section = Some("completions");
            current_content = String::new();
            current_comments = Vec::new();
            continue;
        }

        if trimmed == "//- inlay_hints" {
            // Save previous section if any
            if let Some(section_name) = current_section {
                save_section_with_comments(
                    section_name,
                    &current_content,
                    &current_comments,
                    &mut diagnostics,
                    &mut hovers,
                    &mut completions,
                    &mut inlay_hints,
                    &mut diagnostics_comments,
                    &mut hovers_comments,
                    &mut completions_comments,
                    &mut inlay_hints_comments,
                );
            }
            current_section = Some("inlay_hints");
            current_content = String::new();
            current_comments = Vec::new();
            continue;
        }

        // Skip empty comment lines that are just section separators
        if trimmed == "//" && current_section.is_some() {
            continue;
        }

        // Check for preserved comments
        if current_section.is_some() && is_preserved_comment(line) {
            current_comments.push(line.to_string());
            continue;
        }

        // Add line to current section
        if current_section.is_some() {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    // Save the last section
    if let Some(section_name) = current_section {
        save_section_with_comments(
            section_name,
            &current_content,
            &current_comments,
            &mut diagnostics,
            &mut hovers,
            &mut completions,
            &mut inlay_hints,
            &mut diagnostics_comments,
            &mut hovers_comments,
            &mut completions_comments,
            &mut inlay_hints_comments,
        );
    }

    ParsedExpectations {
        diagnostics: diagnostics.trim().to_string(),
        hovers: hovers.map(|s| s.trim().to_string()),
        completions,
        inlay_hints: inlay_hints.map(|s| s.trim().to_string()),
        diagnostics_comments,
        hovers_comments,
        completions_comments,
        inlay_hints_comments,
    }
}

/// Parse completion expectations from the raw section content.
/// Format: `// SHOULD_CONTAIN: item1, item2, item3`
fn parse_completion_content(content: &str) -> CompletionExpectation {
    let mut expectation = CompletionExpectation::default();

    for line in content.lines() {
        let trimmed = line.trim();
        // Strip leading `// ` if present
        let stripped = trimmed
            .strip_prefix("// ")
            .or_else(|| trimmed.strip_prefix("//"))
            .unwrap_or(trimmed);

        if let Some(items) = stripped.strip_prefix("SHOULD_CONTAIN:") {
            let items: Vec<String> = items
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            expectation.should_contain.extend(items);
        }
    }

    expectation
}

#[allow(clippy::too_many_arguments)]
fn save_section_with_comments(
    section_name: &str,
    content: &str,
    comments: &[String],
    diagnostics: &mut String,
    hovers: &mut Option<String>,
    completions: &mut Option<CompletionExpectation>,
    inlay_hints: &mut Option<String>,
    diagnostics_comments: &mut Vec<String>,
    hovers_comments: &mut Vec<String>,
    completions_comments: &mut Vec<String>,
    inlay_hints_comments: &mut Vec<String>,
) {
    let content_trimmed = content.trim().to_string();
    match section_name {
        "diagnostics" => {
            *diagnostics = content_trimmed;
            *diagnostics_comments = comments.to_vec();
        }
        "hovers" => {
            *hovers = Some(content_trimmed);
            *hovers_comments = comments.to_vec();
        }
        "completions" => {
            *completions = Some(parse_completion_content(&content_trimmed));
            *completions_comments = comments.to_vec();
        }
        "inlay_hints" => {
            *inlay_hints = Some(content_trimmed);
            *inlay_hints_comments = comments.to_vec();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_file() {
        let content = r#"class Foo {
    name string
}

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.files.len(), 1);
        assert!(parsed.files.contains_key("test.baml"));

        let file = &parsed.files["test.baml"];
        assert!(file.content.contains("class Foo"));
        assert_eq!(parsed.expected_diagnostics, "// <no-diagnostics-expected>");
        assert!(parsed.expected_hovers.is_none());
    }

    #[test]
    fn test_parse_multi_file() {
        let content = r#"// file: types.baml
class Address {
    street string
}

// file: main.baml
class Person {
    home Address
}

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.files.len(), 2);
        assert!(parsed.files.contains_key("types.baml"));
        assert!(parsed.files.contains_key("main.baml"));

        let types_file = &parsed.files["types.baml"];
        assert!(types_file.content.contains("class Address"));

        let main_file = &parsed.files["main.baml"];
        assert!(main_file.content.contains("class Person"));
    }

    #[test]
    fn test_parse_expectations_with_hovers() {
        let content = r#"class Foo {}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// `Foo` at test.baml:1:1
// class Foo {}"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.expected_diagnostics, "// <no-diagnostics-expected>");
        assert!(parsed.expected_hovers.is_some());
        let hovers = parsed.expected_hovers.unwrap();
        assert!(hovers.contains("`Foo`"));
    }

    #[test]
    fn test_no_separator() {
        let content = r#"class Foo {
    name string
}"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.files.len(), 1);
        assert!(parsed.expected_diagnostics.is_empty());
        assert!(parsed.expected_hovers.is_none());
    }

    #[test]
    fn test_file_marker_parsing() {
        assert_eq!(
            parse_file_marker("// file: test.baml"),
            Some("test.baml".to_string())
        );
        assert_eq!(
            parse_file_marker("// file:main.baml"),
            Some("main.baml".to_string())
        );
        assert_eq!(parse_file_marker("// not a file marker"), None);
        assert_eq!(parse_file_marker("class Foo {}"), None);
    }

    #[test]
    fn test_preserved_comments_parsing() {
        let content = r#"class Foo {}

//----
//- diagnostics
// (expect no errors here)
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.diagnostics_comments.len(), 1);
        assert_eq!(parsed.diagnostics_comments[0], "// (expect no errors here)");
        assert_eq!(parsed.expected_diagnostics, "// <no-diagnostics-expected>");
    }

    #[test]
    fn test_preserved_comments_not_in_diagnostics() {
        // Comments that don't start with `// (` should NOT be preserved
        let content = r#"class Foo {}

//----
//- diagnostics
// regular comment
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.diagnostics_comments.len(), 0);
        // The regular comment is part of the diagnostics content
        assert!(parsed.expected_diagnostics.contains("// regular comment"));
    }

    #[test]
    fn test_preserved_comments_in_hovers_section() {
        let content = r#"class Foo {}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (testing hover on Foo)
// `Foo` at test.baml:1"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.hovers_comments.len(), 1);
        assert_eq!(parsed.hovers_comments[0], "// (testing hover on Foo)");
        assert!(parsed.expected_hovers.is_some());
        assert!(parsed.expected_hovers.as_ref().unwrap().contains("`Foo`"));
    }

    #[test]
    fn test_multiple_preserved_comments() {
        let content = r#"class Foo {}

//----
//- diagnostics
// (first comment)
// (second comment)
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.diagnostics_comments.len(), 2);
        assert_eq!(parsed.diagnostics_comments[0], "// (first comment)");
        assert_eq!(parsed.diagnostics_comments[1], "// (second comment)");
    }

    #[test]
    fn test_is_preserved_comment() {
        assert!(is_preserved_comment("// (this is preserved)"));
        assert!(is_preserved_comment("  // (with leading spaces)"));
        assert!(!is_preserved_comment("// not preserved"));
        assert!(!is_preserved_comment("// <no-diagnostics-expected>"));
        assert!(!is_preserved_comment("class Foo {}"));
    }

    #[test]
    fn test_extract_cursor_from_content() {
        let content = "class Foo<[CURSOR] {}";
        let result = extract_cursor_from_content(content, "test.baml");

        assert!(result.is_some());
        let (clean, marker) = result.unwrap();
        assert_eq!(clean, "class Foo {}");
        assert_eq!(marker.file, "test.baml");
        assert_eq!(marker.offset, 9);
        assert_eq!(marker.line, 0);
        assert_eq!(marker.column, 9);
    }

    #[test]
    fn test_extract_cursor_no_marker() {
        let content = "class Foo {}";
        let result = extract_cursor_from_content(content, "test.baml");

        assert!(result.is_none());
    }

    #[test]
    fn test_cursor_marker_multiline() {
        // "class Foo {\n    name<[CURSOR] string\n}"
        // Line 0: "class Foo {\n" = 12 bytes
        // Line 1: "    name" = 8 bytes, so offset = 12 + 8 = 20
        let content = "class Foo {\n    name<[CURSOR] string\n}";
        let result = extract_cursor_from_content(content, "test.baml");

        assert!(result.is_some());
        let (clean, marker) = result.unwrap();
        assert_eq!(clean, "class Foo {\n    name string\n}");
        assert_eq!(marker.file, "test.baml");
        assert_eq!(marker.offset, 20); // "class Foo {\n" (12) + "    name" (8) = 20
        assert_eq!(marker.line, 1);
        assert_eq!(marker.column, 8); // 4 spaces + "name" (4) = 8 chars from start of line
    }

    #[test]
    fn test_parse_test_file_with_cursor() {
        let content = r#"class Person<[CURSOR] {
    name string
}

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.cursor_markers.len(), 1);

        let marker = &parsed.cursor_markers[0];
        assert_eq!(marker.file, "test.baml");
        assert_eq!(marker.offset, 12); // Right after "Person"
        assert_eq!(marker.line, 0);
        assert_eq!(marker.column, 12);

        // Content should have marker removed
        let file = &parsed.files["test.baml"];
        assert!(!file.content.contains("<[CURSOR]"));
        assert!(file.content.contains("class Person {"));
    }

    #[test]
    fn test_parse_multi_file_with_cursor() {
        let content = r#"// file: types.baml
class Address {
    street string
}

// file: main.baml
class Person<[CURSOR] {
    home Address
}

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.cursor_markers.len(), 1);

        let marker = &parsed.cursor_markers[0];
        assert_eq!(marker.file, "main.baml");

        // Verify content is clean
        assert!(!parsed.files["main.baml"].content.contains("<[CURSOR]"));
        assert!(!parsed.files["types.baml"].content.contains("<[CURSOR]"));
    }

    #[test]
    fn test_parse_completions_section() {
        let content = r#"<[CURSOR]

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// SHOULD_CONTAIN: function, class, enum"#;

        let parsed = parse_test_file(content, "test.baml");

        assert!(parsed.expected_completions.is_some());
        let completions = parsed.expected_completions.unwrap();
        assert_eq!(completions.should_contain.len(), 3);
        assert!(completions.should_contain.contains(&"function".to_string()));
        assert!(completions.should_contain.contains(&"class".to_string()));
        assert!(completions.should_contain.contains(&"enum".to_string()));
    }

    #[test]
    fn test_parse_completions_multiple_lines() {
        let content = r#"<[CURSOR]

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// SHOULD_CONTAIN: function, class
// SHOULD_CONTAIN: enum, client"#;

        let parsed = parse_test_file(content, "test.baml");

        assert!(parsed.expected_completions.is_some());
        let completions = parsed.expected_completions.unwrap();
        assert_eq!(completions.should_contain.len(), 4);
        assert!(completions.should_contain.contains(&"function".to_string()));
        assert!(completions.should_contain.contains(&"class".to_string()));
        assert!(completions.should_contain.contains(&"enum".to_string()));
        assert!(completions.should_contain.contains(&"client".to_string()));
    }

    #[test]
    fn test_parse_completions_with_preserved_comment() {
        let content = r#"<[CURSOR]

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// (testing top-level keywords)
// SHOULD_CONTAIN: function, class"#;

        let parsed = parse_test_file(content, "test.baml");

        assert!(parsed.expected_completions.is_some());
        assert_eq!(parsed.completions_comments.len(), 1);
        assert_eq!(
            parsed.completions_comments[0],
            "// (testing top-level keywords)"
        );

        let completions = parsed.expected_completions.unwrap();
        assert_eq!(completions.should_contain.len(), 2);
    }
}
