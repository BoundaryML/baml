//! Native `RUSTC_WRAPPER` shim — the compiled counterpart to the
//! `tools/baml-sccache` shell script, used on Windows.
//!
//! cargo invokes `RUSTC_WRAPPER` once per crate, passing the full rustc command
//! line through to it. A Windows `.cmd`/`.bat` wrapper is run through
//! `cmd.exe`, whose ~8191-character command-line limit some crates exceed —
//! e.g. `windows-sys`, with its enormous `--check-cfg cfg(feature,
//! values(...))` argument — producing "The command line is too long". A real
//! `.exe` is launched by cargo via `CreateProcess` directly (32767-character
//! limit), so routing rustc through this binary avoids the cmd.exe hop and its
//! tighter limit entirely.
//!
//! Its only job, like the shell script, is to map the `BAML_SCCACHE_R2_*`
//! credentials to the `AWS_*` names sccache reads, then hand off to sccache
//! with the same arguments.

use std::process::{Command, exit};

fn main() {
    // sccache reads the R2 credentials under the AWS_* names. We run before any
    // threads are spawned, so the set_var calls are sound.
    for (from, to) in [
        ("BAML_SCCACHE_R2_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID"),
        ("BAML_SCCACHE_R2_SECRET_ACCESS_KEY", "AWS_SECRET_ACCESS_KEY"),
    ] {
        if let Some(value) = std::env::var_os(from) {
            // SAFETY: single-threaded at this point; no concurrent env access.
            unsafe { std::env::set_var(to, value) };
        }
    }

    // argv[1..] is `<path-to-rustc> <args...>`; forward it verbatim to sccache.
    // Use *_os to preserve non-UTF-8 paths/arguments.
    let args = std::env::args_os().skip(1);

    match Command::new("sccache").args(args).status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(err) => {
            eprintln!("baml-sccache: failed to spawn sccache: {err}");
            exit(127);
        }
    }
}
