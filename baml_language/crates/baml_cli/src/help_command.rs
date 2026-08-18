use std::{fmt::Write as _, io::Write as _};

use anyhow::{Result, anyhow};
use clap::Args;

use crate::{ExitCode, commands::RuntimeCli};

const DETAILED_HELP_TEMPLATE: &str = "\
{about-with-newline}
{usage-heading} {usage}

{before-help}{all-args}{after-help}";

#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Show help for running functions:
    baml help run

  Show help for running tests:
    baml help test")]
pub(crate) struct HelpArgs {
    /// Command path to document. Omit to show root help.
    #[arg(value_name = "COMMAND")]
    command: Vec<String>,
}

impl HelpArgs {
    pub(crate) fn run(&self, color: bool) -> Result<ExitCode> {
        let rendered = render(&self.command)?;
        let plain = rendered.help.to_string();
        let ansi = rendered.help.ansi().to_string();
        let output = if color { ansi } else { plain };

        // Always write help directly. Interactive pagers can trap coding agents
        // that run commands through a pseudo-terminal and wait for completion.
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(output.as_bytes())?;
        if !output.ends_with('\n') {
            stdout.write_all(b"\n")?;
        }
        Ok(ExitCode::Success)
    }
}

#[derive(Debug)]
struct RenderedHelp {
    help: clap::builder::StyledStr,
}

fn render(query: &[String]) -> Result<RenderedHelp> {
    render_from(query, RuntimeCli::command())
}

fn render_from(query: &[String], mut root: clap::Command) -> Result<RenderedHelp> {
    root.build();

    let command = find_command(query, &root).map_err(|(unmatched, nearest)| {
        let prefix = if query.len() == unmatched.len() {
            "baml".to_string()
        } else {
            format!("baml {}", query[..query.len() - unmatched.len()].join(" "))
        };
        let choices = nearest
            .get_subcommands()
            .filter(|command| !command.is_hide_set() && command.get_name() != "help")
            .map(clap::Command::get_name)
            .collect::<Vec<_>>();
        if choices.is_empty() {
            anyhow!(
                "no command `{}` for `{prefix}`; `{prefix}` has no subcommands",
                unmatched.join(" ")
            )
        } else {
            anyhow!(
                "no command `{}` for `{prefix}`. Available commands:\n    {}",
                unmatched.join(" "),
                choices.join("\n    ")
            )
        }
    })?;

    let is_root = query.is_empty();
    let mut command = command.clone();
    let help = if is_root {
        command.render_help()
    } else {
        if command.get_after_long_help().is_none() {
            command = command.after_long_help("");
        }
        promote_examples(&mut command);
        if command.has_subcommands() {
            let hint = format!(
                "Use `baml help {} <command>` for more information on a specific command.",
                query.join(" ")
            );
            let after = command
                .get_after_long_help()
                .map_or(hint.clone(), |existing| format!("{existing}\n\n{hint}"));
            command = command.after_long_help(after);
        }
        command.render_long_help()
    };

    Ok(RenderedHelp { help })
}

fn promote_examples(command: &mut clap::Command) {
    let Some(after) = command.get_after_long_help().map(ToString::to_string) else {
        return;
    };
    let Some((before, examples, after)) = split_examples(&after) else {
        return;
    };

    let mut styled_examples = clap::builder::StyledStr::new();
    let header = command.get_styles().get_header();
    let literal = command.get_styles().get_literal();
    let mut lines = examples.lines();
    let heading = lines.next().expect("an examples block has a heading");
    write!(styled_examples, "{header}{heading}{header:#}")
        .expect("writing to a string cannot fail");
    for line in lines {
        if let Some(command) = line.strip_prefix("    ") {
            write!(styled_examples, "\n    {command}").expect("writing to a string cannot fail");
        } else if let Some(label) = line.strip_prefix("  ") {
            write!(styled_examples, "\n  {literal}{label}{literal:#}")
                .expect("writing to a string cannot fail");
        } else {
            write!(styled_examples, "\n{line}").expect("writing to a string cannot fail");
        }
    }

    let remaining = [before.trim(), after.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut configured = std::mem::take(command)
        .before_long_help(styled_examples)
        .help_template(DETAILED_HELP_TEMPLATE);
    configured = configured.after_long_help(remaining);
    *command = configured;
}

fn split_examples(help: &str) -> Option<(&str, &str, &str)> {
    let start = if help.starts_with("Examples:\n") {
        0
    } else {
        help.rfind("\n\nExamples:\n")? + 2
    };
    Some((&help[..start], &help[start..], ""))
}

#[cfg(test)]
pub(crate) fn render_for_test(query: &[&str]) -> String {
    render_from(
        &query
            .iter()
            .map(|part| (*part).to_string())
            .collect::<Vec<_>>(),
        RuntimeCli::command_with_internal(false),
    )
    .unwrap()
    .help
    .to_string()
}

fn find_command<'a>(
    query: &'a [String],
    command: &'a clap::Command,
) -> Result<&'a clap::Command, (&'a [String], &'a clap::Command)> {
    let Some(next) = query.first() else {
        return Ok(command);
    };
    let subcommand = command.find_subcommand(next).ok_or((query, command))?;
    find_command(&query[1..], subcommand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_help_is_concise() {
        let help = render(&[]).unwrap().help.to_string();
        assert!(help.contains("Usage: baml [OPTIONS] <COMMAND>"), "{help}");
        assert!(
            !help.contains("Overrides the selected output preset"),
            "{help}"
        );
    }

    #[test]
    fn nested_command_renders_detailed_help() {
        let help = render_for_test(&["test"]);
        assert!(help.contains("SELECTORS:"), "{help}");
    }

    #[test]
    fn concise_and_detailed_help_are_snapshot_tested() {
        let concise = normalize_snapshot(&render_for_test(&[]));
        crate::file_snapshot!("root_concise_help", concise);
        crate::file_snapshot!(
            "describe_detailed_help",
            normalize_snapshot(&render_for_test(&["describe"]))
        );
        crate::file_snapshot!(
            "run_detailed_help",
            normalize_snapshot(&render_for_test(&["run"]))
        );
        crate::file_snapshot!(
            "test_detailed_help",
            normalize_snapshot(&render_for_test(&["test"]))
        );
    }

    fn normalize_snapshot(help: &str) -> String {
        let mut normalized = help
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        if help.ends_with('\n') {
            normalized.push('\n');
        }
        normalized
    }

    #[test]
    fn unknown_nested_command_reports_nearest_choices() {
        let error = render(&["auth".to_string(), "missing".to_string()]).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("no command `missing` for `baml auth`"));
        assert!(message.contains("whoami"));
        assert!(message.contains("logout"));
    }

    #[test]
    fn unknown_child_of_leaf_reports_that_it_has_no_subcommands() {
        let error = render(&["run".to_string(), "missing".to_string()]).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("`baml run` has no subcommands"),
            "{message}"
        );
        assert!(!message.contains("Available commands"), "{message}");
    }

    #[test]
    fn detailed_examples_follow_usage_and_use_clap_styles() {
        let rendered = render(&["run".to_string()]).unwrap().help;
        let plain = rendered.to_string();
        assert!(plain.find("Usage:").unwrap() < plain.find("Examples:").unwrap());
        assert!(plain.find("Examples:").unwrap() < plain.find("Arguments:").unwrap());

        let ansi = rendered.ansi().to_string();
        assert!(ansi.contains("\u{1b}["), "{ansi:?}");
        assert!(
            ansi.contains("  \u{1b}[1mRun a function:\u{1b}[0m\n    baml run main"),
            "{ansi:?}"
        );
        assert_eq!(console::strip_ansi_codes(&ansi), plain);
    }

    #[test]
    fn split_examples_preserves_reference_text_before_examples() {
        let help = "Reference\n\nExamples:\n  Run tests:\n    baml test";
        assert_eq!(
            split_examples(help),
            Some((
                "Reference\n\n",
                "Examples:\n  Run tests:\n    baml test",
                ""
            ))
        );
    }
}
