//! What the test suites share: running a function on the interpreter and
//! as a native executable.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use baml_compiler2_native::{CompileOptions, compile, link};
use baml_db::{
    CheckedWorld, ProjectDatabase,
    baml_compiler2_hir::item_data::{file_functions, function_data},
    testing::OptLevel,
};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

/// How a run ends: what it prints, or the value it throws uncaught.
pub(crate) type Ending = Result<String, String>;

/// The text of the thrown value in an uncaught-throw report.
fn thrown(report: &str) -> &str {
    let marker = "uncaught throw: ";
    let start = report
        .find(marker)
        .unwrap_or_else(|| panic!("no uncaught throw in: {report}"));
    report[start + marker.len()..].lines().next().unwrap_or("")
}

/// An interpreter running the program of `db`.
pub(crate) fn engine(db: &ProjectDatabase) -> Arc<BexEngine> {
    let bytecode = CheckedWorld::collect(db, db.workspace_root().expect("a workspace root"))
        .expect("the program checks")
        .compile(OptLevel::Two)
        .expect("the program compiles to bytecode");
    Arc::new(
        BexEngine::new(bytecode, Arc::new(sys_ops::SysOps::native()), Vec::new())
            .expect("the engine starts"),
    )
}

pub(crate) async fn interpret(engine: &Arc<BexEngine>, function: &str) -> Ending {
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
pub(crate) fn build(
    db: &ProjectDatabase,
    function: &str,
    directory: &Path,
) -> Result<PathBuf, String> {
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
pub(crate) fn run(executable: &Path, stress: bool) -> Ending {
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
        // an ending no interpreter run has.
        _ => Ok(format!(
            "ended with {}; stdout: {stdout}; stderr: {stderr}",
            output.status
        )),
    }
}
