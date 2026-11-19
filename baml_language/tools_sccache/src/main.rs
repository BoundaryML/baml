//! Native `RUSTC_WRAPPER` shim, the compiled counterpart to the
//! `tools/sccache-shim/sccache` shell script, used on Windows.
//!
//! cargo invokes `RUSTC_WRAPPER` once per crate, passing the full rustc command
//! line through to it. A Windows `.cmd`/`.bat` wrapper is run through
//! `cmd.exe`, whose ~8191-character command-line limit some crates exceed
//! (e.g. `windows-sys`, with its enormous `--check-cfg cfg(feature,
//! values(...))` argument), producing "The command line is too long". A real
//! `.exe` is launched by cargo via `CreateProcess` directly (32767-character
//! limit), so routing rustc through this binary avoids the cmd.exe hop and its
//! tighter limit entirely.
//!
//! Its only job, like the shell script, is to map the `BAML_SCCACHE_R2_*`
//! credentials to the `AWS_*` names sccache reads, then hand off to sccache
//! with the same arguments.
//!
//! The binary is itself named `sccache`: the `cc` crate only routes C/C++
//! compiles through `RUSTC_WRAPPER` when the wrapper's file stem is `sccache`,
//! so any other name leaves every `-sys` crate's C build uncached.
#![allow(clippy::disallowed_methods)]

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, exit},
};

/// Finds the real sccache on `PATH`, skipping this shim.
///
/// The lookup cannot be left to `Command::new("sccache")`: on Windows that
/// searches the running executable's directory first and would resolve back to
/// this shim.
fn real_sccache() -> Option<PathBuf> {
    let name = format!("sccache{}", env::consts::EXE_SUFFIX);
    let this = env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(&name))
        .find(|candidate| is_other_file(candidate, this.as_deref()))
}

fn is_other_file(candidate: &Path, this: Option<&Path>) -> bool {
    candidate.is_file() && candidate.canonicalize().ok().as_deref() != this
}

fn main() {
    // sccache reads the R2 credentials under the AWS_* names. We run before any
    // threads are spawned, so the set_var calls are sound.
    for (from, to) in [
        ("BAML_SCCACHE_R2_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID"),
        ("BAML_SCCACHE_R2_SECRET_ACCESS_KEY", "AWS_SECRET_ACCESS_KEY"),
    ] {
        if let Some(value) = env::var_os(from) {
            // SAFETY: single-threaded at this point; no concurrent env access.
            unsafe { env::set_var(to, value) };
        }
    }

    let Some(sccache) = real_sccache() else {
        eprintln!("sccache shim: no sccache found on PATH");
        exit(127);
    };

    // `argv[1..]` is `<path-to-compiler> <args...>`; forward it verbatim to
    // sccache. Use *_os to preserve non-UTF-8 paths/arguments.
    let args = env::args_os().skip(1);

    match Command::new(&sccache).args(args).status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(err) => {
            eprintln!("sccache shim: failed to spawn {}: {err}", sccache.display());
            exit(127);
        }
    }
}
