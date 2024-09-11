mod panic_with_diff;

use baml_lib::SourceFile;
use internal_baml_core::ir::repr::IntermediateRepr;

use std::sync::Once;

use std::{
    fs,
    io::Write as _,
    path::{self, PathBuf},
    sync::Arc,
};
use strip_ansi_escapes::strip_str;

const TESTS_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/validation_files");

/// Parse and analyze a Prisma schema, returning Err if there are any diagnostics (warnings or errors).
fn parse_schema_fail_on_diagnostics(file: impl Into<SourceFile>) -> Result<(), String> {
    let path = PathBuf::from("./unknown");
    let file = file.into();
    let schema = baml_lib::validate(&path, vec![file]);

    match (schema.diagnostics.warnings(), schema.diagnostics.errors()) {
        ([], []) => {
            match IntermediateRepr::from_parser_database(&schema.db, schema.configuration) {
                Ok(_ir) => Ok(()),
                Err(e) => Err(format!("{:?}", e.context("Error while converting AST to IR (did you mean to add a step to AST validation?)")))
            }
        }
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

static INIT: Once = Once::new();

#[inline(never)] // we want to compile fast
fn run_validation_test(test_file_path: &str) {
    INIT.call_once(|| {
        env_logger::init();
    });

    let file_path = path::Path::new(TESTS_ROOT).join(test_file_path);
    let text = fs::read_to_string(file_path.clone()).unwrap();
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
        SourceFile::new_allocated(test_file_path.into(), Arc::from(text.into_boxed_str()));

    let validation_result = parse_schema_fail_on_diagnostics(source_file.clone());

    let diagnostics = match (last_comment_contents.is_empty(), validation_result) {
        (true, Ok(_)) => return, // expected and got a valid schema
        (false, Err(diagnostics)) => {
            // remove the span source file location since it depends on whos running the test
            let pruned_last_comment_contents = strip_str(String::from_utf8_lossy(
                last_comment_contents.lines().collect::<String>().as_bytes(),
            ));

            let pruned_diagnostics = strip_str(String::from_utf8_lossy(
                diagnostics.lines().collect::<String>().as_bytes(),
            ));

            if pruned_last_comment_contents == pruned_diagnostics {
                return; // we expected the diagnostics we got
            } else {
                diagnostics
            }
        }
        (_, Err(diagnostics)) => diagnostics,
        (false, Ok(_)) => String::new(), // expected diagnostics, got none
    };

    if std::env::var("UPDATE_EXPECT").map_or(false, |s| s == "1") {
        let mut file = fs::File::create(&file_path).unwrap(); // truncate

        let schema = last_comment_idx
            .map(|idx| &source_file.as_str()[..idx])
            .unwrap_or(source_file.as_str())
            .trim();
        file.write_all(schema.as_bytes()).unwrap();

        if !diagnostics.is_empty() {
            file.write_all(b"\n\n").unwrap();

            for line in diagnostics.lines() {
                // remove colored chars
                let line = strip_str(line);
                writeln!(file, "// {line}").unwrap();
            }
        }
        return;
    }

    panic_with_diff::panic_with_diff(&last_comment_contents, &diagnostics)
}

include!(concat!(env!("OUT_DIR"), "/validation_tests.rs"));
