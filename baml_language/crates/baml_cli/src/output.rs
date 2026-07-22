//! Unified CLI output policy.
//!
//! A preset supplies defaults for each independent output dial. Explicit CLI
//! flags and their environment-variable equivalents override those defaults.

use std::{
    io::IsTerminal,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

use baml_db::baml_compiler_diagnostics::render::{DiagnosticFormat, RenderConfig};
use clap::{Args, ValueEnum};

#[derive(Args, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[command(next_help_heading = "Output options")]
pub(crate) struct OutputArgs {
    /// Output defaults: auto, human, or agent.
    ///
    /// `auto` selects the agent preset when a known coding-agent environment
    /// is detected and the human preset otherwise.
    #[arg(
        long = "output-preset",
        env = "BAML_OUTPUT_PRESET",
        value_enum,
        default_value_t = OutputPreset::Auto,
        global = true
    )]
    pub preset: OutputPreset,

    /// When to emit ANSI color: auto, always, or never.
    ///
    /// Overrides the selected output preset.
    #[arg(long, env = "BAML_COLOR", value_enum, global = true)]
    pub color: Option<ColorChoice>,

    /// When to emit terminal hyperlinks: auto, always, or never.
    ///
    /// Overrides the selected output preset.
    #[arg(long, env = "BAML_HYPERLINKS", value_enum, global = true)]
    pub hyperlinks: Option<HyperlinkChoice>,

    /// Compiler diagnostic format: human, agent, or concise.
    ///
    /// Overrides the selected output preset.
    #[arg(
        long = "diagnostic-format",
        env = "BAML_DIAGNOSTIC_FORMAT",
        value_enum,
        global = true
    )]
    pub diagnostic_format: Option<DiagnosticFormatChoice>,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputPreset {
    /// Detect a known coding agent; otherwise use human defaults.
    #[default]
    Auto,
    /// Human-readable output with terminal decorations when supported.
    Human,
    /// Compact agent-readable output without terminal decorations.
    Agent,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorChoice {
    /// Enable color independently for each interactive terminal stream.
    Auto,
    /// Always emit ANSI color.
    Always,
    /// Never emit ANSI color.
    Never,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HyperlinkChoice {
    /// Enable hyperlinks independently for each interactive terminal stream.
    Auto,
    /// Always emit terminal hyperlinks.
    Always,
    /// Never emit terminal hyperlinks.
    Never,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticFormatChoice {
    /// Source snippets and labels intended for terminal users.
    Human,
    /// Compact locations and messages intended for coding agents.
    Agent,
    /// One diagnostic per line without secondary locations.
    Concise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OutputConfig {
    stdout_color: bool,
    stderr_color: bool,
    stdout_hyperlinks: bool,
    stderr_hyperlinks: bool,
    diagnostic_format: DiagnosticFormat,
}

#[derive(Clone, Copy)]
struct OutputSignals {
    running_in_agent: bool,
    color_forced: bool,
    stdout_auto_color: bool,
    stderr_auto_color: bool,
    stdout_is_terminal: bool,
    stderr_is_terminal: bool,
}

const HUMAN_FORMAT: u8 = 0;
const AGENT_FORMAT: u8 = 1;
const CONCISE_FORMAT: u8 = 2;

static STDOUT_HYPERLINKS: AtomicBool = AtomicBool::new(false);
static STDERR_HYPERLINKS: AtomicBool = AtomicBool::new(false);
static DIAGNOSTIC_FORMAT: AtomicU8 = AtomicU8::new(HUMAN_FORMAT);

/// Resolve and install the process-wide output policy before a command writes.
pub(crate) fn init(args: OutputArgs) {
    let config = resolve(args, output_signals());
    console::set_colors_enabled(config.stdout_color);
    console::set_colors_enabled_stderr(config.stderr_color);
    STDOUT_HYPERLINKS.store(config.stdout_hyperlinks, Ordering::Relaxed);
    STDERR_HYPERLINKS.store(config.stderr_hyperlinks, Ordering::Relaxed);
    DIAGNOSTIC_FORMAT.store(format_to_u8(config.diagnostic_format), Ordering::Relaxed);
}

pub(crate) fn stdout_hyperlinks_enabled() -> bool {
    STDOUT_HYPERLINKS.load(Ordering::Relaxed)
}

pub(crate) fn stderr_hyperlinks_enabled() -> bool {
    STDERR_HYPERLINKS.load(Ordering::Relaxed)
}

/// Compiler diagnostic configuration derived from the same resolved output
/// policy used by the rest of the CLI.
pub(crate) fn diagnostic_render_config() -> RenderConfig {
    RenderConfig {
        format: match DIAGNOSTIC_FORMAT.load(Ordering::Relaxed) {
            AGENT_FORMAT => DiagnosticFormat::Agent,
            CONCISE_FORMAT => DiagnosticFormat::Concise,
            _ => DiagnosticFormat::Ariadne,
        },
        color: console::colors_enabled_stderr(),
        show_error_codes: true,
    }
}

fn resolve(args: OutputArgs, signals: OutputSignals) -> OutputConfig {
    let preset = match args.preset {
        OutputPreset::Auto if signals.running_in_agent => OutputPreset::Agent,
        OutputPreset::Auto => OutputPreset::Human,
        explicit => explicit,
    };

    let default_color = match preset {
        OutputPreset::Agent if !signals.color_forced => ColorChoice::Never,
        OutputPreset::Agent => ColorChoice::Always,
        OutputPreset::Human | OutputPreset::Auto => ColorChoice::Auto,
    };
    let default_hyperlinks = match preset {
        OutputPreset::Agent => HyperlinkChoice::Never,
        OutputPreset::Human | OutputPreset::Auto => HyperlinkChoice::Auto,
    };
    let default_format = match preset {
        OutputPreset::Agent => DiagnosticFormatChoice::Agent,
        OutputPreset::Human | OutputPreset::Auto => DiagnosticFormatChoice::Human,
    };

    let color = args.color.unwrap_or(default_color);
    let hyperlinks = args.hyperlinks.unwrap_or(default_hyperlinks);
    let diagnostic_format = args.diagnostic_format.unwrap_or(default_format);

    OutputConfig {
        stdout_color: resolve_color(color, signals.stdout_auto_color),
        stderr_color: resolve_color(color, signals.stderr_auto_color),
        stdout_hyperlinks: resolve_hyperlinks(hyperlinks, signals.stdout_is_terminal),
        stderr_hyperlinks: resolve_hyperlinks(hyperlinks, signals.stderr_is_terminal),
        diagnostic_format: match diagnostic_format {
            DiagnosticFormatChoice::Human => DiagnosticFormat::Ariadne,
            DiagnosticFormatChoice::Agent => DiagnosticFormat::Agent,
            DiagnosticFormatChoice::Concise => DiagnosticFormat::Concise,
        },
    }
}

fn resolve_color(choice: ColorChoice, auto: bool) -> bool {
    match choice {
        ColorChoice::Auto => auto,
        ColorChoice::Always => true,
        ColorChoice::Never => false,
    }
}

fn resolve_hyperlinks(choice: HyperlinkChoice, is_terminal: bool) -> bool {
    match choice {
        HyperlinkChoice::Auto => is_terminal,
        HyperlinkChoice::Always => true,
        HyperlinkChoice::Never => false,
    }
}

fn output_signals() -> OutputSignals {
    OutputSignals {
        running_in_agent: running_in_agent(),
        color_forced: env_truthy("CLICOLOR_FORCE"),
        stdout_auto_color: auto_color(&console::Term::stdout()),
        stderr_auto_color: auto_color(&console::Term::stderr()),
        stdout_is_terminal: std::io::stdout().is_terminal(),
        stderr_is_terminal: std::io::stderr().is_terminal(),
    }
}

fn auto_color(term: &console::Term) -> bool {
    (term.features().colors_supported() && !env_equals("CLICOLOR", "0"))
        || env_truthy("CLICOLOR_FORCE")
}

fn env_equals(var: &str, expected: &str) -> bool {
    std::env::var(var).is_ok_and(|value| value == expected)
}

fn env_truthy(var: &str) -> bool {
    std::env::var(var).is_ok_and(|value| !value.is_empty() && value != "0")
}

/// Environment variables that identify a known coding-agent process.
const AGENT_ENV_VARS: &[&str] = &[
    "CLAUDECODE",
    "CODEX_SANDBOX",
    "PI_CODING_AGENT",
    "OPENCODE_CLIENT",
    "AI_AGENT",
    "CURSOR_TRACE_ID",
    "REPL_ID",
    "AGENT",
];

fn running_in_agent() -> bool {
    AGENT_ENV_VARS.iter().any(|var| env_truthy(var))
}

const fn format_to_u8(format: DiagnosticFormat) -> u8 {
    match format {
        DiagnosticFormat::Ariadne => HUMAN_FORMAT,
        DiagnosticFormat::Agent => AGENT_FORMAT,
        DiagnosticFormat::Concise => CONCISE_FORMAT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERACTIVE: OutputSignals = OutputSignals {
        running_in_agent: false,
        color_forced: false,
        stdout_auto_color: true,
        stderr_auto_color: true,
        stdout_is_terminal: true,
        stderr_is_terminal: true,
    };

    fn args(preset: OutputPreset) -> OutputArgs {
        OutputArgs {
            preset,
            color: None,
            hyperlinks: None,
            diagnostic_format: None,
        }
    }

    #[test]
    fn auto_selects_agent_defaults_when_agent_is_detected() {
        let config = resolve(
            args(OutputPreset::Auto),
            OutputSignals {
                running_in_agent: true,
                ..INTERACTIVE
            },
        );

        assert!(!config.stdout_color);
        assert!(!config.stderr_color);
        assert!(!config.stdout_hyperlinks);
        assert!(!config.stderr_hyperlinks);
        assert_eq!(config.diagnostic_format, DiagnosticFormat::Agent);
    }

    #[test]
    fn human_preset_uses_per_stream_terminal_capabilities() {
        let config = resolve(
            args(OutputPreset::Human),
            OutputSignals {
                stdout_auto_color: false,
                stderr_auto_color: true,
                stdout_is_terminal: false,
                stderr_is_terminal: true,
                ..INTERACTIVE
            },
        );

        assert!(!config.stdout_color);
        assert!(config.stderr_color);
        assert!(!config.stdout_hyperlinks);
        assert!(config.stderr_hyperlinks);
        assert_eq!(config.diagnostic_format, DiagnosticFormat::Ariadne);
    }

    #[test]
    fn granular_settings_override_agent_preset() {
        let mut output_args = args(OutputPreset::Agent);
        output_args.color = Some(ColorChoice::Always);
        output_args.hyperlinks = Some(HyperlinkChoice::Always);
        output_args.diagnostic_format = Some(DiagnosticFormatChoice::Human);

        let config = resolve(output_args, INTERACTIVE);

        assert!(config.stdout_color);
        assert!(config.stderr_color);
        assert!(config.stdout_hyperlinks);
        assert!(config.stderr_hyperlinks);
        assert_eq!(config.diagnostic_format, DiagnosticFormat::Ariadne);
    }

    #[test]
    fn conventional_force_color_overrides_agent_preset() {
        let config = resolve(
            args(OutputPreset::Agent),
            OutputSignals {
                color_forced: true,
                ..INTERACTIVE
            },
        );

        assert!(config.stdout_color);
        assert!(config.stderr_color);
        assert!(!config.stdout_hyperlinks);
        assert!(!config.stderr_hyperlinks);
        assert_eq!(config.diagnostic_format, DiagnosticFormat::Agent);
    }
}
