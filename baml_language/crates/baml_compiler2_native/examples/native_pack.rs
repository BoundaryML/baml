//! Developer entry point for the native backend.
//!
//! ```text
//! native_pack <file.baml> <function> <out>   compile and link an executable
//!     [--emit-clif]                          also write <out>.clif
//!     [--emit-asm]                           also write <out>.s
//!     [--checked]                            link the runtime tests link
//! native_pack --coverage <file.baml> <function>
//!                                            report what blocks compilation
//! native_pack --mir <file.baml> <function>   print the optimized MIR of every
//!                                            function in the file
//! ```
//!
//! `--coverage` prints one tab-separated line per blocker
//! (`blocked`, function, construct, detail), or a single `ok` line with the
//! number of reachable functions.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use baml_compiler2_native::{CompileOptions, RuntimeBuild, compile, coverage, link};
use baml_db::{
    ProjectDatabase, SourceRootKind, SourceRootSpec,
    baml_compiler_diagnostics::Severity,
    baml_compiler2_hir::{
        item_data::{file_functions, function_data},
        loc::FunctionLoc,
    },
    baml_compiler2_mir::{OptLevel, lower_function, pretty::display_function},
    collect_diagnostics,
};

/// What the invocation asks for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pack,
    Coverage,
    Mir,
}

struct Arguments {
    mode: Mode,
    emit_clif: bool,
    emit_asm: bool,
    runtime: RuntimeBuild,
    positional: Vec<String>,
}

fn parse_arguments() -> Arguments {
    let mut arguments = Arguments {
        mode: Mode::Pack,
        emit_clif: false,
        emit_asm: false,
        runtime: RuntimeBuild::Lean,
        positional: Vec::new(),
    };
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--coverage" => arguments.mode = Mode::Coverage,
            "--mir" => arguments.mode = Mode::Mir,
            "--emit-clif" => arguments.emit_clif = true,
            "--emit-asm" => arguments.emit_asm = true,
            "--checked" => arguments.runtime = RuntimeBuild::Checked,
            _ => arguments.positional.push(argument),
        }
    }
    arguments
}

/// A database holding the standard library and the one source file.
fn load(path: &Path) -> Result<ProjectDatabase, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let directory = path.parent().unwrap_or(Path::new("."));
    let root = db
        .add_source_root(SourceRootSpec::new(directory, SourceRootKind::Workspace))
        .map_err(|error| format!("cannot open `{}`: {error}", directory.display()))?;
    db.add_or_update_file_in(root, path, &text);

    let errors: Vec<String> = collect_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| matches!(diagnostic.severity, Severity::Error))
        .map(|diagnostic| diagnostic.message)
        .collect();
    if errors.is_empty() {
        Ok(db)
    } else {
        Err(format!(
            "`{}` does not check:\n{}",
            path.display(),
            errors.join("\n")
        ))
    }
}

fn find_function<'db>(db: &'db ProjectDatabase, name: &str) -> Result<FunctionLoc<'db>, String> {
    db.workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).iter().copied())
        .find(|function| function_data(db, *function).name.as_str() == name)
        .ok_or_else(|| format!("no function `{name}`"))
}

fn run(arguments: &Arguments, out: &mut impl Write) -> Result<(), String> {
    let (path, function) = match arguments.positional.as_slice() {
        [path, function] if arguments.mode != Mode::Pack => (path, function),
        [path, function, _] if arguments.mode == Mode::Pack => (path, function),
        _ => {
            return Err(
                "usage: native_pack <file.baml> <function> <out> [--emit-clif] [--emit-asm] [--checked]\n       native_pack --coverage <file.baml> <function>"
                    .to_owned(),
            );
        }
    };
    let started = Instant::now();
    let db = load(Path::new(path))?;
    let entry = find_function(&db, function)?;
    let checked = started.elapsed();

    if arguments.mode == Mode::Mir {
        for file in db.workspace_files() {
            for function in file_functions(&db, file) {
                match lower_function(&db, *function, OptLevel::Two) {
                    Ok(mir) => {
                        let _ = writeln!(out, "{}", display_function(&db, mir));
                    }
                    Err(error) => {
                        let _ = writeln!(out, "lowering failed: {error}");
                    }
                }
            }
        }
        return Ok(());
    }

    if arguments.mode == Mode::Coverage {
        let coverage = coverage(&db, entry);
        if coverage.is_supported() {
            let _ = writeln!(out, "ok\t{}", coverage.reachable);
        }
        for blocked in &coverage.blocked {
            for blocker in &blocked.blockers {
                let _ = writeln!(
                    out,
                    "blocked\t{}\t{}\t{}",
                    blocked.function, blocker.construct, blocker.detail
                );
            }
        }
        return Ok(());
    }

    let output = PathBuf::from(&arguments.positional[2]);
    let options = CompileOptions {
        emit_clif: arguments.emit_clif,
        emit_disassembly: arguments.emit_asm,
    };
    let started = Instant::now();
    let compiled = compile(&db, entry, options).map_err(|error| error.to_string())?;
    let compiled_in = started.elapsed();
    for (text, extension) in [(&compiled.clif, "clif"), (&compiled.disassembly, "s")] {
        if let Some(text) = text {
            let mut listing = output.clone().into_os_string();
            listing.push(format!(".{extension}"));
            std::fs::write(&listing, text)
                .map_err(|error| format!("cannot write `{}`: {error}", listing.display()))?;
        }
    }
    let started = Instant::now();
    link(&compiled.object, &output, arguments.runtime).map_err(|error| error.to_string())?;
    let linked_in = started.elapsed();
    let _ = writeln!(
        out,
        "{}: {} functions, {} byte object; check {:.1} ms, MIR and codegen {:.1} ms, link {:.1} ms",
        output.display(),
        compiled.functions,
        compiled.object.len(),
        checked.as_secs_f64() * 1e3,
        compiled_in.as_secs_f64() * 1e3,
        linked_in.as_secs_f64() * 1e3,
    );
    Ok(())
}

fn main() -> ExitCode {
    let arguments = parse_arguments();
    match run(&arguments, &mut std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(std::io::stderr().lock(), "error: {message}");
            ExitCode::FAILURE
        }
    }
}
