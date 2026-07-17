//! Cargo-style status reporter.
//!
//! Provides a [`Reporter`] that emits cargo-shaped status lines
//! (`   Compiling 552 files`). `console::style` auto-strips ANSI when
//! stderr is not a TTY, so piped output stays plain.
//!
//! Verb formatting matches cargo: 12-char right-aligned, bold green for
//! normal phases.

// Status reporting is intrinsically `print*!` to stderr — that's the
// whole job of this module. The workspace clippy ban exists to flag
// stray dev-prints elsewhere, not the deliberate non-TTY fallbacks
// inside `Reporter::status` / `spin` / `finish` / `warning`.
#![allow(clippy::print_stderr)]

use std::time::Instant;

/// Clap help-text styling re-exported from `baml_term`. Lives there so
/// the `baml` wrapper and the packed-binary host — which can't depend
/// on `baml_cli` — get the same brand-purple look as `baml run`,
/// `baml pack`, etc. Everything in this crate that wants the styled
/// `--help` palette pulls it from here for the shorter import path.
pub use baml_term::CLAP_STYLING;
use baml_term::format_status;
use indicatif::HumanDuration;

/// Cargo-style status reporter.
pub struct Reporter {
    started: Instant,
}

impl Reporter {
    /// Construct a reporter.
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    /// Print a one-shot status line that stays in the scrollback —
    /// cargo's `   Compiling foo v0.1.0` shape.
    pub fn status(&self, verb: &str, msg: impl AsRef<str>) {
        let line = format_status(verb, msg.as_ref());
        eprintln!("{line}");
    }

    /// Mark a new phase with a persistent cargo-style line.
    pub fn spin(&self, verb: &str, msg: impl AsRef<str>) {
        eprintln!("{}", format_status(verb, msg.as_ref()));
    }

    /// Print a final cargo-style "Finished" line with elapsed wall-clock time.
    pub fn finish(&self, verb: &str, msg: impl AsRef<str>) {
        // `{:#}` is `HumanDuration`'s alternate form — the compact
        // `5s` / `2m` / `1h` shape, matching cargo's `Finished` line.
        // The default `{}` form spells out `5 seconds`, which is too
        // wordy for a one-line status.
        let elapsed = HumanDuration(self.started.elapsed());
        let line = format!("{} in {elapsed:#}", format_status(verb, msg.as_ref()));
        eprintln!("{line}");
    }

    /// Preserve the old call shape for diagnostic/error paths.
    pub fn abandon(&self) {}

    /// Run `f` and return whatever it returns.
    pub fn suspend<R>(&self, f: impl FnOnce() -> R) -> R {
        f()
    }
}

impl Default for Reporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-exports of the shared `error:` / `warning:` printers (defined in
/// `baml_term`). Lives in this module so callers across `baml_cli` keep
/// importing `crate::reporter::print_error` etc., while the `baml`
/// wrapper and the pack host — which can't depend on `baml_cli` — pull
/// the same functions from `baml_term` / `baml_exec`.
pub use baml_term::{print_anyhow_error, print_error, print_warning};

impl Reporter {
    /// Print an error through this reporter.
    pub fn error(&self, msg: impl std::fmt::Display) {
        self.abandon();
        print_error(msg);
    }

    /// Print a warning through this reporter.
    pub fn warning(&self, msg: impl std::fmt::Display) {
        print_warning(msg);
    }
}
