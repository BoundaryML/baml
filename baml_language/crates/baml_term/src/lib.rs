//! Terminal output conventions shared by every BAML CLI surface: the
//! `baml` wrapper, `baml-cli`, and the packed-binary host.
//!
//! Lives in its own tiny crate because the surfaces can't share anything
//! heavier: the wrapper must stay a small static binary with no
//! compiler/engine dependencies, and the pack host can't depend on
//! `baml_cli`. Everything here is presentation-only: the process-wide
//! color decision, clap help styling, and the `error:` / `warning:`
//! line printers.

// The printers write styled diagnostics to stderr — that's this crate's
// whole job. The workspace clippy ban exists to flag stray dev-prints
// elsewhere.
#![allow(clippy::print_stderr)]

use console::Style;

// ── Output mode (color) ─────────────────────────────────────────────────────────

/// When to emit color and hyperlinks. Mirrors the conventional `--color` flag.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Color on an interactive terminal; off when piped or inside an AI agent.
    #[default]
    Auto,
    /// Always emit color/hyperlinks.
    Always,
    /// Never emit color/hyperlinks.
    Never,
}

/// Environment variables that signal a non-interactive AI agent is capturing
/// output, where ANSI color and hyperlinks are noise rather than UI. Sourced
/// from each agent's docs and from maintained detection matrices
/// (`@vercel/detect-agent`, Bun's agent checks).
const AGENT_ENV_VARS: &[&str] = &[
    "CLAUDECODE",      // Claude Code (code.claude.com/docs/en/env-vars)
    "CODEX_SANDBOX",   // OpenAI Codex CLI (set in its sandbox)
    "PI_CODING_AGENT", // Pi (earendil-works/pi)
    "OPENCODE_CLIENT", // opencode
    "AI_AGENT",        // @vercel/detect-agent universal var (+ custom agents)
    "CURSOR_TRACE_ID", // Cursor agent terminal
    "REPL_ID",         // Replit
    "AGENT",           // generic (Codex `AGENT=codex`, Bun)
];

fn env_truthy(var: &str) -> bool {
    std::env::var(var).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Whether output is being captured by a known AI coding agent.
fn running_in_agent() -> bool {
    AGENT_ENV_VARS.iter().any(|var| env_truthy(var))
}

/// Resolve color/hyperlink output once at startup, applying the decision to both
/// stdout and stderr so the two streams agree (avoids leaking codes into a
/// redirected stream or dropping color on a redirected sibling).
pub fn init_color(choice: ColorChoice) {
    match choice {
        ColorChoice::Always => {
            console::set_colors_enabled(true);
            console::set_colors_enabled_stderr(true);
        }
        ColorChoice::Never => {
            console::set_colors_enabled(false);
            console::set_colors_enabled_stderr(false);
        }
        ColorChoice::Auto => {
            // An explicit `CLICOLOR_FORCE` (honored by console's defaults) always
            // wins; only suppress for agents when color was not force-requested.
            if !env_truthy("CLICOLOR_FORCE") && running_in_agent() {
                console::set_colors_enabled(false);
                console::set_colors_enabled_stderr(false);
            }
            // Otherwise leave console's per-stream TTY / NO_COLOR / CLICOLOR defaults.
        }
    }
}

// ── Clap help styling ───────────────────────────────────────────────────────────

/// Clap styling shared by every BAML CLI surface, so `baml toolchain --help`,
/// `baml check --help`, and a packed binary's `--help` all render alike.
pub const CLAP_STYLING: clap::builder::styling::Styles = {
    use clap::builder::styling::{AnsiColor, Color, Effects, RgbColor, Style, Styles};
    const PURPLE: Color = Color::Rgb(RgbColor(0xA8, 0x55, 0xF7));
    // Tonal pair — same hue family, pale (Tailwind purple-200). Used on
    // `<placeholders>` so they read as quiet secondary text against the
    // bold primary purple without washing out into background gray.
    const PURPLE_LIGHT: Color = Color::Rgb(RgbColor(0xE9, 0xD5, 0xFF));
    Styles::styled()
        .header(Style::new().fg_color(Some(PURPLE)).effects(Effects::BOLD))
        .usage(Style::new().fg_color(Some(PURPLE)).effects(Effects::BOLD))
        .literal(Style::new().effects(Effects::BOLD))
        .placeholder(Style::new().fg_color(Some(PURPLE_LIGHT)))
        .error(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Red)))
                .effects(Effects::BOLD),
        )
        .valid(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Green)))
                .effects(Effects::BOLD),
        )
        .invalid(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Yellow)))
                .effects(Effects::BOLD),
        )
};

// ── error: / warning: line printers ─────────────────────────────────────────────

/// Bold-red `error` keyword, matching ariadne's lowercase custom-kind
/// header. `for_stderr` gates the styling on stderr (where the printers
/// write) rather than console's stdout default, so piping stdout alone
/// doesn't strip color and piping stderr through `grep` stays plain.
fn error_prefix() -> impl std::fmt::Display {
    Style::new().red().bold().for_stderr().apply_to("error")
}

/// Bold-yellow `warning` keyword, matching ariadne's lowercase
/// custom-kind header.
fn warning_prefix() -> impl std::fmt::Display {
    Style::new()
        .yellow()
        .bold()
        .for_stderr()
        .apply_to("warning")
}

/// Print a single-line error with the ariadne-matching bold-red
/// `error:` header. Routes to stderr.
pub fn print_error(msg: impl std::fmt::Display) {
    eprintln!("{}: {msg}", error_prefix());
}

/// Print a one-line warning with the ariadne-matching bold-yellow
/// `warning:` header. Routes to stderr.
pub fn print_warning(msg: impl std::fmt::Display) {
    eprintln!("{}: {msg}", warning_prefix());
}

/// Print an [`anyhow::Error`] with the ariadne-matching header, walking
/// the cause chain so each tier is visible — matches anyhow's default
/// Debug format but with the bold-red header so it lines up visually
/// with the ariadne source-snippet blocks the compiler emits.
pub fn print_anyhow_error(err: &anyhow::Error) {
    eprintln!("{}: {err}", error_prefix());
    let causes: Vec<_> = err.chain().skip(1).collect();
    if !causes.is_empty() {
        eprintln!();
        eprintln!("Caused by:");
        for (i, cause) in causes.iter().enumerate() {
            eprintln!("    {i}: {cause}");
        }
    }
}
