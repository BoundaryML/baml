//! Parser for shared type serialization test specifications.
//!
//! This module parses markdown files that define test cases for BAML type serialization
//! across multiple target languages (Python, TypeScript, Go).
//!
//! # Format
//!
//! Each test case is defined as an H2 section with:
//! - A ```baml code block for the BAML source
//! - A `### target: \`path\`` line for the target path
//! - Language sections (`### Python`, `### TypeScript`, `### Go`) with labeled bullet points:
//!   - `- Non-streaming: \`type\`` (inline) or followed by a code block
//!   - `- Streaming: \`type\`` (inline) or followed by a code block
//! - Optional `### enum_values:` line for enum tests

/// A single test case parsed from the markdown spec.
#[derive(Debug, Clone)]
pub struct TestCase {
    /// The test name (from the H2 heading)
    pub name: String,
    /// The BAML source code
    pub baml: String,
    /// The target path (e.g., "T.f" or "TypeAlias")
    pub target: String,
    /// Expected Python types: (non_streaming, streaming)
    pub python: Option<(String, String)>,
    /// Expected TypeScript types: (non_streaming, streaming)
    pub typescript: Option<(String, String)>,
    /// Expected Go types: (non_streaming, streaming)
    pub go: Option<(String, String)>,
    /// Expected Rust types: (non_streaming, streaming)
    pub rust: Option<(String, String)>,
    /// For enum tests: the expected values
    pub enum_values: Option<Vec<String>>,
    /// Line number in the markdown file where this test is defined (1-indexed)
    pub line_number: usize,
}

/// Parse the test specification markdown into test cases.
pub fn parse_test_spec(content: &str) -> Vec<TestCase> {
    let mut tests = Vec::new();
    let mut current_test: Option<TestCaseBuilder> = None;
    let mut in_baml_block = false;
    let mut baml_content = String::new();
    let mut current_language: Option<&str> = None;

    // For parsing labeled bullet points with code blocks
    let mut parse_mode = ParseMode::None;
    let mut in_type_block = false;
    let mut type_block_content = String::new();
    let mut pending_non_streaming: Option<String> = None;
    let mut pending_streaming: Option<String> = None;

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Handle BAML code blocks
        if line.trim().starts_with("```baml") {
            in_baml_block = true;
            baml_content.clear();
            i += 1;
            continue;
        }
        if in_baml_block {
            if line.trim() == "```" {
                in_baml_block = false;
                if let Some(ref mut test) = current_test {
                    test.baml = baml_content.trim().to_string();
                }
            } else {
                if !baml_content.is_empty() {
                    baml_content.push('\n');
                }
                baml_content.push_str(line);
            }
            i += 1;
            continue;
        }

        // Handle type code blocks (for long types)
        if in_type_block {
            if line.trim() == "```" {
                in_type_block = false;
                let value = type_block_content.trim().to_string();
                match parse_mode {
                    ParseMode::NonStreaming => pending_non_streaming = Some(value),
                    ParseMode::Streaming => pending_streaming = Some(value),
                    ParseMode::None => {}
                }
                type_block_content.clear();
            } else {
                if !type_block_content.is_empty() {
                    type_block_content.push('\n');
                }
                type_block_content.push_str(line);
            }
            i += 1;
            continue;
        }

        // Check for type code block start (```python, ```typescript, ```go, ```rust, or just ```)
        if current_language.is_some()
            && parse_mode != ParseMode::None
            && (line.trim().starts_with("```python")
                || line.trim().starts_with("```typescript")
                || line.trim().starts_with("```go")
                || line.trim().starts_with("```rust")
                || line.trim() == "```")
        {
            in_type_block = true;
            type_block_content.clear();
            i += 1;
            continue;
        }

        // H2: New test case
        if line.starts_with("## ") && !line.starts_with("### ") {
            // Save pending values before switching tests
            if let Some(ref mut test) = current_test {
                if pending_non_streaming.is_some() || pending_streaming.is_some() {
                    if let (Some(ns), Some(s)) =
                        (pending_non_streaming.take(), pending_streaming.take())
                    {
                        match current_language {
                            Some("python") => test.python = Some((ns, s)),
                            Some("typescript") => test.typescript = Some((ns, s)),
                            Some("go") => test.go = Some((ns, s)),
                            Some("rust") => test.rust = Some((ns, s)),
                            _ => {}
                        }
                    }
                }
            }

            // Save previous test if exists
            if let Some(test) = current_test.take() {
                if let Some(tc) = test.build() {
                    tests.push(tc);
                }
            }
            let name = line[3..].trim().to_string();
            // Line numbers are 1-indexed for user display
            current_test = Some(TestCaseBuilder::new(name, i + 1));
            current_language = None;
            parse_mode = ParseMode::None;
            pending_non_streaming = None;
            pending_streaming = None;
            i += 1;
            continue;
        }

        // H3: Target or language section
        if let Some(section) = line.strip_prefix("### ") {
            // Save pending values before switching sections
            if let Some(ref mut test) = current_test {
                if pending_non_streaming.is_some() || pending_streaming.is_some() {
                    if let (Some(ns), Some(s)) =
                        (pending_non_streaming.take(), pending_streaming.take())
                    {
                        match current_language {
                            Some("python") => test.python = Some((ns, s)),
                            Some("typescript") => test.typescript = Some((ns, s)),
                            Some("go") => test.go = Some((ns, s)),
                            Some("rust") => test.rust = Some((ns, s)),
                            _ => {}
                        }
                    }
                }
            }

            if section.starts_with("target:") {
                if let Some(ref mut test) = current_test {
                    // Extract path from backticks
                    if let Some(path) = extract_backtick_content(section) {
                        test.target = Some(path);
                    }
                }
                current_language = None;
                parse_mode = ParseMode::None;
            } else if let Some(values_part) = section.strip_prefix("enum_values:") {
                if let Some(ref mut test) = current_test {
                    // Parse comma-separated values from backticks
                    let values: Vec<String> = values_part
                        .split(',')
                        .filter_map(|s| extract_backtick_content(s.trim()))
                        .collect();
                    if !values.is_empty() {
                        test.enum_values = Some(values);
                    }
                }
                current_language = None;
                parse_mode = ParseMode::None;
            } else if section == "Python" {
                current_language = Some("python");
                parse_mode = ParseMode::None;
                pending_non_streaming = None;
                pending_streaming = None;
            } else if section == "TypeScript" {
                current_language = Some("typescript");
                parse_mode = ParseMode::None;
                pending_non_streaming = None;
                pending_streaming = None;
            } else if section == "Go" {
                current_language = Some("go");
                parse_mode = ParseMode::None;
                pending_non_streaming = None;
                pending_streaming = None;
            } else if section == "Rust" {
                current_language = Some("rust");
                parse_mode = ParseMode::None;
                pending_non_streaming = None;
                pending_streaming = None;
            } else {
                current_language = None;
                parse_mode = ParseMode::None;
            }
            i += 1;
            continue;
        }

        // Bullet point with type expectations
        if line.trim().starts_with("- ") && current_language.is_some() {
            if let Some(ref mut test) = current_test {
                // Parse labeled format: `- Non-streaming: ...` or `- Streaming: ...`
                if let Some((mode, inline_value)) = parse_labeled_line(line) {
                    parse_mode = mode;
                    if let Some(value) = inline_value {
                        match mode {
                            ParseMode::NonStreaming => pending_non_streaming = Some(value),
                            ParseMode::Streaming => pending_streaming = Some(value),
                            ParseMode::None => {}
                        }
                    }
                    // Check if both are complete
                    if let (Some(ns), Some(s)) = (&pending_non_streaming, &pending_streaming) {
                        match current_language {
                            Some("python") => test.python = Some((ns.clone(), s.clone())),
                            Some("typescript") => test.typescript = Some((ns.clone(), s.clone())),
                            Some("go") => test.go = Some((ns.clone(), s.clone())),
                            Some("rust") => test.rust = Some((ns.clone(), s.clone())),
                            _ => {}
                        }
                    }
                }
            }
        }

        i += 1;
    }

    // Save any pending values for the last test
    if let Some(ref mut test) = current_test {
        if let (Some(ns), Some(s)) = (pending_non_streaming, pending_streaming) {
            match current_language {
                Some("python") => test.python = Some((ns, s)),
                Some("typescript") => test.typescript = Some((ns, s)),
                Some("go") => test.go = Some((ns, s)),
                Some("rust") => test.rust = Some((ns, s)),
                _ => {}
            }
        }
    }

    // Don't forget the last test
    if let Some(test) = current_test {
        if let Some(tc) = test.build() {
            tests.push(tc);
        }
    }

    tests
}

/// Builder for constructing test cases during parsing.
#[derive(Debug)]
struct TestCaseBuilder {
    name: String,
    baml: String,
    target: Option<String>,
    python: Option<(String, String)>,
    typescript: Option<(String, String)>,
    go: Option<(String, String)>,
    rust: Option<(String, String)>,
    enum_values: Option<Vec<String>>,
    line_number: usize,
}

impl TestCaseBuilder {
    fn new(name: String, line_number: usize) -> Self {
        Self {
            name,
            baml: String::new(),
            target: None,
            python: None,
            typescript: None,
            go: None,
            rust: None,
            enum_values: None,
            line_number,
        }
    }

    fn build(self) -> Option<TestCase> {
        let target = self.target?;
        Some(TestCase {
            name: self.name,
            baml: self.baml,
            target,
            python: self.python,
            typescript: self.typescript,
            go: self.go,
            rust: self.rust,
            enum_values: self.enum_values,
            line_number: self.line_number,
        })
    }
}

/// Extract content from within backticks.
fn extract_backtick_content(s: &str) -> Option<String> {
    let start = s.find('`')?;
    let rest = &s[start + 1..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

/// Parsing mode for type expectations.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ParseMode {
    None,
    NonStreaming,
    Streaming,
}

/// Parse a labeled bullet line like `- Non-streaming: \`str\``.
/// Returns (mode, optional_inline_value).
fn parse_labeled_line(line: &str) -> Option<(ParseMode, Option<String>)> {
    let content = line.trim().strip_prefix("- ")?;

    if content.starts_with("Non-streaming:") {
        let rest = content.strip_prefix("Non-streaming:")?.trim();
        let value = if rest.is_empty() {
            None
        } else {
            extract_backtick_content(rest)
        };
        Some((ParseMode::NonStreaming, value))
    } else if content.starts_with("Streaming:") {
        let rest = content.strip_prefix("Streaming:")?.trim();
        let value = if rest.is_empty() {
            None
        } else {
            extract_backtick_content(rest)
        };
        Some((ParseMode::Streaming, value))
    } else {
        None
    }
}

/// The raw test specification markdown content.
pub const TEST_SPEC: &str = include_str!("../../../type_serialization_tests.md");

/// Get all test cases from the embedded specification.
pub fn get_test_cases() -> Vec<TestCase> {
    parse_test_spec(TEST_SPEC)
}

/// Generate Rust test code for a specific language.
/// This is used by build.rs scripts to generate individual test functions.
pub fn generate_test_code(language: &str) -> String {
    let tests = parse_test_spec(TEST_SPEC);
    let mut code = String::new();

    code.push_str("// Auto-generated test code from type_serialization_tests.md\n");
    code.push_str("// Do not edit manually!\n\n");

    let (module_name, _field_accessor, _type_method) = match language {
        "python" => ("type_gen", "python", "serialize_type"),
        "typescript" => ("type_gen", "typescript", "serialize_type"),
        "go" => ("type_gen", "go", "serialize_type"),
        "rust" => ("type_gen", "rust", "serialize_type"),
        _ => panic!("Unknown language: {}", language),
    };

    code.push_str(&format!("#[cfg(test)]\npub mod {} {{\n", module_name));

    for test in &tests {
        // Skip tests that don't have expectations for this language
        let has_type_test = match language {
            "python" => test.python.is_some(),
            "typescript" => test.typescript.is_some(),
            "go" => test.go.is_some(),
            "rust" => test.rust.is_some(),
            _ => false,
        };
        let has_enum_test = test.enum_values.is_some();

        if !has_type_test && !has_enum_test {
            continue;
        }

        // Generate test function
        code.push_str("\n    #[test]\n");
        code.push_str(&format!("    fn {}() {{\n", test.name));

        // Escape the BAML source for use in a raw string
        let baml_escaped = test.baml.replace('\\', "\\\\");

        if has_enum_test {
            // Generate enum test
            let values = test.enum_values.as_ref().unwrap();
            code.push_str(&format!(
                "        crate::test_{}_type!(\n",
                language_short(language)
            ));
            code.push_str(&format!("            r#\"{}\"#,\n", baml_escaped));
            code.push_str(&format!("            \"{}\",\n", test.target));
            code.push_str(&format!("            {},\n", test.line_number));
            code.push_str("            [");
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    code.push_str(", ");
                }
                code.push_str(&format!("\"{}\"", v));
            }
            code.push_str("]\n");
            code.push_str("        );\n");
        } else if has_type_test {
            // Generate type test
            let (non_streaming, streaming) = match language {
                "python" => test.python.as_ref().unwrap(),
                "typescript" => test.typescript.as_ref().unwrap(),
                "go" => test.go.as_ref().unwrap(),
                "rust" => test.rust.as_ref().unwrap(),
                _ => unreachable!(),
            };

            code.push_str(&format!(
                "        crate::test_{}_type!(\n",
                language_short(language)
            ));
            code.push_str(&format!("            r#\"{}\"#,\n", baml_escaped));
            code.push_str(&format!("            \"{}\",\n", test.target));
            code.push_str(&format!("            {},\n", test.line_number));
            // Use raw strings - no need to escape quotes inside r#"..."#
            code.push_str(&format!("            r#\"{}\"#,\n", non_streaming));
            code.push_str(&format!("            r#\"{}\"#\n", streaming));
            code.push_str("        );\n");
        }

        code.push_str("    }\n");
    }

    code.push_str("}\n");
    code
}

fn language_short(language: &str) -> &str {
    match language {
        "python" => "py",
        "typescript" => "ts",
        "go" => "go",
        "rust" => "rs",
        _ => language,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_labeled_inline_format() {
        // New labeled format with inline backticks
        let md = r#"
## string_field

```baml
class T { f string }
```

### target: `T.f`

### Python
- Non-streaming: `str`
- Streaming: `typing.Optional[str]`

### TypeScript
- Non-streaming: `string`
- Streaming: `string | null`
"#;

        let tests = parse_test_spec(md);
        assert_eq!(tests.len(), 1);

        let test = &tests[0];
        assert_eq!(
            test.python,
            Some(("str".to_string(), "typing.Optional[str]".to_string()))
        );
        assert_eq!(
            test.typescript,
            Some(("string".to_string(), "string | null".to_string()))
        );
    }

    #[test]
    fn test_parse_labeled_codeblock_format() {
        // New labeled format with code blocks for long types
        let md = r#"
## complex_type

```baml
class T { f int @check(valid, {{ true }}) @stream.with_state }
```

### target: `T.f`

### Python
- Non-streaming:
```python
Checked[int, typing_extensions.Literal['valid']]
```
- Streaming:
```python
StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['valid']]]]
```
"#;

        let tests = parse_test_spec(md);
        assert_eq!(tests.len(), 1);

        let test = &tests[0];
        assert_eq!(
            test.python,
            Some((
                "Checked[int, typing_extensions.Literal['valid']]".to_string(),
                "StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['valid']]]]".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_enum_case() {
        let md = r#"
## enum_color

```baml
enum Color {
    Red
    Green
    Blue
}
```

### target: `Color`
### enum_values: `Red`, `Green`, `Blue`
"#;

        let tests = parse_test_spec(md);
        assert_eq!(tests.len(), 1);

        let test = &tests[0];
        assert_eq!(test.name, "enum_color");
        assert_eq!(test.target, "Color");
        assert_eq!(
            test.enum_values,
            Some(vec![
                "Red".to_string(),
                "Green".to_string(),
                "Blue".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_multiline_baml() {
        let md = r#"
## class_reference

```baml
class Inner { x int }
class Outer { inner Inner }
```

### target: `Outer.inner`

### Python
- Non-streaming: `Inner`
- Streaming: `typing.Optional["Inner"]`
"#;

        let tests = parse_test_spec(md);
        assert_eq!(tests.len(), 1);
        assert_eq!(
            tests[0].baml,
            "class Inner { x int }\nclass Outer { inner Inner }"
        );
    }

    #[test]
    fn test_parse_full_spec() {
        let tests = get_test_cases();
        // Should have many tests from the full spec
        assert!(
            tests.len() > 30,
            "Expected at least 30 tests, got {}",
            tests.len()
        );

        // Verify a few specific tests exist
        let test_names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
        assert!(test_names.contains(&"string_field"));
        assert!(test_names.contains(&"optional_string"));
        assert!(test_names.contains(&"union_int_string"));
        assert!(test_names.contains(&"enum_color"));
    }
}
