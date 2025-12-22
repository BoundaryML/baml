//! Parser for inline assertion test files.
//!
//! Test files are valid BAML syntax with special comments for assertions.
//!
//! # Format
//!
//! Test files have two sections separated by `//----`:
//!
//! 1. **Source section** (before `//----`): Contains BAML code and inline assertions
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
//! ## Inline Hover Assertions
//!
//! Single-line: `// on_hover `symbol`: expected text`
//! Multi-line:
//! ```text
//! // on_hover `symbol`: expected text
//! //   continues on next line
//! //   and more lines
//! ```

use std::collections::HashMap;

/// A parsed inline test file.
#[derive(Debug)]
pub struct ParsedTestFile {
    /// Virtual files extracted from the test (filename -> content).
    /// Content excludes the `// file:` marker line.
    pub files: HashMap<String, VirtualFile>,
    /// Hover assertions found in the source.
    pub hover_assertions: Vec<HoverAssertion>,
    /// Expected diagnostics section (raw text after `//- diagnostics`).
    pub expected_diagnostics: String,
    /// Expected hover section (raw text after `//- on_hover expressions`).
    /// None if section is omitted.
    pub expected_hovers: Option<String>,
    /// User comments in the diagnostics section that should be preserved.
    /// Lines starting with `// (` are treated as preserved comments.
    pub diagnostics_comments: Vec<String>,
    /// User comments in the hovers section that should be preserved.
    pub hovers_comments: Vec<String>,
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

/// A hover assertion from the source code.
#[derive(Debug, Clone)]
pub struct HoverAssertion {
    /// The symbol to hover over (e.g., "x", "Bar").
    pub symbol: String,
    /// The expected hover text.
    pub expected_text: String,
    /// The virtual file containing this assertion.
    pub file: String,
    /// Line number within the virtual file (0-indexed).
    pub line: usize,
}

/// Parse a test file into its components.
pub fn parse_test_file(content: &str, default_filename: &str) -> ParsedTestFile {
    // Split on `//----` to separate source from expectations
    let (source, expectations) = split_on_separator(content);

    // Parse source section for `// file:` markers and hover assertions
    let (files, hover_assertions) = parse_source_section(source, default_filename);

    // Parse expectations section for `//- diagnostics` and `//- on_hover expressions`
    let (expected_diagnostics, expected_hovers, diagnostics_comments, hovers_comments) =
        parse_expectations_section(expectations);

    ParsedTestFile {
        files,
        hover_assertions,
        expected_diagnostics,
        expected_hovers,
        diagnostics_comments,
        hovers_comments,
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
) -> (HashMap<String, VirtualFile>, Vec<HoverAssertion>) {
    let mut files: HashMap<String, VirtualFile> = HashMap::new();
    let mut hover_assertions: Vec<HoverAssertion> = Vec::new();

    let mut current_filename = default_filename.to_string();
    let mut current_content = String::new();
    let mut current_start_line: usize = 0;
    let mut line_in_current_file: usize = 0;

    // State for multi-line hover assertion parsing
    let mut pending_hover: Option<(String, String, String, usize)> = None; // (symbol, expected_text, file, line)

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Check for `// file:` marker
        if let Some(filename) = parse_file_marker(line) {
            // Finalize any pending hover assertion
            if let Some((symbol, expected_text, file, hover_line)) = pending_hover.take() {
                hover_assertions.push(HoverAssertion {
                    symbol,
                    expected_text,
                    file,
                    line: hover_line,
                });
            }

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
            line_in_current_file = 0;
            i += 1;
            continue;
        }

        // Check for hover assertion start
        if let Some((symbol, first_line_text)) = parse_inline_hover(line) {
            // Finalize any previous pending hover assertion
            if let Some((prev_symbol, prev_text, prev_file, prev_line)) = pending_hover.take() {
                hover_assertions.push(HoverAssertion {
                    symbol: prev_symbol,
                    expected_text: prev_text,
                    file: prev_file,
                    line: prev_line,
                });
            }

            // Start new hover assertion
            pending_hover = Some((
                symbol,
                first_line_text,
                current_filename.clone(),
                line_in_current_file.saturating_sub(1), // Hover is for the previous line
            ));

            // Add line to current file content
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
            line_in_current_file += 1;
            i += 1;
            continue;
        }

        // Check for hover assertion continuation
        if pending_hover.is_some() && is_hover_continuation(line) {
            // Append to pending hover's expected text
            if let Some((symbol, expected_text, file, hover_line)) = pending_hover.as_mut() {
                let continuation = extract_hover_continuation(line);
                expected_text.push('\n');
                expected_text.push_str(&continuation);

                // Add line to current file content
                if !current_content.is_empty() {
                    current_content.push('\n');
                }
                current_content.push_str(line);
                line_in_current_file += 1;
                i += 1;

                // Clone values to avoid borrow issues
                let _ = (symbol, file, hover_line);
                continue;
            }
        }

        // Not a continuation - finalize any pending hover
        if let Some((symbol, expected_text, file, hover_line)) = pending_hover.take() {
            hover_assertions.push(HoverAssertion {
                symbol,
                expected_text,
                file,
                line: hover_line,
            });
        }

        // Add line to current file content
        if !current_content.is_empty() {
            current_content.push('\n');
        }
        current_content.push_str(line);
        line_in_current_file += 1;
        i += 1;
    }

    // Finalize any pending hover assertion
    if let Some((symbol, expected_text, file, hover_line)) = pending_hover.take() {
        hover_assertions.push(HoverAssertion {
            symbol,
            expected_text,
            file,
            line: hover_line,
        });
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

    (files, hover_assertions)
}

/// Check if a line is a continuation of a multi-line hover assertion.
/// Continuation lines start with `//` followed by space or more content.
fn is_hover_continuation(line: &str) -> bool {
    let trimmed = line.trim();
    // Must start with // but NOT be a new on_hover, expect on_hover, or file marker
    trimmed.starts_with("//")
        && !trimmed.starts_with("// on_hover")
        && !trimmed.starts_with("// expect on_hover")
        && !trimmed.starts_with("// file:")
        && !trimmed.starts_with("//----")
}

/// Extract the content from a hover continuation line.
/// Removes the leading `//` and preserves the rest (including leading spaces for indentation).
fn extract_hover_continuation(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed == "//" {
        // Empty line
        String::new()
    } else if let Some(rest) = trimmed.strip_prefix("// ") {
        rest.to_string()
    } else if let Some(rest) = trimmed.strip_prefix("//") {
        rest.to_string()
    } else {
        trimmed.to_string()
    }
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

/// Parse the expectations section (after `//----`).
/// Returns (diagnostics, hovers, diagnostics_comments, hovers_comments).
fn parse_expectations_section(section: &str) -> (String, Option<String>, Vec<String>, Vec<String>) {
    if section.is_empty() {
        return (String::new(), None, Vec::new(), Vec::new());
    }

    let mut diagnostics = String::new();
    let mut hovers: Option<String> = None;
    let mut diagnostics_comments: Vec<String> = Vec::new();
    let mut hovers_comments: Vec<String> = Vec::new();

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
                    &mut diagnostics_comments,
                    &mut hovers_comments,
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
                    &mut diagnostics_comments,
                    &mut hovers_comments,
                );
            }
            current_section = Some("hovers");
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
            &mut diagnostics_comments,
            &mut hovers_comments,
        );
    }

    (
        diagnostics.trim().to_string(),
        hovers.map(|s| s.trim().to_string()),
        diagnostics_comments,
        hovers_comments,
    )
}

fn save_section_with_comments(
    section_name: &str,
    content: &str,
    comments: &[String],
    diagnostics: &mut String,
    hovers: &mut Option<String>,
    diagnostics_comments: &mut Vec<String>,
    hovers_comments: &mut Vec<String>,
) {
    let content = content.trim().to_string();
    match section_name {
        "diagnostics" => {
            *diagnostics = content;
            *diagnostics_comments = comments.to_vec();
        }
        "hovers" => {
            *hovers = Some(content);
            *hovers_comments = comments.to_vec();
        }
        _ => {}
    }
}

/// Extract hover assertions from a single line comment.
/// Formats:
/// - `// on_hover `symbol`: expected_text`
/// - `// expect on_hover `symbol`: expected_text`
fn parse_inline_hover(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Check for both on_hover patterns
    let rest = if trimmed.starts_with("// on_hover `") {
        trimmed.strip_prefix("// on_hover `")?
    } else if trimmed.starts_with("// expect on_hover `") {
        trimmed.strip_prefix("// expect on_hover `")?
    } else {
        return None;
    };

    // Extract the symbol between backticks
    let backtick_end = rest.find('`')?;
    let symbol = &rest[..backtick_end];

    // Get the expected text after `: `
    let after_symbol = &rest[backtick_end + 1..];
    let expected = after_symbol.strip_prefix(": ")?.trim();

    Some((symbol.to_string(), expected.to_string()))
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
    fn test_parse_hover_assertion() {
        let content = r#"let x = 42
// on_hover `x`: int

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.hover_assertions.len(), 1);
        let assertion = &parsed.hover_assertions[0];
        assert_eq!(assertion.symbol, "x");
        assert_eq!(assertion.expected_text, "int");
        assert_eq!(assertion.file, "test.baml");
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
    fn test_inline_hover_parsing() {
        assert_eq!(
            parse_inline_hover("// on_hover `x`: int"),
            Some(("x".to_string(), "int".to_string()))
        );
        assert_eq!(
            parse_inline_hover("// on_hover `Foo`: class Foo {}"),
            Some(("Foo".to_string(), "class Foo {}".to_string()))
        );
        assert_eq!(parse_inline_hover("// not a hover"), None);
        assert_eq!(parse_inline_hover("class Foo {}"), None);
    }

    #[test]
    fn test_expect_on_hover_parsing() {
        // Test the alternative `// expect on_hover` syntax
        assert_eq!(
            parse_inline_hover("// expect on_hover `x`: int"),
            Some(("x".to_string(), "int".to_string()))
        );
        assert_eq!(
            parse_inline_hover("// expect on_hover `Foo`: class Foo {}"),
            Some(("Foo".to_string(), "class Foo {}".to_string()))
        );
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
    fn test_multiline_hover_assertion() {
        let content = r#"class Foo {
    x int
}
// on_hover `Foo`: class Foo {
//   x int
// }

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.hover_assertions.len(), 1);
        let assertion = &parsed.hover_assertions[0];
        assert_eq!(assertion.symbol, "Foo");
        assert_eq!(assertion.expected_text, "class Foo {\n  x int\n}");
    }

    #[test]
    fn test_multiline_expect_on_hover() {
        let content = r#"class Bar {
    y string
}
// expect on_hover `Bar`: class Bar {
//   y string
// }

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");

        assert_eq!(parsed.hover_assertions.len(), 1);
        let assertion = &parsed.hover_assertions[0];
        assert_eq!(assertion.symbol, "Bar");
        assert_eq!(assertion.expected_text, "class Bar {\n  y string\n}");
    }

    #[test]
    fn test_is_hover_continuation() {
        assert!(is_hover_continuation("//   continued"));
        assert!(is_hover_continuation("// }"));
        assert!(!is_hover_continuation("// on_hover `x`: int"));
        assert!(!is_hover_continuation("// expect on_hover `x`: int"));
        assert!(!is_hover_continuation("// file: test.baml"));
        assert!(!is_hover_continuation("//----"));
        assert!(!is_hover_continuation("class Foo {}"));
    }
}
