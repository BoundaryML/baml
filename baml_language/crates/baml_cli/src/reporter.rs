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

use std::{
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
    time::Instant,
};

use console::{Color, Style};
use indicatif::HumanDuration;

/// BAML purple for bold status verbs and section headers.
const VERB_COLOR: Color = Color::TrueColor(0xA8, 0x55, 0xF7);

static QUIET: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicU8 = AtomicU8::new(0);

pub(crate) fn init(quiet: u8, verbose: u8) {
    QUIET.store(quiet > 0, Ordering::Relaxed);
    VERBOSE.store(verbose, Ordering::Relaxed);
}

pub(crate) fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed) > 0
}

pub(crate) fn print_verbose(args: std::fmt::Arguments<'_>) {
    if verbose() && !QUIET.load(Ordering::Relaxed) {
        eprintln!("{args}");
    }
}

pub(crate) fn accent_style() -> Style {
    Style::new().fg(VERB_COLOR).bold()
}

pub(crate) fn secondary_style() -> Style {
    Style::new().dim()
}

/// Clap help-text styling re-exported from `baml_exec`. Lives there so
/// the packed-binary host — which can't depend on `baml_cli` — gets the
/// same brand-purple look as `baml run`, `baml pack`, etc. Everything
/// in this crate that wants the styled `--help` palette pulls it from
/// here for the shorter import path.
pub use baml_exec::CLAP_STYLING;

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
        if QUIET.load(Ordering::Relaxed) {
            return;
        }
        let line = format_status(verb, msg.as_ref());
        eprintln!("{line}");
    }

    /// Mark a new phase with a persistent cargo-style line.
    pub fn spin(&self, verb: &str, msg: impl AsRef<str>) {
        if QUIET.load(Ordering::Relaxed) || !crate::output::policy().progress {
            return;
        }
        eprintln!("{}", format_status(verb, msg.as_ref()));
    }

    /// Print a final cargo-style "Finished" line with elapsed wall-clock time.
    pub fn finish(&self, verb: &str, msg: impl AsRef<str>) {
        if QUIET.load(Ordering::Relaxed) {
            return;
        }
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

/// Format a single status line as cargo does it: 12-char right-aligned
/// bold BAML-purple verb, then a space, then the
/// payload. Padding is computed on the *unstyled* verb so ANSI escape
/// bytes don't blow up column counts. `console::Style` strips the escape
/// when stderr isn't a TTY, so piped output stays plain.
fn format_status(verb: &str, msg: &str) -> String {
    let padded = format!("{verb:>12}");
    let styled = accent_style().apply_to(padded);
    format!("{styled} {msg}")
}

/// Re-exports of the shared `error:` / `warning:` printers (defined in
/// `baml_exec::diag_print`). Lives in this module so callers across
/// `baml_cli` keep importing `crate::reporter::print_error` etc., while
/// the pack host — which can't depend on `baml_cli` — pulls the same
/// functions from `baml_exec` directly.
pub use baml_exec::{print_anyhow_error, print_error, print_warning};

impl Reporter {
    /// Print an error through this reporter.
    pub fn error(&self, msg: impl std::fmt::Display) {
        self.abandon();
        print_error(msg);
    }

    /// Print a fatal error and hand back the terminal exit code, so
    /// command handlers can `return Ok(reporter.fatal(...))` instead of
    /// printing through `eprintln!` and constructing the code by hand.
    #[must_use]
    pub fn fatal(&self, msg: impl std::fmt::Display) -> crate::ExitCode {
        self.error(msg);
        crate::ExitCode::Other
    }

    /// Print a warning. Mirrors the formatting of [`print_warning`].
    pub fn warning(&self, msg: impl std::fmt::Display) {
        let line = format!(
            "{}: {msg}",
            Style::new().yellow().bold().apply_to("warning")
        );
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Status lines pad to cargo's 12-char column, putting the verb
    /// flush against where cargo prints `Compiling`. Counting on unstyled
    /// text keeps ANSI codes from breaking the alignment.
    #[test]
    fn format_status_pads_verb_to_column_12() {
        let line = format_status("Compiling", "foo");
        // Strip ANSI to inspect the underlying spacing.
        let stripped = console::strip_ansi_codes(&line);
        assert!(
            stripped.starts_with("   Compiling foo"),
            "got: {stripped:?}"
        );
        // Sanity: longer verb shifts the leading padding.
        let line = format_status("Loading", "bar");
        let stripped = console::strip_ansi_codes(&line);
        assert!(
            stripped.starts_with("     Loading bar"),
            "got: {stripped:?}"
        );
    }

    #[test]
    fn shared_styles_use_brand_accent_and_default_secondary_text() {
        let accent = accent_style()
            .force_styling(true)
            .apply_to("accent")
            .to_string();
        assert!(accent.contains("\x1b[38;2;168;85;247m"), "{accent:?}");
        assert!(!accent.contains("\x1b[38;5;"), "{accent:?}");

        let secondary = secondary_style()
            .force_styling(true)
            .apply_to("secondary")
            .to_string();
        assert!(secondary.contains("\x1b[2m"), "{secondary:?}");
        assert!(!secondary.contains("\x1b[38;"), "{secondary:?}");
    }
}
