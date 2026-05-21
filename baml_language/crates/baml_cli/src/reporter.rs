//! Cargo-style status reporter for `baml run` / `baml pack`.
//!
//! Provides a [`Reporter`] that emits cargo-shaped status lines
//! (`   Compiling 552 files`) and an inline spinner during long phases.
//! When stderr isn't a TTY (CI, redirected to file, piped to `less`) the
//! spinner is suppressed and each status update prints as a plain line —
//! `console::style` auto-strips ANSI in that case, so no raw color codes
//! end up in logs.
//!
//! Verb formatting matches cargo: 12-char right-aligned, bold green for
//! normal phases. Diagnostic output is routed through [`Reporter::suspend`]
//! so the spinner pauses cleanly during multi-line ariadne dumps instead
//! of getting interleaved with the source-snippet output.

// Status reporting is intrinsically `print*!` to stderr — that's the
// whole job of this module. The workspace clippy ban exists to flag
// stray dev-prints elsewhere, not the deliberate non-TTY fallbacks
// inside `Reporter::status` / `spin` / `finish` / `warning`.
#![allow(clippy::print_stderr)]

use std::time::{Duration, Instant};

use console::{Color, Style};
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};

/// Brand purple `#A855F7` for the cargo-style verb. Rendered via 24-bit
/// truecolor ANSI (`\x1b[38;2;168;85;247m`) on terminals that support it;
/// `console::Style` strips the escape when stderr isn't a TTY, so piped
/// output stays plain.
const VERB_COLOR: Color = Color::TrueColor(0xA8, 0x55, 0xF7);

fn verb_style() -> Style {
    Style::new().fg(VERB_COLOR).bold()
}

/// Clap help-text styling re-exported from `baml_exec`. Lives there so
/// the packed-binary host — which can't depend on `baml_cli` — gets the
/// same brand-purple look as `baml run`, `baml pack`, etc. Everything
/// in this crate that wants the styled `--help` palette pulls it from
/// here for the shorter import path.
pub use baml_exec::CLAP_STYLING;

/// Cargo-style status reporter.
///
/// Holds either a real `indicatif` spinner (when stderr is a TTY) or
/// nothing (when not), so the same API works in both modes.
pub struct Reporter {
    bar: Option<ProgressBar>,
    started: Instant,
}

impl Reporter {
    /// Construct a reporter that auto-detects whether to render an
    /// animated spinner based on stderr being a TTY.
    pub fn new() -> Self {
        use std::io::IsTerminal;
        let bar = if std::io::stderr().is_terminal() {
            let pb = ProgressBar::new_spinner();
            #[rustfmt::skip]
            pb.set_style(
                ProgressStyle::with_template("{spinner} {wide_msg}")
                    .expect("static template")
                    .tick_strings(&[
                        concat!("            ", "\n",
                                "🐑   🚧   🐑"),
                        concat!("            ", "\n",
                                "     🚧  🐑 "),
                        concat!("            ", "\n",
                                "     🚧 🐑  "),
                        concat!("            ", "\n",
                                "     🚧🐑   "),
                        concat!("     🐑      ", "\n",
                                "     🚧     "),
                        concat!("            ", "\n",
                                "   🐑🚧     "),
                        concat!("            ", "\n",
                                "  🐑 🚧     "),
                        concat!("            ", "\n",
                                " 🐑  🚧     "),
                        // final / resting
                        "",
                    ]),
            );
            // ~150ms/frame -> roughly one full walk-cycle every ~2.7s.
            // Slow enough to read the lamb's position, fast enough to
            // feel alive.
            pb.enable_steady_tick(Duration::from_millis(150));
            Some(pb)
        } else {
            None
        };
        Self {
            bar,
            started: Instant::now(),
        }
    }

    /// Print a one-shot status line that stays in the scrollback —
    /// cargo's `   Compiling foo v0.1.0` shape.
    pub fn status(&self, verb: &str, msg: impl AsRef<str>) {
        let line = format_status(verb, msg.as_ref());
        match &self.bar {
            Some(b) => b.println(line),
            None => eprintln!("{line}"),
        }
    }

    /// Mark a new phase. Persists the phase line to scrollback (cargo
    /// behavior — each `   Verb msg` stays in history once it scrolls
    /// past) *and* updates the lamb spinner's inline message so the
    /// bar reflects the current phase.
    ///
    /// Two formats for the two destinations:
    ///   - `bar.println` gets the cargo-padded version (12-col
    ///     right-aligned verb) so the persistent scrollback lines look
    ///     identical to the piped/non-TTY output.
    ///   - `bar.set_message` gets the inline (no padding) version
    ///     because the 🐑 spinner already provides the visual left
    ///     margin — stacking the 12-col cargo padding on top would put
    ///     6+ blank cells between the lamb and the verb text.
    ///
    /// Non-TTY: single cargo-aligned `eprintln!` (only path; nothing
    /// to update).
    pub fn spin(&self, verb: &str, msg: impl AsRef<str>) {
        let msg = msg.as_ref();
        match &self.bar {
            Some(b) => {
                b.println(format_status(verb, msg));
                b.set_message(format_status_inline(verb, msg));
            }
            None => eprintln!("{}", format_status(verb, msg)),
        }
    }

    /// Stop the spinner and print a final cargo-style "Finished" line
    /// with elapsed wall-clock time.
    ///
    /// Uses `println` + `finish_and_clear` rather than
    /// `finish_with_message` so the Finished line lands in scrollback
    /// the same way every other phase line does — no `{spinner}`
    /// prefix from the bar's template, which would otherwise insert
    /// the final tick string (+ a literal template space) ahead of the
    /// 12-col cargo padding and shift the verb one column right
    /// versus its peers above.
    pub fn finish(&self, verb: &str, msg: impl AsRef<str>) {
        // `{:#}` is `HumanDuration`'s alternate form — the compact
        // `5s` / `2m` / `1h` shape, matching cargo's `Finished` line.
        // The default `{}` form spells out `5 seconds`, which is too
        // wordy for a one-line status.
        let elapsed = HumanDuration(self.started.elapsed());
        let line = format!("{} in {elapsed:#}", format_status(verb, msg.as_ref()));
        match &self.bar {
            Some(b) => {
                b.println(line);
                b.finish_and_clear();
            }
            None => eprintln!("{line}"),
        }
    }

    /// Stop the spinner without writing a status line. Use before
    /// printing an error so the spinner doesn't get left animating.
    pub fn abandon(&self) {
        if let Some(b) = &self.bar {
            b.finish_and_clear();
        }
    }

    /// Run `f` with the spinner paused so multi-line output (ariadne
    /// diagnostic blocks, panic backtraces) can render cleanly. Returns
    /// whatever `f` returns. A no-op when no spinner is active.
    pub fn suspend<R>(&self, f: impl FnOnce() -> R) -> R {
        match &self.bar {
            Some(b) => b.suspend(f),
            None => f(),
        }
    }
}

impl Default for Reporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a single status line as cargo does it: 12-char right-aligned
/// bold purple verb (BAML brand `#A855F7`), then a space, then the
/// payload. Padding is computed on the *unstyled* verb so ANSI escape
/// bytes don't blow up column counts. `console::Style` strips the escape
/// when stderr isn't a TTY, so piped output stays plain.
fn format_status(verb: &str, msg: &str) -> String {
    let padded = format!("{verb:>12}");
    let styled = verb_style().apply_to(padded);
    format!("{styled} {msg}")
}

/// Inline (no-padding) verb formatting for spinner messages. Used when
/// the 🐑 spinner already provides a visual left margin, so the cargo
/// 12-col verb pad would just stack extra blank cells between the lamb
/// and the verb text.
fn format_status_inline(verb: &str, msg: &str) -> String {
    let styled = verb_style().apply_to(verb);
    format!("{styled} {msg}")
}

/// Re-exports of the shared `error:` / `warning:` printers (defined in
/// `baml_exec::diag_print`). Lives in this module so callers across
/// `baml_cli` keep importing `crate::reporter::print_error` etc., while
/// the pack host — which can't depend on `baml_cli` — pulls the same
/// functions from `baml_exec` directly.
pub use baml_exec::{print_anyhow_error, print_error, print_warning};

impl Reporter {
    /// Print an error through this reporter, abandoning the spinner
    /// first so multi-line error output doesn't interleave with ticks.
    pub fn error(&self, msg: impl std::fmt::Display) {
        self.abandon();
        print_error(msg);
    }

    /// Print a warning that scrolls into the buffer *above* the active
    /// spinner. Unlike [`Reporter::error`], the spinner keeps running
    /// after the warning — warnings are advisories, not failures.
    /// Falls back to a plain stderr write when no spinner is active.
    /// Mirrors the formatting of [`print_warning`].
    pub fn warning(&self, msg: impl std::fmt::Display) {
        let line = format!(
            "{}: {msg}",
            Style::new().yellow().bold().apply_to("warning")
        );
        match &self.bar {
            Some(b) => b.println(line),
            #[allow(clippy::print_stderr)]
            None => eprintln!("{line}"),
        }
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
}
