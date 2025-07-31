mod panic_with_diff;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use baml_lib::SourceFile;
use strip_ansi_escapes::strip_str;

#[allow(dead_code)]
pub(crate) fn run_bytecode_test(test_name: &str, content: &str) {
    let result = get_bytecode_output(content);
    let (without_expected, expected) = parse_expected_from_comments(content);
    
    let actual = result.unwrap_or_else(|e| format!("error: {}", e));
    
    if std::env::var("UPDATE_EXPECT").is_ok() {
        update_expected(&format!("bytecode_files/{}", test_name), &without_expected, &actual);
    } else {
        compare_output(&expected, &actual, test_name);
    }
}

fn get_bytecode_output(content: &str) -> Result<String, String> {
    // Need to add baml_compiler and baml_vm dependencies to access bytecode generation and display
    // For now, return a placeholder until we can update Cargo.toml
    Err("Bytecode tests require baml_compiler and baml_vm dependencies".to_string())
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
                if line.is_empty() {
                    "//".to_string()
                } else {
                    format!("// {}", line)
                }
            })
            .collect();
        
        format!("{}\n\n{}", content.trim_end(), comment_lines.join("\n"))
    };
    
    fs::write(&test_path, new_content).unwrap_or_else(|e| {
        panic!("Failed to update test file {}: {}", test_path.display(), e);
    });
    
    println!("Updated expected output for test: {}", test_name);
}

fn compare_output(expected: &str, actual: &str, test_name: &str) {
    let expected = strip_str(expected);
    let actual = strip_str(actual);
    
    if expected != actual {
        panic_with_diff::panic_with_diff(
            &expected,
            &actual,
        );
    }
}

// Include the generated test functions from build.rs
include!(concat!(env!("OUT_DIR"), "/bytecode_tests.rs"));