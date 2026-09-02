//! Unified CLI output policy.
//!
//! A preset produces a concrete output policy. Explicit CLI flags and their
//! environment-variable equivalents override individual fields afterward.

use std::{io::IsTerminal, sync::RwLock};

use baml_db::baml_compiler_diagnostics::render::{DiagnosticFormat, RenderConfig};
use clap::{Args, ValueEnum};

#[derive(Args, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OutputArgs {
    #[arg(
        long = "output-preset",
        env = "BAML_OUTPUT_PRESET",
        value_enum,
        value_name = "PRESET",
        help = "Select output defaults [default: auto] [possible values: auto, human, agent]",
        hide_default_value = true,
        hide_env = true,
        hide_possible_values = true,
        default_value_t = OutputPreset::Auto,
        global = true,
        help_heading = "Global options",
        display_order = 70
    )]
    pub preset: OutputPreset,

    #[arg(
        long,
        env = "BAML_COLOR",
        value_enum,
        value_name = "WHEN",
        help = "Control ANSI colors [possible values: auto, always, never]",
        hide_env = true,
        hide_possible_values = true,
        global = true,
        help_heading = "Global options",
        display_order = 30
    )]
    pub color: Option<ColorChoice>,

    #[arg(
        long,
        global = true,
        help = "Disable progress output",
        help_heading = "Global options",
        display_order = 40
    )]
    pub no_progress: bool,

    #[arg(
        long,
        env = "BAML_HYPERLINKS",
        value_enum,
        value_name = "WHEN",
        help = "Control terminal hyperlinks [possible values: auto, always, never]",
        hide_env = true,
        hide_possible_values = true,
        global = true,
        help_heading = "Global options",
        display_order = 80
    )]
    pub hyperlinks: Option<HyperlinkChoice>,

    #[arg(
        long = "diagnostic-format",
        env = "BAML_DIAGNOSTIC_FORMAT",
        value_enum,
        value_name = "FORMAT",
        help = "Select the diagnostic format [possible values: human, agent, concise]",
        hide_env = true,
        hide_possible_values = true,
        global = true,
        help_heading = "Global options",
        display_order = 90
    )]
    pub diagnostic_format: Option<DiagnosticFormatChoice>,

    #[arg(
        long = "agent-skill-check",
        env = "BAML_AGENT_SKILL_CHECK",
        value_enum,
        value_name = "MODE",
        help = "Control BAML agent skill validation [default: auto] [possible values: auto, require, warn, off]",
        hide_default_value = true,
        hide_env = true,
        hide_possible_values = true,
        default_value_t = AgentSkillCheckChoice::Auto,
        global = true,
        help_heading = "Global options",
        display_order = 95
    )]
    pub agent_skill_check: AgentSkillCheckChoice,
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

#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentSkillCheckChoice {
    /// Require a matching skill for agents; warn for humans.
    #[default]
    Auto,
    /// Fail when the matching skill is missing or outdated.
    Require,
    /// Warn when the matching skill is missing or outdated.
    Warn,
    /// Skip agent skill validation.
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentSkillCheckPolicy {
    Require,
    Warn,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputPolicy {
    pub stdout: StreamPolicy,
    pub stderr: StreamPolicy,
    pub diagnostics: DiagnosticPolicy,
    pub progress: bool,
    pub agent_skill_check: AgentSkillCheckPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StreamPolicy {
    pub color: bool,
    pub hyperlinks: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticPolicy {
    pub format: DiagnosticFormat,
    pub show_error_codes: bool,
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

const DEFAULT_POLICY: OutputPolicy = OutputPolicy {
    stdout: StreamPolicy {
        color: false,
        hyperlinks: false,
    },
    stderr: StreamPolicy {
        color: false,
        hyperlinks: false,
    },
    diagnostics: DiagnosticPolicy {
        format: DiagnosticFormat::Human,
        show_error_codes: true,
    },
    progress: true,
    agent_skill_check: AgentSkillCheckPolicy::Warn,
};

static OUTPUT_POLICY: RwLock<OutputPolicy> = RwLock::new(DEFAULT_POLICY);

/// Resolve and install the process-wide output policy before a command writes.
pub(crate) fn init(args: OutputArgs) {
    let policy = resolve(args, output_signals());
    console::set_colors_enabled(policy.stdout.color);
    console::set_colors_enabled_stderr(policy.stderr.color);
    *OUTPUT_POLICY
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = policy;
}

pub(crate) fn policy() -> OutputPolicy {
    *OUTPUT_POLICY
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl OutputPolicy {
    pub(crate) fn diagnostic_render_config(self) -> RenderConfig {
        RenderConfig {
            format: self.diagnostics.format,
            color: self.stderr.color,
            show_error_codes: self.diagnostics.show_error_codes,
        }
    }
}

fn resolve(args: OutputArgs, signals: OutputSignals) -> OutputPolicy {
    let preset = match args.preset {
        OutputPreset::Auto if signals.running_in_agent => OutputPreset::Agent,
        OutputPreset::Auto => OutputPreset::Human,
        explicit => explicit,
    };
    let mut policy = preset.policy(signals);

    if let Some(color) = args.color {
        policy.stdout.color = resolve_color(color, signals.stdout_auto_color);
        policy.stderr.color = resolve_color(color, signals.stderr_auto_color);
    }
    if let Some(hyperlinks) = args.hyperlinks {
        policy.stdout.hyperlinks = resolve_hyperlinks(hyperlinks, signals.stdout_is_terminal);
        policy.stderr.hyperlinks = resolve_hyperlinks(hyperlinks, signals.stderr_is_terminal);
    }
    if let Some(format) = args.diagnostic_format {
        policy.diagnostics.format = match format {
            DiagnosticFormatChoice::Human => DiagnosticFormat::Human,
            DiagnosticFormatChoice::Agent => DiagnosticFormat::Agent,
            DiagnosticFormatChoice::Concise => DiagnosticFormat::Concise,
        };
    }
    if args.no_progress {
        policy.progress = false;
    }
    policy.agent_skill_check = match args.agent_skill_check {
        AgentSkillCheckChoice::Auto if signals.running_in_agent => AgentSkillCheckPolicy::Require,
        AgentSkillCheckChoice::Auto | AgentSkillCheckChoice::Warn => AgentSkillCheckPolicy::Warn,
        AgentSkillCheckChoice::Require => AgentSkillCheckPolicy::Require,
        AgentSkillCheckChoice::Off => AgentSkillCheckPolicy::Off,
    };

    policy
}

impl OutputPreset {
    fn policy(self, signals: OutputSignals) -> OutputPolicy {
        match self {
            Self::Agent => OutputPolicy {
                stdout: StreamPolicy {
                    color: signals.color_forced,
                    hyperlinks: false,
                },
                stderr: StreamPolicy {
                    color: signals.color_forced,
                    hyperlinks: false,
                },
                diagnostics: DiagnosticPolicy {
                    format: DiagnosticFormat::Agent,
                    show_error_codes: true,
                },
                progress: false,
                agent_skill_check: AgentSkillCheckPolicy::Warn,
            },
            Self::Human | Self::Auto => OutputPolicy {
                stdout: StreamPolicy {
                    color: signals.stdout_auto_color,
                    hyperlinks: signals.stdout_is_terminal,
                },
                stderr: StreamPolicy {
                    color: signals.stderr_auto_color,
                    hyperlinks: signals.stderr_is_terminal,
                },
                diagnostics: DiagnosticPolicy {
                    format: DiagnosticFormat::Human,
                    show_error_codes: true,
                },
                progress: true,
                agent_skill_check: AgentSkillCheckPolicy::Warn,
            },
        }
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
        running_in_agent: crate::agent_harness::detect().is_some(),
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
            no_progress: false,
            hyperlinks: None,
            diagnostic_format: None,
            agent_skill_check: AgentSkillCheckChoice::Auto,
        }
    }

    #[test]
    fn auto_selects_agent_defaults_when_agent_is_detected() {
        let policy = resolve(
            args(OutputPreset::Auto),
            OutputSignals {
                running_in_agent: true,
                ..INTERACTIVE
            },
        );

        assert!(!policy.stdout.color);
        assert!(!policy.stderr.color);
        assert!(!policy.stdout.hyperlinks);
        assert!(!policy.stderr.hyperlinks);
        assert_eq!(policy.diagnostics.format, DiagnosticFormat::Agent);
        assert!(!policy.progress);
        assert_eq!(policy.agent_skill_check, AgentSkillCheckPolicy::Require);
    }

    #[test]
    fn human_preset_uses_per_stream_terminal_capabilities() {
        let policy = resolve(
            args(OutputPreset::Human),
            OutputSignals {
                stdout_auto_color: false,
                stderr_auto_color: true,
                stdout_is_terminal: false,
                stderr_is_terminal: true,
                ..INTERACTIVE
            },
        );

        assert!(!policy.stdout.color);
        assert!(policy.stderr.color);
        assert!(!policy.stdout.hyperlinks);
        assert!(policy.stderr.hyperlinks);
        assert_eq!(policy.diagnostics.format, DiagnosticFormat::Human);
        assert!(policy.progress);
        assert_eq!(policy.agent_skill_check, AgentSkillCheckPolicy::Warn);
    }

    #[test]
    fn granular_settings_override_agent_preset() {
        let mut output_args = args(OutputPreset::Agent);
        output_args.color = Some(ColorChoice::Always);
        output_args.hyperlinks = Some(HyperlinkChoice::Always);
        output_args.diagnostic_format = Some(DiagnosticFormatChoice::Human);

        let policy = resolve(output_args, INTERACTIVE);

        assert!(policy.stdout.color);
        assert!(policy.stderr.color);
        assert!(policy.stdout.hyperlinks);
        assert!(policy.stderr.hyperlinks);
        assert_eq!(policy.diagnostics.format, DiagnosticFormat::Human);
        assert!(!policy.progress);
    }

    #[test]
    fn no_progress_overrides_human_preset() {
        let mut output_args = args(OutputPreset::Human);
        output_args.no_progress = true;

        let policy = resolve(output_args, INTERACTIVE);

        assert!(!policy.progress);
    }

    #[test]
    fn conventional_force_color_overrides_agent_preset() {
        let policy = resolve(
            args(OutputPreset::Agent),
            OutputSignals {
                color_forced: true,
                ..INTERACTIVE
            },
        );

        assert!(policy.stdout.color);
        assert!(policy.stderr.color);
        assert!(!policy.stdout.hyperlinks);
        assert!(!policy.stderr.hyperlinks);
        assert_eq!(policy.diagnostics.format, DiagnosticFormat::Agent);
    }

    #[test]
    fn explicit_agent_skill_check_overrides_detection() {
        for (choice, running_in_agent, expected) in [
            (
                AgentSkillCheckChoice::Require,
                false,
                AgentSkillCheckPolicy::Require,
            ),
            (
                AgentSkillCheckChoice::Warn,
                true,
                AgentSkillCheckPolicy::Warn,
            ),
            (AgentSkillCheckChoice::Off, true, AgentSkillCheckPolicy::Off),
        ] {
            let mut output_args = args(OutputPreset::Auto);
            output_args.agent_skill_check = choice;
            let policy = resolve(
                output_args,
                OutputSignals {
                    running_in_agent,
                    ..INTERACTIVE
                },
            );

            assert_eq!(policy.agent_skill_check, expected);
        }
    }

    #[test]
    fn explicit_human_output_does_not_disable_agent_skill_requirement() {
        let policy = resolve(
            args(OutputPreset::Human),
            OutputSignals {
                running_in_agent: true,
                ..INTERACTIVE
            },
        );

        assert_eq!(policy.diagnostics.format, DiagnosticFormat::Human);
        assert_eq!(policy.agent_skill_check, AgentSkillCheckPolicy::Require);
    }
}
