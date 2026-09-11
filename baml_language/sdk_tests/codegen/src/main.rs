//! Command-line driver for sdk-test codegen.
//!
//! Generator crates under `sdk_tests/crates/` invoke this from their
//! `setup.sh` / `setup.ps1` before their tests run. It is deliberately a
//! normal binary rather than a build script: `cargo check` and `cargo clippy`
//! only *check* this crate's compiler and sdkgen closure instead of building
//! it, and an ordinary workspace build regenerates nothing.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use sdk_test_codegen::CodegenCtx;

/// A generator's codegen entry point.
type RunAll = fn(&CodegenCtx);

/// The generators this binary drives, and their entry points.
///
/// A generator joins this table when its `build.rs` is deleted and its
/// `setup.sh` starts invoking us instead; until then it is still driven by
/// cargo and naming it here would run its codegen twice.
const GENERATORS: &[(&str, RunAll)] = &[
    ("cpp", sdk_test_codegen::cpp::run_all),
    ("csharp", sdk_test_codegen::csharp::run_all),
    (
        "python_pydantic2",
        sdk_test_codegen::python_pydantic2::run_all,
    ),
];

const USAGE: &str = "\
usage: sdk_test_codegen <command>

commands:
  <generator>...
      Generate every fixture for each named generator, installing the result
      into `sdk_tests/crates/<generator>/`. Run from that crate's setup.sh.

  emit-bytecode --fixture <name> --out <path>
      Compile `sdk_tests/fixtures/<name>` and write its encoded program to
      <path>, which must be absolute. Feeds the C# and Ruby bridge ABI probes.
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sdk_test_codegen: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("emit-bytecode") => emit_bytecode(&args[1..]),
        None | Some("-h" | "--help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(_) => generate(args),
    }
}

/// Run each named generator in turn. Resolving every name before running any
/// of them keeps a typo from leaving the tree half-generated.
fn generate(names: &[String]) -> Result<(), String> {
    let selected = names
        .iter()
        .map(|name| {
            GENERATORS
                .iter()
                .find(|(generator, _)| generator == name)
                .ok_or_else(|| {
                    let known: Vec<&str> =
                        GENERATORS.iter().map(|(generator, _)| *generator).collect();
                    format!(
                        "unknown command or generator `{name}` (generators: {})\n\n{USAGE}",
                        known.join(", ")
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (generator, run_all) in selected {
        let ctx = CodegenCtx::new(&sdk_tests_root(), generator);
        run_all(&ctx);
    }
    Ok(())
}

/// Compile one fixture and write its encoded program to `--out`.
///
/// The C# product workflow and the Ruby bridge loader test both need a real
/// `Program` artifact for a known fixture, without running a generator or
/// installing anything into the fixture's crate.
fn emit_bytecode(args: &[String]) -> Result<(), String> {
    let mut fixture: Option<&str> = None;
    let mut out: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("`{flag}` needs a value\n\n{USAGE}"))?;
        match flag {
            "--fixture" => fixture = Some(value.as_str()),
            "--out" => out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown flag `{other}`\n\n{USAGE}")),
        }
        index += 2;
    }
    let fixture = fixture.ok_or_else(|| format!("`--fixture` is required\n\n{USAGE}"))?;
    let out = out.ok_or_else(|| format!("`--out` is required\n\n{USAGE}"))?;
    // Callers write into their own scratch directory; a relative path would
    // resolve against whatever working directory the setup script happened to
    // be in, silently landing the artifact somewhere the caller never reads.
    if !out.is_absolute() {
        return Err(format!(
            "`--out` must be an absolute path, got `{}`",
            out.display()
        ));
    }

    let loaded = sdk_test_codegen::load_fixture(&sdk_tests_root().join("fixtures"), fixture);
    fs::write(&out, loaded.baml_bytecode)
        .map_err(|error| format!("failed to write bytecode to {}: {error}", out.display()))
}

/// `<workspace>/sdk_tests`, resolved from the compile-time manifest directory
/// so the binary behaves the same from any working directory.
fn sdk_tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| unreachable!("the codegen crate is not inside sdk_tests"))
        .to_path_buf()
}
