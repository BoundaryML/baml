//! Expectation updater for inline assertion tests.
//!
//! When `UPDATE_EXPECT=1` is set, this module rewrites test files
//! with the actual diagnostics and hover output.

use std::path::Path;

use super::runner::TestResult;

/// Update the test file with actual output.
pub fn update_test_file(
    path: &Path,
    original_content: &str,
    result: &TestResult,
) -> std::io::Result<()> {
    // Find the `//----` marker in original content
    let source = if let Some(idx) = original_content.find("//----") {
        &original_content[..idx]
    } else {
        original_content
    };

    // Extract completion section from original if it exists (we don't auto-generate completions)
    let completion_section = extract_completion_section(original_content);

    // Generate new expectations section, preserving user comments
    let expectations = generate_expectations_section(
        &result.actual_diagnostics,
        result.actual_hovers.as_deref(),
        completion_section.as_deref(),
        &result.diagnostics_comments,
        &result.hovers_comments,
        &result.completions_comments,
    );

    // Combine source and new expectations
    let new_content = format!("{}{}", source.trim_end(), expectations);

    // Write back to file
    std::fs::write(path, new_content)
}

/// Extract the completion section from original content.
/// Returns the raw content after `//- completions` marker (excluding comments).
fn extract_completion_section(content: &str) -> Option<String> {
    let marker = "//- completions";
    let marker_pos = content.find(marker)?;
    let after_marker = &content[marker_pos + marker.len()..];

    // Find where the next section starts (or end of file)
    let section_end = after_marker
        .find("//- diagnostics")
        .or_else(|| after_marker.find("//- on_hover"))
        .unwrap_or(after_marker.len());

    let section = &after_marker[..section_end];

    // Parse out just the SHOULD_CONTAIN lines (skip empty lines and preserved comments)
    let lines: Vec<&str> = section
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && trimmed != "//" && !trimmed.starts_with("// (") // Skip preserved comments (added separately)
        })
        .collect();

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Generate the expectations section from actual output.
/// Preserves user comments (lines starting with `// (`) from the original file.
fn generate_expectations_section(
    diagnostics: &str,
    hovers: Option<&str>,
    completions: Option<&str>,
    diagnostics_comments: &[String],
    hovers_comments: &[String],
    completions_comments: &[String],
) -> String {
    let mut section = String::new();
    section.push_str("\n\n//----\n");
    section.push_str("//- diagnostics\n");

    // Add preserved diagnostics comments first
    for comment in diagnostics_comments {
        section.push_str(comment);
        section.push('\n');
    }

    section.push_str(diagnostics);
    section.push('\n');

    if let Some(hovers) = hovers {
        section.push_str("//\n");
        section.push_str("//- on_hover expressions\n");

        // Add preserved hovers comments first
        for comment in hovers_comments {
            section.push_str(comment);
            section.push('\n');
        }

        section.push_str(hovers);
        section.push('\n');
    }

    if let Some(completions) = completions {
        section.push_str("//\n");
        section.push_str("//- completions\n");

        // Add preserved completions comments first
        for comment in completions_comments {
            section.push_str(comment);
            section.push('\n');
        }

        section.push_str(completions);
        section.push('\n');
    }

    section
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_expectations_section() {
        let diagnostics = "// <no-diagnostics-expected>";
        let section = generate_expectations_section(diagnostics, None, None, &[], &[], &[]);

        assert!(section.contains("//----"));
        assert!(section.contains("//- diagnostics"));
        assert!(section.contains("// <no-diagnostics-expected>"));
        assert!(!section.contains("//- on_hover expressions"));
    }

    #[test]
    fn test_generate_expectations_with_hovers() {
        let diagnostics = "// <no-diagnostics-expected>";
        let hovers = "// `Foo` at test.baml:1:1\n// class Foo {}";
        let section = generate_expectations_section(diagnostics, Some(hovers), None, &[], &[], &[]);

        assert!(section.contains("//----"));
        assert!(section.contains("//- diagnostics"));
        assert!(section.contains("//- on_hover expressions"));
        assert!(section.contains("`Foo`"));
    }

    #[test]
    fn test_generate_expectations_with_preserved_comments() {
        let diagnostics = "// Error: something went wrong";
        let diagnostics_comments = vec!["// (expect one error here)".to_string()];
        let section =
            generate_expectations_section(diagnostics, None, None, &diagnostics_comments, &[], &[]);

        assert!(section.contains("//- diagnostics"));
        assert!(section.contains("// (expect one error here)"));
        assert!(section.contains("// Error: something went wrong"));

        // Comment should appear before the actual diagnostics
        let comment_pos = section.find("// (expect one error here)").unwrap();
        let diag_pos = section.find("// Error: something went wrong").unwrap();
        assert!(comment_pos < diag_pos);
    }

    #[test]
    fn test_generate_expectations_with_hovers_comments() {
        let diagnostics = "// <no-diagnostics-expected>";
        let hovers = "// `Foo` at test.baml:1";
        let hovers_comments = vec!["// (testing hover on Foo class)".to_string()];
        let section = generate_expectations_section(
            diagnostics,
            Some(hovers),
            None,
            &[],
            &hovers_comments,
            &[],
        );

        assert!(section.contains("//- on_hover expressions"));
        assert!(section.contains("// (testing hover on Foo class)"));
        assert!(section.contains("// `Foo` at test.baml:1"));

        // Comment should appear before the actual hovers
        let comment_pos = section.find("// (testing hover on Foo class)").unwrap();
        let hover_pos = section.find("// `Foo` at test.baml:1").unwrap();
        assert!(comment_pos < hover_pos);
    }

    #[test]
    fn test_generate_expectations_with_completions() {
        let diagnostics = "// <no-diagnostics-expected>";
        let completions = "// SHOULD_CONTAIN: function, class, enum";
        let section =
            generate_expectations_section(diagnostics, None, Some(completions), &[], &[], &[]);

        assert!(section.contains("//- completions"));
        assert!(section.contains("// SHOULD_CONTAIN: function, class, enum"));
    }
}
