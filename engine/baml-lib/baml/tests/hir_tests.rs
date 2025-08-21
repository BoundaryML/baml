fn panic_with_diff(expected: &str, found: &str) {
    let chunks = dissimilar::diff(expected, found);
    let diff = format_chunks(chunks);
    panic!(
        r#"
Snapshot comparison failed. Run the test again with UPDATE_EXPECT=1 in the environment to update the snapshot.

===== EXPECTED ====
{expected}
====== FOUND ======
{found}
======= DIFF ======
{diff}
      "#
    );
}

fn format_chunks(chunks: Vec<dissimilar::Chunk<'_>>) -> String {
    let mut buf = String::new();
    for chunk in chunks {
        let formatted = match chunk {
            dissimilar::Chunk::Equal(text) => text.into(),
            dissimilar::Chunk::Delete(text) => format!("\x1b[41m{text}\x1b[0m"),
            dissimilar::Chunk::Insert(text) => format!("\x1b[42m{text}\x1b[0m"),
        };
        buf.push_str(&formatted);
    }
    buf
}

use std::{fs, path::Path, sync::Arc};

use baml_compiler::hir::Hir;
use baml_lib::{FeatureFlags, SourceFile};
use strip_ansi_escapes::strip_str;

#[allow(dead_code)]
fn run_hir_test(test_name: &str, content: &str) {
    let result = get_hir_output(content);
    println!("result: {result:?}");
    let (without_expected, expected) = parse_expected_from_comments(content);

    let actual = result.unwrap_or_else(|e| e);

    if std::env::var("UPDATE_EXPECT").is_ok() {
        update_expected(
            &format!("hir_files/{test_name}"),
            &without_expected,
            &actual,
        );
    } else {
        compare_output(&expected, &actual, test_name);
    }
}

fn get_hir_output(content: &str) -> Result<String, String> {
    let source_file = SourceFile::new_allocated(
        "test.baml".into(),
        Arc::from(content.to_string().into_boxed_str()),
    );
    let schema = baml_lib::validate(
        &std::path::PathBuf::from("./test"),
        vec![source_file],
        FeatureFlags::new(),
    );

    // Check for validation errors first
    if !schema.diagnostics.errors().is_empty() {
        let mut message: Vec<u8> = Vec::new();
        for err in schema.diagnostics.errors() {
            err.pretty_print(&mut message)
                .expect("printing datamodel error");
        }
        return Err(String::from_utf8_lossy(&message).into_owned());
    }

    // Convert AST to HIR and pretty print
    let hir = Hir::from_ast(&schema.db.ast);
    Ok(hir.pretty_print())
}

fn parse_expected_from_comments(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();

    // Find the last block of consecutive comment lines
    let mut last_comment_block = Vec::new();
    let mut in_comment_block = false;
    let mut content_lines = Vec::new();

    for (i, line) in lines.iter().enumerate().rev() {
        if line.trim_start().starts_with("//") {
            if !in_comment_block && i == lines.len() - 1 {
                in_comment_block = true;
            }
            if in_comment_block {
                last_comment_block.push(*line);
            }
        } else if in_comment_block {
            // End of comment block
            content_lines = lines[0..=i].to_vec();
            break;
        }
    }

    if !in_comment_block {
        content_lines = lines.clone();
    }

    last_comment_block.reverse();

    let expected = last_comment_block
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("// ") {
                &trimmed[3..]
            } else if trimmed == "//" {
                ""
            } else {
                &trimmed[2..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let without_expected = content_lines.join("\n");

    (without_expected, expected)
}

fn update_expected(test_name: &str, content: &str, actual: &str) {
    let test_path = Path::new("tests").join(test_name);

    let new_content = if actual.is_empty() {
        content.to_string()
    } else {
        let comment_lines: Vec<String> = actual
            .lines()
            .map(|line| {
                strip_ansi_escapes::strip_str(if line.is_empty() {
                    "//".to_string()
                } else {
                    format!("// {line}")
                })
            })
            .collect();

        format!("{}\n\n{}\n", content.trim_end(), comment_lines.join("\n"))
    };

    fs::write(&test_path, new_content).unwrap_or_else(|e| {
        panic!("Failed to update test file {}: {}", test_path.display(), e);
    });

    println!("Updated expected output for test: {test_name}");
}

fn compare_output(expected: &str, actual: &str, test_name: &str) {
    // Strip ANSI codes and normalize trailing whitespace
    let expected = strip_str(expected)
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let actual = strip_str(actual)
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");

    if expected != actual {
        panic_with_diff(&expected, &actual);
    }
}

// Include the generated test functions from build.rs
include!(concat!(env!("OUT_DIR"), "/hir_tests.rs"));
