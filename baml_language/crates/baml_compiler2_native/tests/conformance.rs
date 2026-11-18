//! Conformance of the standard library's natives: each file under
//! `tests/conformance` holds the cases of one native, as functions taking no
//! arguments. Every case runs on the interpreter, as a native executable,
//! and as that executable collecting on every allocation, and all three must
//! end the same way: the same printed result or the same uncaught throw.
//!
//! `support.baml` holds what the cases share. Values come out of loops there
//! so that they are built at run time, on the heap.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use baml_compiler2_native::{CompileOptions, compile, link};
use baml_db::{
    CheckedWorld, ProjectDatabase,
    baml_compiler2_hir::item_data::{file_functions, function_data},
    testing::{OptLevel, assert_no_diagnostic_errors, setup_test_db},
};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

/// How a case ends: what it prints, or the value it throws uncaught.
type Outcome = Result<String, String>;

/// The text of the thrown value in an uncaught-throw report.
fn thrown(report: &str) -> &str {
    let marker = "uncaught throw: ";
    let start = report
        .find(marker)
        .unwrap_or_else(|| panic!("no uncaught throw in: {report}"));
    report[start + marker.len()..].lines().next().unwrap_or("")
}

async fn interpret(engine: &Arc<BexEngine>, function: &str) -> Outcome {
    engine
        .call_function_bound_args(
            &format!("user.{function}"),
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .map(|value| match value {
            // `baml run` prints nothing for a null result.
            BexExternalValue::Null => String::new(),
            other => other.render_readable(),
        })
        .map_err(|error| thrown(&error.to_string()).to_owned())
}

/// Compile and link `function` as the entry of an executable.
fn build(db: &ProjectDatabase, function: &str, directory: &Path) -> Result<PathBuf, String> {
    let entry = db
        .workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).iter().copied())
        .find(|candidate| function_data(db, *candidate).name.as_str() == function)
        .ok_or_else(|| format!("no function `{function}`"))?;
    let compiled = compile(db, entry, CompileOptions::default())
        .map_err(|error| format!("does not compile: {error}"))?;
    let executable = directory.join(function);
    link(&compiled.object, &executable).map_err(|error| format!("does not link: {error}"))?;
    Ok(executable)
}

/// Run an executable, collecting on every allocation if `stress`.
fn run(executable: &Path, stress: bool) -> Outcome {
    let mut command = Command::new(executable);
    if stress {
        command.env("BAML_GC_STRESS", "1");
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("`{}` does not start: {error}", executable.display()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    match output.status.code() {
        Some(0) if stderr.is_empty() => Ok(stdout.trim_end_matches('\n').to_owned()),
        Some(1) if stdout.is_empty() => Err(thrown(&stderr).to_owned()),
        // Anything else is a failure of the executable itself; report it as
        // an outcome no interpreter run has.
        _ => Ok(format!(
            "ended with {}; stdout: {stdout}; stderr: {stderr}",
            output.status
        )),
    }
}

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

/// The functions `source` declares without parameters: its cases.
fn cases(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter_map(|line| line.strip_prefix("function "))
        .filter_map(|rest| rest.split_once("()").map(|(name, _)| name))
        .collect()
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
    let bytecode = CheckedWorld::collect(&db, db.workspace_root().expect("a workspace root"))
        .expect("the program checks")
        .compile(OptLevel::Two)
        .expect("the program compiles to bytecode");
    let engine = Arc::new(
        BexEngine::new(bytecode, Arc::new(sys_ops::SysOps::native()), Vec::new())
            .expect("the engine starts"),
    );
    let directory = tempfile::tempdir().expect("a temporary directory");

    let mut ran = 0;
    let mut failures = Vec::new();
    for (file, text) in &texts {
        for case in cases(text) {
            ran += 1;
            let interpreted = interpret(&engine, case).await;
            let (native, stressed) = match build(&db, case, directory.path()) {
                Ok(executable) => (run(&executable, false), run(&executable, true)),
                Err(error) => (Ok(error.clone()), Ok(error)),
            };
            if native != interpreted || stressed != interpreted {
                failures.push(format!(
                    "{file}: {case}:\n  interpreter       {interpreted:?}\n  native            {native:?}\n  collecting always {stressed:?}",
                ));
            }
        }
    }
    assert!(ran > 0, "no conformance cases under `{}`", root.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
