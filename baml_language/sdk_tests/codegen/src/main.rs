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

const USAGE: &str = "\
usage: sdk_test_codegen <command>

commands:
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
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
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

    let loaded = sdk_test_codegen::load_fixture(&fixtures_root(), fixture);
    fs::write(&out, loaded.baml_bytecode)
        .map_err(|error| format!("failed to write bytecode to {}: {error}", out.display()))
}

/// `<workspace>/sdk_tests/fixtures`, resolved from the compile-time manifest
/// directory so the binary behaves the same from any working directory.
fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| unreachable!("the codegen crate is not inside sdk_tests"))
        .join("fixtures")
}
