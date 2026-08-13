// Wires up the BAML CLI subcommands: Run, Describe, Generate, Test,
// Format, and LanguageServer. `baml run` is the top-level entry for
// standalone execution.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, CommandFactory, Parser, Subcommand};

pub(crate) const fn release_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}

#[derive(Parser, Debug)]
#[command(
    name = "baml-cli",
    bin_name = "baml",
    author,
    version = release_version(),
    about = "Build and run BAML projects.",
    long_about = None,
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
#[command(styles = crate::reporter::CLAP_STYLING)]
pub(crate) struct RuntimeCli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(flatten)]
    pub output: crate::output::OutputArgs,

    /// Display concise help for this command.
    #[arg(
        global = true,
        short,
        long,
        action = clap::ArgAction::HelpShort,
        help_heading = "Global options",
        display_order = 100
    )]
    help: Option<bool>,

    /// Print version.
    #[arg(
        short = 'V',
        long = "version",
        action = clap::ArgAction::Version,
        help_heading = "Global options",
        display_order = 110
    )]
    version: Option<bool>,

    /// Specifies a subcommand to run.
    #[command(subcommand)]
    pub(crate) command: Commands,

    /// Name of the invoked top-level subcommand, as registered with clap
    /// (e.g. `"fmt"`, `"lsp"`). Not a CLI argument: it's populated in
    /// [`Self::parse_from_smart`] from the parsed matches so telemetry can
    /// report the exact clap name without a hand-maintained mapping.
    #[arg(skip)]
    pub(crate) invoked_subcommand: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct GlobalArgs {
    /// Suppress nonessential output.
    #[arg(
        short,
        long,
        action = clap::ArgAction::Count,
        global = true,
        help_heading = "Global options",
        display_order = 10
    )]
    pub quiet: u8,

    /// Increase diagnostic verbosity. Repeatable.
    #[arg(
        short,
        long,
        action = clap::ArgAction::Count,
        global = true,
        help_heading = "Global options",
        display_order = 20
    )]
    pub verbose: u8,

    /// Change to this directory before running the command.
    #[arg(
        long,
        value_name = "PATH",
        global = true,
        help_heading = "Global options",
        display_order = 50
    )]
    pub directory: Option<PathBuf>,

    /// Discover the BAML project from this path.
    #[arg(
        long,
        value_name = "PATH",
        global = true,
        help_heading = "Global options",
        display_order = 60
    )]
    pub project: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    // TODO: All other commands are disabled for now as they depend on baml_runtime
    // #[command(about = "Initialize a new BAML project.")]
    // Init(baml_runtime::cli::init::InitArgs),

    // #[command(about = "Runs all generators in the baml_src directory")]
    // Generate(baml_runtime::cli::generate::GenerateArgs),

    // #[command(about = "Checks for errors and warnings in the baml_src directory")]
    // Check(baml_runtime::cli::check::CheckArgs),
    #[command(about = "Check BAML source files for compiler errors")]
    Check(crate::check_command::CheckArgs),

    // #[command(about = "Starts a server that translates LLM responses to BAML responses")]
    // Serve(baml_runtime::cli::serve::ServeArgs),

    // #[command(about = "Starts a development server")]
    // Dev(baml_runtime::cli::dev::DevArgs),
    #[command(
        subcommand,
        about = "Manage authentication",
        long_about = "Manage the identity used by BAML services.\n\nUse `baml auth login` to authenticate, `baml auth whoami` to inspect the current identity, and `baml auth logout` to remove the authenticated session.",
        after_long_help = "Examples:\n  Log in:\n    baml auth login\n\n  Show the current identity:\n    baml auth whoami\n\n  Log out:\n    baml auth logout"
    )]
    Auth(crate::auth::AuthCommands),

    #[command(about = "Report an issue or improvement to Boundary")]
    Feedback(crate::feedback_command::FeedbackArgs),

    #[command(about = "Format BAML source files", name = "fmt")]
    Format(crate::format::FormatArgs),

    // #[command(about = "Run BAML tests")]
    // Test(baml_runtime::cli::testing::TestArgs),

    // #[command(about = "Print HIR from BAML files", hide = true)]
    // DumpHIR(baml_runtime::cli::dump_intermediate::DumpIntermediateArgs),

    // #[command(about = "Print Bytecode from BAML files", hide = true)]
    // DumpBytecode(baml_runtime::cli::dump_intermediate::DumpIntermediateArgs),
    #[command(about = "Describe a BAML symbol", name = "describe")]
    Describe(crate::describe_command::DescribeArgs),

    #[command(about = "Generate client code from BAML definitions")]
    Generate(crate::generate::GenerateArgs),

    #[command(about = "Run BAML tests")]
    Test(crate::test_command::TestArgs),

    #[command(about = "Initialize a new BAML project (creates baml.toml)")]
    Init(crate::init_command::InitArgs),

    #[command(about = "Create a new BAML project directory")]
    New(crate::init_command::NewArgs),

    #[command(about = "Run a BAML function or script")]
    Run(crate::run_command::RunArgs),

    #[command(about = "Open the BAML playground in your browser")]
    Playground(crate::playground_command::PlaygroundArgs),

    #[command(about = "Package a BAML target as a standalone executable")]
    Pack(crate::pack_command::PackArgs),

    #[command(
        about = "Install or manage IDE integration assets",
        after_long_help = "Examples:\n  Install into the detected editor:\n    baml ide install\n\n  Install into Cursor:\n    baml ide install --cursor"
    )]
    Ide(crate::ide_command::IdeArgs),

    #[command(
        about = "Install BAML agent skills for this project",
        after_long_help = "Examples:\n  Install the latest skills:\n    baml agent install\n\n  Install in a specific project:\n    baml agent install --project ./my-project"
    )]
    Agent(crate::agent_command::AgentArgs),

    #[command(about = "Start a language server", name = "lsp")]
    LanguageServer(crate::lsp::LanguageServerArgs),

    #[command(about = "Display documentation for a command")]
    Help(crate::help_command::HelpArgs),

    // Hidden from `baml --help` by default: the first-run notice + the
    // `boundaryml.com/telemetry` docs page cover discovery for users who
    // want to opt out, and hiding keeps the top-level command list from
    // reading like an ops-console. Still fully functional (`baml
    // telemetry`, `baml telemetry disable`, etc.) and re-listed
    // automatically by `parse_from_smart` when `BAML_INTERNAL=1`.
    #[command(about = "Show or change BAML CLI telemetry preferences", hide = true)]
    Telemetry(crate::telemetry_command::TelemetryArgs),

    // The detached telemetry flush child (see `telemetry::queue`). Spawned
    // by the CLI itself on exit / rotation; hidden even from
    // `BAML_INTERNAL=1` listings by the `__` naming convention being
    // self-explanatory, but marked hide for good measure.
    #[command(
        name = "__flush-telemetry",
        about = "(internal) drain the on-disk telemetry queue",
        hide = true
    )]
    FlushTelemetry(crate::telemetry_command::FlushTelemetryArgs),
    // #[command(about = "Start an interactive REPL for BAML expressions", hide = true)]
    // Repl(baml_runtime::cli::repl::ReplArgs),

    // #[command(about = "Optimize prompts using GEPA algorithm")]
    // Optimize(baml_runtime::cli::optimize::OptimizeArgs),
}

impl RuntimeCli {
    pub(crate) fn command() -> clap::Command {
        Self::command_with_internal(baml_internal_env_is_truthy())
    }

    pub(crate) fn command_with_internal(include_internal: bool) -> clap::Command {
        let mut command = <Self as CommandFactory>::command();
        configure_help_hints(&mut command, &[]);

        if include_internal {
            for subcommand in command
                .get_subcommands_mut()
                .filter(|subcommand| subcommand.is_hide_set())
            {
                let mut new_subcommand = std::mem::take(subcommand);
                new_subcommand = new_subcommand.hide(false);
                if let Some(about) = new_subcommand.get_about() {
                    let new_about = format!("(internal-only) {about}");
                    new_subcommand = new_subcommand.about(new_about);
                }
                *subcommand = new_subcommand;
            }
        }

        command
    }

    /// Parse CLI arguments and optionally unhide internal subcommands.
    ///
    /// Parameters:
    /// - `argv`: Raw process argument vector (`argv[0]` program name followed by CLI tokens).
    ///
    /// Returns:
    /// - A fully parsed [`RuntimeCli`] value.
    ///
    /// Errors/Panics:
    /// - Does not return recoverable errors. On parse failures this calls clap's
    ///   `err.exit()` and terminates the process, matching normal CLI behavior.
    /// - Does not panic.
    ///
    /// This should be used for CLI invocations instead of `RuntimeCli::parse_from`.
    pub fn parse_from_smart(argv: Vec<String>) -> Self {
        use clap::FromArgMatches;

        let mut command = RuntimeCli::command();

        // BEP-027 §"`--` separator" — note: clap already prints a
        // helpful "to pass '<flag>' as a value, use '-- <flag>'" tip on
        // `ErrorKind::UnknownArgument`, so we don't need to add our own.
        // The error format ships with `[-- <TARGET_ARGS>...]` in the
        // usage line as further reinforcement.
        let matches = match command.try_get_matches_from_mut(argv) {
            Ok(matches) => matches,
            Err(err) => err.exit(),
        };

        let mut cli = match RuntimeCli::from_arg_matches(&matches) {
            Ok(cli) => cli,
            Err(err) => err.exit(),
        };

        // Record the invoked subcommand's clap name for telemetry, straight
        // from the parsed matches so it always matches what clap registered.
        cli.invoked_subcommand = matches.subcommand_name().map(str::to_string);

        // Preserve whether global test-compatible options were supplied on the
        // real command line. Test profiles are parsed later, after locating the
        // project manifest, and direct scalar values must take precedence.
        if let Commands::Test(test) = &mut cli.command {
            test.cli_output =
                crate::test_command::TestOutputOverrides::from_cli_matches(&matches, cli.output);
            test.cli_logs = matches
                .subcommand_matches("test")
                .filter(|matches| {
                    matches.value_source("logs") == Some(clap::parser::ValueSource::CommandLine)
                })
                .map(|_| test.logs);
        }

        cli
    }

    pub fn run(&self) -> Result<crate::ExitCode> {
        if let Some(directory) = &self.global.directory {
            std::env::set_current_dir(directory).with_context(|| {
                format!("failed to change directory to {}", directory.display())
            })?;
        }
        crate::reporter::init(self.global.quiet, self.global.verbose);

        // The detached telemetry flush child must run before (and without)
        // `record_invocation` below: recording its own invocation would
        // seal a new queue file on drop and spawn another child, forever.
        if let Commands::FlushTelemetry(args) = &self.command {
            return args.run();
        }
        if let Commands::Help(args) = &self.command {
            crate::output::init(self.output);
            return args.run(crate::output::policy().stdout.color);
        }

        // Fire anonymous, best-effort telemetry for this invocation. The
        // event is appended to an on-disk queue (one atomic write); on drop
        // of the guard (after the match below returns) the queue file is
        // sealed and a detached child process delivers it after this
        // process has already exited. It never fails or delays the command.
        let _telemetry = crate::telemetry::record_invocation(
            self.invoked_subcommand.as_deref().unwrap_or("unknown"),
        );

        // Resolve every output dial once, before any subcommand writes.
        crate::output::init(self.output);

        // Passive skill warning + background freshness refresh, only on the
        // core authoring commands (init, run, generate, pack) so the nag
        // never bleeds into machine-facing or utility invocations. The
        // guard's drop, after the match below returns, gives the background
        // refresh the rest of its time budget.
        let _skill_check = match &self.command {
            Commands::Init(_) | Commands::Run(_) | Commands::Generate(_) | Commands::Pack(_) => {
                crate::skill_check::SkillCheck::start()
            }
            _ => crate::skill_check::SkillCheck::skipped(),
        };

        let project = self.global.project.as_deref();

        match &self.command {
            Commands::Init(args) => args.run(),
            Commands::New(args) => args.run(),
            Commands::Check(args) => args.run(project),
            Commands::Run(args) => args.run(project),
            Commands::Playground(args) => args.run(project),
            Commands::Pack(args) => args.run(project),
            Commands::Ide(args) => args.run(),
            Commands::Agent(args) => args.run(project),
            Commands::Describe(args) => args.run(project),
            Commands::Generate(args) => args.run(project),
            Commands::Test(args) => args.run(project),
            Commands::LanguageServer(args) => match args.run() {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(e) => {
                    crate::reporter::print_error(e);
                    Ok(crate::ExitCode::Other)
                }
            },
            Commands::Auth(args) => args.run(),
            Commands::Feedback(args) => args.run(),
            // Handled before telemetry and command-side effects above.
            Commands::Help(args) => args.run(crate::output::policy().stdout.color),
            Commands::Telemetry(args) => args.run(),
            // Handled by the early return above, before telemetry wiring.
            Commands::FlushTelemetry(args) => args.run(),
            Commands::Format(args) => args.run(project),
        }
    }
}

fn configure_help_hints(command: &mut clap::Command, path: &[String]) {
    let is_root = path.is_empty();
    let has_detailed_help = is_root
        || command.get_long_about().is_some()
        || command.get_after_long_help().is_some()
        || command.has_subcommands();

    if command.get_name() != "help" && has_detailed_help {
        let hint = if is_root {
            "Use `baml help <command>` for more details.".to_string()
        } else {
            format!("Use `baml help {}` for more details.", path.join(" "))
        };
        let mut configured = std::mem::take(command);
        configured = configured.after_help(hint);
        *command = configured;
    }

    for subcommand in command.get_subcommands_mut() {
        let mut child_path = path.to_vec();
        child_path.push(subcommand.get_name().to_string());
        configure_help_hints(subcommand, &child_path);
    }
}

fn baml_internal_env_is_truthy() -> bool {
    std::env::var("BAML_INTERNAL")
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBLIC_COMMAND_PATHS: &[&[&str]] = &[
        &[],
        &["check"],
        &["auth"],
        &["auth", "login"],
        &["auth", "whoami"],
        &["auth", "logout"],
        &["feedback"],
        &["fmt"],
        &["describe"],
        &["generate"],
        &["test"],
        &["init"],
        &["new"],
        &["run"],
        &["playground"],
        &["pack"],
        &["ide"],
        &["ide", "install"],
        &["agent"],
        &["agent", "install"],
        &["lsp"],
        &["help"],
    ];

    #[test]
    fn release_version_matches_compile_time_setting() {
        assert_eq!(release_version(), baml_version::CANONICAL_VERSION);
    }

    /// `parse_from_smart` records the invoked subcommand's clap name for
    /// telemetry, straight from the parsed matches.
    #[test]
    fn parse_records_invoked_subcommand_name() {
        let cli = RuntimeCli::parse_from_smart(vec!["baml-cli".into(), "fmt".into()]);
        assert_eq!(cli.invoked_subcommand.as_deref(), Some("fmt"));

        let cli = RuntimeCli::parse_from_smart(vec!["baml-cli".into(), "lsp".into()]);
        assert_eq!(cli.invoked_subcommand.as_deref(), Some("lsp"));
    }

    #[test]
    fn test_command_records_explicit_global_color_override() {
        let cli = RuntimeCli::parse_from_smart(vec![
            "baml-cli".into(),
            "test".into(),
            "--color".into(),
            "always".into(),
        ]);
        let Commands::Test(args) = cli.command else {
            panic!("expected test command")
        };
        assert_eq!(
            args.cli_output.color,
            Some(crate::output::ColorChoice::Always)
        );
    }

    fn help_for(args: &[&str]) -> String {
        let mut command = RuntimeCli::command();
        command
            .try_get_matches_from_mut(args)
            .expect_err("help request should render clap help")
            .to_string()
    }

    fn help_for_path(path: &[&str], flag: &str) -> String {
        let mut args = vec!["baml-cli"];
        args.extend_from_slice(path);
        args.push(flag);
        help_for(&args)
    }

    #[test]
    fn root_help_presents_public_baml_command() {
        let help = help_for(&["baml-cli", "--help"]);
        assert!(help.contains("Usage: baml [OPTIONS] <COMMAND>"), "{help}");
        assert!(!help.contains("Usage: baml-cli"), "{help}");
    }

    #[test]
    fn pack_help_presents_public_baml_command() {
        let help = help_for(&["baml-cli", "pack", "--help"]);
        assert!(
            help.contains("Usage: baml pack [OPTIONS] [FUNCTION]"),
            "{help}"
        );
        assert!(!help.contains("Usage: baml-cli pack"), "{help}");
    }

    #[test]
    fn ide_help_presents_public_baml_command() {
        let help = help_for(&["baml-cli", "ide", "--help"]);
        assert!(
            help.contains("Usage: baml ide [OPTIONS] <COMMAND>"),
            "{help}"
        );
        assert!(!help.contains("Usage: baml-cli ide"), "{help}");
    }

    #[test]
    fn root_help_lists_check_command() {
        let help = help_for(&["baml-cli", "--help"]);
        assert!(help.contains("check"), "{help}");
        assert!(
            help.contains("Check BAML source files for compiler errors"),
            "{help}"
        );
    }

    #[test]
    fn check_help_includes_global_project_option() {
        let help = help_for(&["baml-cli", "check", "--help"]);
        assert!(help.contains("Usage: baml check [OPTIONS]"), "{help}");
        assert!(help.contains("--project <PATH>"), "{help}");
    }

    #[test]
    fn generate_add_help_lists_every_output_type() {
        let help = help_for(&["baml-cli", "generate", "add", "--help"]);
        for &output_type in baml_codegen_types::OutputType::all() {
            assert!(
                help.contains(output_type.canonical()),
                "missing {output_type:?} in:\n{help}"
            );
        }
        // Aliases parse but are hidden, so the possible-values line names
        // exactly what lands in baml.toml. The help text names them instead.
        // Collapse wrapping before matching: clap rewraps at terminal width.
        let unwrapped = help.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(!unwrapped.contains("possible values: python,"), "{help}");
        assert!(
            unwrapped.contains(
                "`python` and `typescript` are accepted as aliases for \
                 `python/pydantic` and `typescript/node`"
            ),
            "{help}"
        );
    }

    #[test]
    fn generate_accepts_bare_and_add_forms() {
        let cli = RuntimeCli::parse_from_smart(vec![
            "baml-cli".into(),
            "generate".into(),
            "--project".into(),
            ".".into(),
        ]);
        assert_eq!(cli.global.project, Some(PathBuf::from(".")));
        let Commands::Generate(args) = cli.command else {
            panic!("expected generate command");
        };
        assert!(args.command.is_none());

        let cli = RuntimeCli::parse_from_smart(vec![
            "baml-cli".into(),
            "generate".into(),
            "add".into(),
            "python".into(),
            "--project".into(),
            "workspace".into(),
        ]);
        let project = cli.global.project.clone();
        let Commands::Generate(args) = cli.command else {
            panic!("expected generate command");
        };
        let Some(crate::generate::GenerateCommand::Add(args)) = args.command else {
            panic!("expected generate add command");
        };
        assert_eq!(
            args.output_type,
            baml_codegen_types::OutputType::PythonPydantic
        );
        assert_eq!(project, Some(PathBuf::from("workspace")));
    }

    #[test]
    fn root_help_lists_playground_command() {
        let help = help_for(&["baml-cli", "--help"]);
        assert!(help.contains("playground"), "{help}");
        assert!(help.contains("Open the BAML playground"), "{help}");
    }

    #[test]
    fn output_dials_are_global_and_independent() {
        let cli = RuntimeCli::parse_from_smart(vec![
            "baml-cli".into(),
            "check".into(),
            "--output-preset".into(),
            "human".into(),
            "--color".into(),
            "never".into(),
            "--hyperlinks".into(),
            "always".into(),
            "--diagnostic-format".into(),
            "agent".into(),
        ]);

        assert_eq!(cli.output.preset, crate::output::OutputPreset::Human);
        assert_eq!(cli.output.color, Some(crate::output::ColorChoice::Never));
        assert_eq!(
            cli.output.hyperlinks,
            Some(crate::output::HyperlinkChoice::Always)
        );
        assert_eq!(
            cli.output.diagnostic_format,
            Some(crate::output::DiagnosticFormatChoice::Agent)
        );
    }

    #[test]
    fn output_dials_expose_documented_environment_variables() {
        let command = RuntimeCli::command();
        let expected = [
            ("preset", "BAML_OUTPUT_PRESET"),
            ("color", "BAML_COLOR"),
            ("hyperlinks", "BAML_HYPERLINKS"),
            ("diagnostic_format", "BAML_DIAGNOSTIC_FORMAT"),
        ];

        for (id, env) in expected {
            let arg = command
                .get_arguments()
                .find(|arg| arg.get_id() == id)
                .unwrap_or_else(|| panic!("missing argument {id}"));
            assert_eq!(arg.get_env(), Some(std::ffi::OsStr::new(env)));
        }
    }

    #[test]
    fn enum_options_use_compact_help() {
        let cases: &[(&[&str], &[&str])] = &[
            (
                &["run"],
                &[
                    "--output-format <FORMAT>\n          Format returned values [default: debug] [possible values: debug, json]",
                    "--color <WHEN>\n          Control ANSI colors [possible values: auto, always, never]",
                    "--output-preset <PRESET>\n          Select output defaults [default: auto] [possible values: auto, human, agent]",
                    "--hyperlinks <WHEN>\n          Control terminal hyperlinks [possible values: auto, always, never]",
                    "--diagnostic-format <FORMAT>\n          Select the diagnostic format [possible values: human, agent, concise]",
                ],
            ),
            (
                &["test"],
                &[
                    "--logs <LEVEL>\n          Set the BAML log level [default: off] [possible values: off, error, warn, info, debug]",
                ],
            ),
            (
                &["pack"],
                &[
                    "--output-format <FORMAT>\n          Format returned values [default: json] [possible values: debug, json]",
                ],
            ),
            (
                &["telemetry"],
                &[
                    "[ACTION]\n          Telemetry action [default: status] [possible values: status, enable, disable]",
                ],
            ),
        ];

        for (path, expected) in cases {
            let help = crate::help_command::render_for_test(path);
            assert!(!help.contains("Possible values:\n"), "{help}");
            for text in *expected {
                assert!(help.contains(text), "missing `{text}` in:\n{help}");
            }
        }
    }

    #[test]
    fn playground_help_presents_public_baml_command() {
        let help = help_for(&["baml-cli", "playground", "--help"]);
        assert!(help.contains("Usage: baml playground [OPTIONS]"), "{help}");
        assert!(help.contains("--file <PATH>"), "{help}");
        assert!(help.contains("--project <PATH>"), "{help}");
        assert!(help.contains("--port <PORT>"), "{help}");
        assert!(help.contains("--no-open"), "{help}");
    }

    #[test]
    fn test_help_is_a_complete_selector_and_profile_reference() {
        let help = crate::help_command::render_for_test(&["test"]);
        for required in [
            "Plain selectors match anywhere in the full ID",
            "Repeated includes are OR",
            "Excludes always win",
            "case-sensitive",
            "without shell expansion",
            "direct CLI includes narrow",
            "scalar options override",
            "With no default profile",
            "Profile args cannot contain",
        ] {
            assert!(help.contains(required), "missing `{required}` in:\n{help}");
        }
    }

    #[test]
    fn test_concise_help_omits_the_long_reference() {
        let help = help_for(&["baml-cli", "test", "--help"]);
        assert!(!help.contains("SELECTORS:"), "{help}");
        assert!(!help.contains("PROFILES:"), "{help}");
        assert!(help.contains("Use `baml help test`"), "{help}");
    }

    #[test]
    fn short_and_long_help_flags_render_the_same_concise_help() {
        for path in PUBLIC_COMMAND_PATHS {
            assert_eq!(
                help_for_path(path, "-h"),
                help_for_path(path, "--help"),
                "help differs for `baml {}`",
                path.join(" ")
            );
        }
    }

    #[test]
    fn every_public_command_has_detailed_examples() {
        for path in PUBLIC_COMMAND_PATHS.iter().filter(|path| !path.is_empty()) {
            let help = crate::help_command::render_for_test(path);
            assert!(
                help.contains("Examples:"),
                "`baml help {}` has no examples:\n{help}",
                path.join(" ")
            );
            assert!(
                help.find("Usage:") < help.find("Examples:"),
                "`baml help {}` puts examples before usage:\n{help}",
                path.join(" ")
            );
        }
    }

    #[test]
    fn documented_examples_parse() {
        let examples: &[&[&str]] = &[
            &["baml", "check"],
            &["baml", "check", "--project", "./my-project"],
            &["baml", "auth", "whoami"],
            &["baml", "auth", "logout"],
            &["baml", "auth", "login"],
            &["baml", "auth", "login", "--no-open"],
            &["baml", "describe"],
            &["baml", "describe", "baml"],
            &["baml", "describe", "baml.json"],
            &["baml", "describe", "Array"],
            &["baml", "describe", "String.split"],
            &["baml", "describe", "match"],
            &["baml", "test", "--list"],
            &["baml", "test", "-i", "root.payments::*"],
            &["baml", "test", "-i", "*::integration::*", "-x", "slow"],
            &["baml", "run", "main", "--", "--name", "Ada"],
            &[
                "baml",
                "run",
                "--function",
                "Extract",
                "--",
                "Extract",
                "--text",
                "input.txt",
            ],
            &["baml", "run", "-e", "1 + 2"],
            &["baml", "run", "--file", "script.baml"],
            &["baml", "run", "--list"],
            &["baml", "pack", "main"],
            &["baml", "pack", "main", "--output", "./my-tool"],
            &[
                "baml",
                "pack",
                "--function",
                "Extract",
                "--function",
                "Classify",
                "--output",
                "./baml-tools",
            ],
            &["baml", "pack", "--file", "script.baml", "main"],
            &[
                "baml",
                "feedback",
                "--title",
                "Issue (parser): panics on nested unions",
            ],
            &[
                "baml",
                "feedback",
                "--title",
                "...",
                "--description",
                "Minimum repro: class A { ... }",
            ],
            &["baml", "feedback", "-"],
            &[
                "baml",
                "feedback",
                "--title",
                "...",
                "--files",
                "screenshot.png",
                "--files",
                "repro.baml",
            ],
            &["baml", "feedback", "list", "--status", "open"],
            &["baml", "feedback", "view", "a1b2c3d4"],
            &["baml", "fmt"],
            &["baml", "fmt", "baml_src/main.baml"],
            &["baml", "fmt", "--dry-run"],
            &["baml", "generate"],
            &["baml", "generate", "--project", "./my-project"],
            &["baml", "generate", "--output-dir", "./generated"],
            &["baml", "init"],
            &["baml", "init", "./my-project", "--name", "my_project"],
            &["baml", "new", "./my-project"],
            &["baml", "new", "./my-project", "--name", "my_project"],
            &["baml", "playground"],
            &[
                "baml",
                "playground",
                "--project",
                "./my-project",
                "--no-open",
            ],
            &[
                "baml",
                "playground",
                "--file",
                "script.baml",
                "--port",
                "4265",
            ],
            &["baml", "ide", "install"],
            &["baml", "ide", "install", "--cursor"],
            &["baml", "ide", "install", "--output-dir", "./extensions"],
            &["baml", "agent", "install"],
            &["baml", "agent", "install", "--project", "./my-project"],
            &["baml", "agent", "install", "--source", "./skills.tar.gz"],
            &["baml", "lsp"],
            &["baml", "lsp", "--workspace", "./my-project"],
            &["baml", "help", "run"],
            &["baml", "help", "test"],
        ];

        for example in examples {
            RuntimeCli::command()
                .try_get_matches_from(*example)
                .unwrap_or_else(|error| panic!("`{}` did not parse: {error}", example.join(" ")));
        }
    }

    /// `run -e` accepts hyphen-prefixed values without consuming run flags.
    #[test]
    fn run_expression_accepts_hyphen_prefixed_value_and_preserves_run_flags() {
        let cli = RuntimeCli::parse_from_smart(vec![
            "baml-cli".into(),
            "run".into(),
            "-e".into(),
            "-7 % 3".into(),
            "--project".into(),
            "project".into(),
        ]);
        let project = cli.global.project.clone();
        let Commands::Run(args) = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(args.expression.as_deref(), Some("-7 % 3"));
        assert_eq!(project, Some(std::path::PathBuf::from("project")));
    }

    #[test]
    fn project_is_global_before_or_after_the_subcommand() {
        for argv in [
            vec!["baml", "--project", "workspace", "check"],
            vec!["baml", "check", "--project", "workspace"],
        ] {
            let cli = RuntimeCli::parse_from_smart(argv.into_iter().map(str::to_string).collect());
            assert_eq!(cli.global.project, Some(PathBuf::from("workspace")));
            assert!(matches!(cli.command, Commands::Check(_)));
        }
    }

    #[test]
    fn agent_project_and_source_have_distinct_meanings() {
        let cli = RuntimeCli::parse_from_smart(vec![
            "baml".into(),
            "agent".into(),
            "install".into(),
            "--project".into(),
            "workspace".into(),
            "--source".into(),
            "skills.tar.gz".into(),
        ]);
        let project = cli.global.project.clone();
        let Commands::Agent(crate::agent_command::AgentArgs {
            command: crate::agent_command::AgentCommand::Install(args),
        }) = cli.command
        else {
            panic!("expected agent install command");
        };
        assert_eq!(project, Some(PathBuf::from("workspace")));
        assert_eq!(args.source.as_deref(), Some("skills.tar.gz"));
    }
}
