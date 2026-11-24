//! What the test suites share: running a function on the interpreter and
//! as a native executable.
#![allow(
    dead_code,
    reason = "each suite uses its part of what the suites share"
)]

use std::{
    fmt::Write as _,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use baml_compiler2_native::{CompileOptions, RuntimeBuild, compile, link};
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

/// One function as the entry of an executable, linked against the runtime
/// a program ships with and against the one with checks.
pub(crate) struct Built {
    pub(crate) lean: PathBuf,
    checked: PathBuf,
}

/// Compile `function` as the entry of an executable and link it both ways.
pub(crate) fn build(
    db: &ProjectDatabase,
    function: &str,
    directory: &Path,
) -> Result<Built, String> {
    let entry = db
        .workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).iter().copied())
        .find(|candidate| function_data(db, *candidate).name.as_str() == function)
        .ok_or_else(|| format!("no function `{function}`"))?;
    let compiled = compile(db, entry, CompileOptions::default())
        .map_err(|error| format!("does not compile: {error}"))?;
    let built = Built {
        lean: directory.join(function),
        checked: directory.join(format!("{function}.checked")),
    };
    for (executable, runtime) in [
        (&built.lean, RuntimeBuild::Lean),
        (&built.checked, RuntimeBuild::Checked),
    ] {
        link(&compiled.object, executable, runtime)
            .map_err(|error| format!("does not link: {error}"))?;
    }
    Ok(built)
}

/// A way a native executable runs: which runtime it is linked against, and
/// the variables set in its environment.
pub(crate) struct Way {
    pub(crate) name: &'static str,
    checked: bool,
    pub(crate) variables: &'static [&'static str],
}

/// As a program ships, and so collecting on every allocation. Then, on
/// the runtime with checks: with every task that stops going on on another
/// thread and under a worker at another address, and that while collecting
/// on every allocation, where every collection also checks that each
/// compiled frame has a stack map.
pub(crate) const WAYS: [Way; 4] = [
    Way {
        name: "native",
        checked: false,
        variables: &[],
    },
    Way {
        name: "collecting always",
        checked: false,
        variables: &["BAML_GC_STRESS"],
    },
    Way {
        name: "migrating always",
        checked: true,
        variables: &["BAML_MIGRATE_STRESS"],
    },
    Way {
        name: "collecting and migrating",
        checked: true,
        variables: &["BAML_GC_STRESS", "BAML_MIGRATE_STRESS"],
    },
];

/// What starts a program one way: its executable, with the variables of
/// the way set.
pub(crate) fn command(built: &Built, way: &Way) -> Command {
    let mut command = Command::new(if way.checked {
        &built.checked
    } else {
        &built.lean
    });
    for variable in way.variables {
        command.env(variable, "1");
    }
    command
}

/// How long a program may run. No case takes a tenth of it; one that
/// never ends, as a deadlock the runtime does not see, takes all of it.
const LIMIT: Duration = Duration::from_secs(30);

/// Run `command` with nothing to read: what it prints, what it reports and
/// how it exits. One still running at `LIMIT` is killed, and so ends by a
/// signal.
pub(crate) fn finished(command: &mut Command) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("`{:?}` does not start: {error}", command.get_program()));
    let (mut out, mut err) = (child.stdout.take(), child.stderr.take());
    // Both streams are read as they fill, so that neither holds the
    // program up.
    std::thread::scope(|scope| {
        let stdout = scope.spawn(|| {
            let mut bytes = Vec::new();
            let _ = out.as_mut().expect("a pipe").read_to_end(&mut bytes);
            bytes
        });
        let stderr = scope.spawn(|| {
            let mut bytes = Vec::new();
            let _ = err.as_mut().expect("a pipe").read_to_end(&mut bytes);
            bytes
        });
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("a child to wait for") {
                break status;
            }
            if started.elapsed() > LIMIT {
                let _ = child.kill();
                break child.wait().expect("a child to wait for");
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        Output {
            status,
            stdout: stdout.join().expect("the reader does not panic"),
            stderr: stderr.join().expect("the reader does not panic"),
        }
    })
}

/// Run a program one way: what it prints, what it reports and how it
/// exits.
pub(crate) fn output(built: &Built, way: &Way) -> Output {
    finished(&mut command(built, way))
}

/// Run a program one way and say how it ended.
pub(crate) fn run(built: &Built, way: &Way) -> Ending {
    let output = output(built, way);
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

/// A line for each way a case ran and how it ended, for the report of a
/// case that failed.
pub(crate) fn listed(endings: &[(&str, String)]) -> String {
    let mut lines = String::new();
    for (way, ending) in endings {
        let _ = write!(lines, "\n  {way:<24} {ending}");
    }
    lines
}
