//! Conformance of the standard library's natives: each file under
//! `tests/conformance` holds the cases of one native, as functions taking no
//! arguments. Every case runs on the interpreter, as a native executable,
//! and as that executable collecting on every allocation, and all three must
//! end the same way: the same printed result or the same uncaught throw.
//!
//! Native `int` is 64 bits wide and wraps, and the interpreter's is still 63
//! bits wide and panics on overflow. A case whose answer depends on that says
//! how native code ends in a comment on the line before it, `// native
//! prints: <text>` or `// native throws: <text>`, and the interpreter must
//! then end some other way, so that the comment goes when it stops being
//! true.
//!
//! `support.baml` holds what the cases share. Values come out of loops there
//! so that they are built at run time, on the heap.

use std::path::{Path, PathBuf};

use baml_db::testing::{assert_no_diagnostic_errors, setup_test_db};

mod common;
use common::{Ending, build, engine, interpret, run};

/// Every `.baml` file under `directory`, in a stable order.
fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read `{}`: {error}", directory.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            sources(&entry, found);
        } else if entry
            .extension()
            .is_some_and(|extension| extension == "baml")
        {
            found.push(entry);
        }
    }
}

/// The functions `source` declares without parameters: its cases, each with
/// how native code ends where the interpreter is expected to end otherwise.
fn cases(source: &str) -> Vec<(&str, Option<Ending>)> {
    let mut native = None;
    let mut found = Vec::new();
    for line in source.lines() {
        let declared = native.take();
        if let Some(text) = line.strip_prefix("// native prints: ") {
            native = Some(Ok(text.to_owned()));
        } else if let Some(text) = line.strip_prefix("// native throws: ") {
            native = Some(Err(text.to_owned()));
        } else if let Some((name, _)) = line
            .strip_prefix("function ")
            .and_then(|rest| rest.split_once("()"))
        {
            found.push((name, declared));
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread")]
async fn natives_end_as_the_interpreter_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let texts: Vec<(String, String)> = files
        .iter()
        .map(|file| {
            let name = file.strip_prefix(&root).expect("under the root");
            let text = std::fs::read_to_string(file).expect("a readable source");
            (name.display().to_string(), text)
        })
        .collect();
    // One program for every file: a case names its native, so no two
    // collide.
    let program: String = texts.iter().map(|(_, text)| text.as_str()).collect();

    let db = setup_test_db(&program);
    assert_no_diagnostic_errors(&db);
    let engine = engine(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");

    let mut ran = 0;
    let mut failures = Vec::new();
    for (file, text) in &texts {
        for (case, diverging) in cases(text) {
            ran += 1;
            let interpreted = interpret(&engine, case).await;
            let (native, stressed) = match build(&db, case, directory.path()) {
                Ok(executable) => (run(&executable, false), run(&executable, true)),
                Err(error) => (Ok(error.clone()), Ok(error)),
            };
            let agrees = diverging.is_none();
            let expected = diverging.as_ref().unwrap_or(&interpreted);
            if native != *expected || stressed != *expected || (interpreted == *expected) != agrees
            {
                failures.push(format!(
                    "{file}: {case}:\n  interpreter       {interpreted:?}{}\n  native            {native:?}\n  collecting always {stressed:?}",
                    match &diverging {
                        Some(expected) => format!("\n  native expected   {expected:?}"),
                        None => String::new(),
                    },
                ));
            }
        }
    }
    assert!(ran > 0, "no conformance cases under `{}`", root.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
