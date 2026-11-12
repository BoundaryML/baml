//! Formatter cases: every `tests/cases/**/*.baml` is formatted, checked for
//! idempotency, and compared with the `<name>.fmt.snap` beside it.
//!
//! The cases are the formatter's behavioural spec. They were extracted
//! one-to-one from the former in-crate regression tests, grouped by the module
//! they came from, so each directory still reads as the family of rules it
//! pins (`redundant_paren`, `member_chain_layout`, …). A case that needs
//! non-default options starts with `// fmt: line_width=N indent_width=M`; that
//! line is not part of the formatted input.
//!
//! Review a change with `cargo insta test --accept -p baml_fmt --test cases`.

use std::path::Path;

use baml_fmt::{FormatOptions, format};

/// Split an optional `// fmt: key=value …` header off the case text.
fn options(text: &str) -> (FormatOptions, &str) {
    let mut options = FormatOptions::default();
    let Some(rest) = text.strip_prefix("// fmt:") else {
        return (options, text);
    };
    let (header, source) = rest.split_once('\n').unwrap_or((rest, ""));
    for pair in header.split_whitespace() {
        let (key, value) = pair
            .split_once('=')
            .unwrap_or_else(|| panic!("`// fmt:` header entry without `=`: {pair}"));
        let value: usize = value
            .parse()
            .unwrap_or_else(|e| panic!("`// fmt:` header value for {key} is not a number: {e}"));
        match key {
            "line_width" => options.line_width = value,
            "indent_width" => options.indent_width = value,
            other => panic!("unknown `// fmt:` header key: {other}"),
        }
    }
    (options, source)
}

#[test]
fn cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    // Collect every violation so one run reports the full set.
    let mut violations: Vec<String> = Vec::new();
    let mut count = 0usize;

    for entry in walkdir::WalkDir::new(&root).sort_by_file_name() {
        let entry = entry.unwrap_or_else(|e| panic!("walking {}: {e}", root.display()));
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("baml") {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .unwrap_or_else(|_| unreachable!("walkdir yields paths under its root"))
            .display()
            .to_string();
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let (options, source) = options(&text);

        let formatted = match format(source, &options) {
            Ok(formatted) => formatted,
            Err(error) => {
                violations.push(format!("{rel}: formatter failed: {error}"));
                continue;
            }
        };
        match format(&formatted, &options) {
            Ok(second) if second == formatted => {}
            Ok(_) => violations.push(format!("{rel}: formatting is not idempotent")),
            Err(error) => violations.push(format!("{rel}: second pass failed: {error}")),
        }

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| unreachable!("a `.baml` path has a UTF-8 stem"));
        let dir = path
            .parent()
            .unwrap_or_else(|| unreachable!("a case file lives in a directory"));
        insta::with_settings!({
            snapshot_path => dir,
            prepend_module_to_snapshot => false,
            omit_expression => true,
        }, {
            insta::assert_snapshot!(format!("{stem}.fmt"), formatted);
        });
        count += 1;
    }

    assert!(
        violations.is_empty(),
        "{} formatter case violation(s):\n{}",
        violations.len(),
        violations.join("\n")
    );
    assert!(count > 0, "no cases found under {}", root.display());
}
