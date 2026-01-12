pub fn prefix_lines(s: &str, prefix: &str) -> String {
    if s.is_empty() {
        return s.to_string();
    }
    s.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format a string as a Python docstring with proper indentation.
/// The first line starts with """, subsequent lines are indented with the given prefix,
/// and the closing """ is on the same line as the last content.
pub fn format_docstring(s: &str, indent: &str) -> String {
    if s.is_empty() {
        return "\"\"\"\"\"\"".to_string();
    }

    let lines: Vec<&str> = s.lines().collect();
    if lines.len() == 1 {
        // Single line docstring
        return format!("\"\"\"{}\"\"\"", lines[0]);
    }

    // Multi-line docstring: indent all lines after the first
    let mut result = String::new();
    result.push_str("\"\"\"");
    result.push_str(lines[0]);
    for line in &lines[1..] {
        result.push('\n');
        result.push_str(indent);
        result.push_str(line);
    }
    result.push_str("\"\"\"");
    result
}
