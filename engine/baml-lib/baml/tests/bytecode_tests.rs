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

use baml_compiler::compile;
use baml_lib::{FeatureFlags, SourceFile};
use baml_vm::{BamlVmProgram, Instruction};
use strip_ansi_escapes::strip_str;

#[allow(dead_code)]
fn run_bytecode_test(test_name: &str, content: &str) {
    let (without_expected, expected) = parse_expected_from_comments(content);
    let keep_viz = expected.contains("VIZ_");
    let result = get_bytecode_output(content, keep_viz);

    let actual = result.unwrap_or_else(|e| e);

    if std::env::var("UPDATE_EXPECT").is_ok() {
        update_expected(
            &format!("bytecode_files/{test_name}"),
            &without_expected,
            &actual,
        );
    } else {
        let expected = normalize_output(&expected);
        let actual = normalize_output(&actual);
        compare_output(&expected, &actual, test_name);
    }
}

fn normalize_output(text: &str) -> String {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();

            let mut chars = trimmed.chars().peekable();
            let stripped = if chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    chars.next();
                }
                while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                    chars.next();
                }
                chars.collect::<String>()
            } else {
                trimmed.to_string()
            };

            let mut chars = stripped.trim_start().chars().peekable();
            let stripped = if chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    chars.next();
                }
                while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                    chars.next();
                }
                chars.collect::<String>()
            } else {
                stripped
            };

            if stripped.is_empty() {
                None
            } else {
                Some(stripped)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_viz_instructions(function: &baml_vm::Function) -> Result<baml_vm::Function, String> {
    let len = function.bytecode.instructions.len();
    let mut mapping = vec![None; len];
    let mut kept = Vec::new();

    for (idx, instr) in function.bytecode.instructions.iter().enumerate() {
        if !matches!(instr, Instruction::VizEnter(_) | Instruction::VizExit(_)) {
            mapping[idx] = Some(kept.len());
            kept.push(idx);
        }
    }

    let remap_offset = |orig_idx: usize, offset: isize| -> Result<isize, String> {
        let new_current =
            mapping[orig_idx].ok_or_else(|| format!("No mapping for instruction {orig_idx}"))?;

        let mut target = orig_idx as isize + offset;
        let step = if offset >= 0 { 1 } else { -1 };

        while target >= 0 && (target as usize) < len && mapping[target as usize].is_none() {
            target += step;
        }

        let new_target = mapping
            .get(target as usize)
            .and_then(|m| *m)
            .ok_or_else(|| {
                format!("Unable to remap jump target from {orig_idx} with offset {offset}")
            })?;

        Ok(new_target as isize - new_current as isize)
    };

    let mut instructions = Vec::with_capacity(kept.len());
    let mut source_lines = Vec::with_capacity(kept.len());
    let mut scopes = Vec::with_capacity(kept.len());

    for &orig_idx in &kept {
        let instr = &function.bytecode.instructions[orig_idx];
        let adjusted = match instr {
            Instruction::Jump(offset) => Instruction::Jump(remap_offset(orig_idx, *offset)?),
            Instruction::JumpIfFalse(offset) => {
                Instruction::JumpIfFalse(remap_offset(orig_idx, *offset)?)
            }
            other => *other,
        };

        instructions.push(adjusted);
        source_lines.push(function.bytecode.source_lines[orig_idx]);
        scopes.push(function.bytecode.scopes[orig_idx]);
    }

    let mut stripped = function.clone();
    stripped.bytecode.instructions = instructions;
    stripped.bytecode.source_lines = source_lines;
    stripped.bytecode.scopes = scopes;
    stripped.viz_nodes = Vec::new();
    Ok(stripped)
}

fn get_bytecode_output(content: &str, keep_viz: bool) -> Result<String, String> {
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

    // Compile to bytecode
    match compile(&schema.db) {
        Ok(BamlVmProgram {
            objects, globals, ..
        }) => {
            // Format bytecode output
            let mut output = String::new();

            // Display all objects
            for obj in &objects {
                match obj {
                    baml_vm::Object::Function(func) => {
                        let func = if keep_viz {
                            func.clone()
                        } else {
                            strip_viz_instructions(func)
                                .map_err(|e| format!("Failed to strip viz instructions: {e}"))?
                        };

                        output.push_str(&format!("Function: {}\n", func.name));
                        output.push_str(&baml_vm::debug::display_bytecode(
                            &func,
                            &baml_vm::EvalStack::new(),
                            &objects,
                            &globals,
                            false, // no colors for golden tests
                        ));
                        output.push('\n');
                    }
                    baml_vm::Object::Class(class) => {
                        output.push_str(&format!(
                            "Class: {} with {} fields\n",
                            class.name,
                            class.field_names.len()
                        ));
                    }
                    baml_vm::Object::Enum(enm) => {
                        output.push_str(&format!("Enum {}\n", enm.name));
                    }
                    baml_vm::Object::Instance(instance) => {
                        output
                            .push_str(&format!("Instance with {} fields\n", instance.fields.len()));
                    }
                    baml_vm::Object::Variant(variant) => {
                        output.push_str(&format!(
                            "Variant {} of Enum {}\n",
                            variant.index, variant.enm
                        ));
                    }
                    baml_vm::Object::String(s) => {
                        output.push_str(&format!("String: {s:?}\n"));
                    }
                    baml_vm::Object::Array(arr) => {
                        output.push_str(&format!("Array with {} elements\n", arr.len()));
                    }
                    baml_vm::Object::Future(_) => {
                        output.push_str("Future\n");
                    }
                    baml_vm::Object::Map(index_map) => {
                        output.push_str(&format!("Map with {} elements\n", index_map.len()));
                    }
                    baml_vm::Object::Media(_) => {
                        output.push_str("Media\n");
                    }
                    baml_vm::Object::BamlType(_) => {
                        output.push_str("BamlType\n");
                    }
                }
            }

            Ok(output.trim().to_string())
        }
        Err(e) => Err(format!("Compilation error: {e:?}")),
    }
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
include!(concat!(env!("OUT_DIR"), "/bytecode_tests.rs"));
