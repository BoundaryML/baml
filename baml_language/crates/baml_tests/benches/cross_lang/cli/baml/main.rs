//! cross_lang_baml — wrapper binary that compiles and runs a single
//! `.baml` file. Used by the Suite A2 hyperfine harness so we can time
//! `bex` execution as a subprocess (compile + run), comparable to
//! `python3 foo.py` and `./go_binary`.
//!
//! Usage: cross_lang_baml <path-to-.baml-file>

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use baml_compiler2_emit::{generate_project_bytecode, CompileOptions};
use baml_project::ProjectDatabase;
use bex_engine::{BexEngine, FunctionCallContextBuilder};
use sys_native::{CallId, SysOpsExt};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: cross_lang_baml <path-to-.baml-file>");
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[1]);
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to read {}: {e}", path.display());
            return ExitCode::from(2);
        }
    };

    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db.add_file("bench.baml", &source);
    let bytecode = match generate_project_bytecode(
        &db,
        &CompileOptions { emit_test_cases: false },
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("compile error: {e:?}");
            return ExitCode::from(1);
        }
    };
    let engine = match BexEngine::new(
        bytecode,
        Arc::new(sys_native::SysOps::native()),
        None,
        vec![],
    ) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            eprintln!("engine init error: {e:?}");
            return ExitCode::from(1);
        }
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = rt.block_on(engine.call_function(
        "main",
        vec![],
        FunctionCallContextBuilder::new(CallId::next()).build(),
        true,
    ));

    match result {
        Ok(v) => {
            // Prevent DCE / make the result observable (writes to a sink
            // hyperfine ignores). hyperfine times wall-clock from spawn
            // to exit so this just needs to not be optimized out.
            let _ = format!("{v:?}").len();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("execution error: {e:?}");
            ExitCode::from(1)
        }
    }
}
