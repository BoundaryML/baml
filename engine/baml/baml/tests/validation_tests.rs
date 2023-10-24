mod panic_with_diff;

use baml::{SourceFile, ValidatedSchema};
use internal_baml_core::internal_baml_schema_ast::parse_schema;
use std::{
    fs,
    io::Write as _,
    path::{self, PathBuf},
    sync::Arc,
};

const TESTS_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/validation_files");

/// Parse and analyze a Prisma schema, returning Err if there are any diagnostics (warnings or errors).
fn parse_schema_fail_on_diagnostics(
    file: impl Into<SourceFile>,
) -> Result<ValidatedSchema, String> {
    let path = PathBuf::from("./unknown");
    let file = file.into();
    let schema = baml::validate(&path, vec![file]);

    match (schema.diagnostics.warnings(), schema.diagnostics.errors()) {
        ([], []) => Ok(schema),
        (warnings, errors) => {
            let mut message: Vec<u8> = Vec::new();

            for warn in warnings {
                warn.pretty_print(&mut message)
                    .expect("printing datamodel warning");
            }

            for err in errors {
                err.pretty_print(&mut message)
                    .expect("printing datamodel error");
            }

            Err(String::from_utf8_lossy(&message).into_owned())
        }
    }
}

#[inline(never)] // we want to compile fast
fn run_validation_test(test_file_path: &str) {
    let file_path = path::Path::new(TESTS_ROOT).join(test_file_path);
    let text = fs::read_to_string(&file_path.clone()).unwrap();
    let last_comment_idx = {
        let mut idx = None;
        let newlines = text.char_indices().filter(|(_, c)| *c == '\n');

        for (newline_idx, _) in newlines {
            match (text.get(newline_idx + 1..newline_idx + 3), idx) {
                (Some("//"), None) => {
                    idx = Some(newline_idx + 1); // new comment
                }
                (Some("//"), Some(_)) => (), // comment continues
                (None, _) => (),             // eof
                (Some(_), _) => {
                    idx = None;
                }
            }
        }

        idx
    };
    let last_comment_contents: String = last_comment_idx
        .map(|idx| {
            let mut out = String::with_capacity(text.len() - idx);
            for line in text[idx..].lines() {
                out.push_str(line.trim_start_matches("// "));
                out.push('\n');
            }
            out
        })
        .unwrap_or_default();

    let source_file =
        SourceFile::new_allocated(file_path.clone(), Arc::from(text.into_boxed_str()));

    let validation_result = parse_schema_fail_on_diagnostics(source_file.clone());

    let diagnostics = match (last_comment_contents.is_empty(), validation_result) {
        (true, Ok(_)) => return, // expected and got a valid schema
        (false, Err(diagnostics)) if last_comment_contents == diagnostics => return, // we expected the diagnostics we got
        (_, Err(diagnostics)) => diagnostics,
        (false, Ok(_)) => String::new(), // expected diagnostics, got none
    };

    if std::env::var("UPDATE_EXPECT").is_ok() {
        let mut file = fs::File::create(&file_path).unwrap(); // truncate

        let schema = last_comment_idx
            .map(|idx| &source_file.as_str()[..idx])
            .unwrap_or(source_file.as_str());
        file.write_all(schema.as_bytes()).unwrap();

        for line in diagnostics.lines() {
            writeln!(file, "// {line}").unwrap();
        }
        return;
    }

    panic_with_diff::panic_with_diff(&last_comment_contents, &diagnostics)
}

include!(concat!(env!("OUT_DIR"), "/validation_tests.rs"));
