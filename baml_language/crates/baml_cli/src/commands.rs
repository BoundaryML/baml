// The BAML CLI command tree and its dispatcher.
//
// Rules (BEP-086):
// - Anything that needs a Boundary Cloud account lives under `baml cloud`.
// - A command that runs against local or cloud data exists as both
//   `baml local <cmd>` and `baml cloud <cmd>`, with the same args and output.
// - `--project-dir` is always the local path; under `baml cloud`, `--project`
//   is always the cloud project.
// - `login`, `logout` and `query` are the only shortcuts. Docs, help examples
//   and agents use the explicit commands.
// - Commands return a `Report`; only the dispatcher writes stdout.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};

use crate::{
    ExitCode,
    output::{run_command, run_raw},
    recordings::{Backend, CloudScope, Query, RecordingsCommands},
};

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

    #[command(subcommand)]
    pub(crate) command: Commands,

    /// Name of the invoked top-level subcommand, as registered with clap
    /// (e.g. `"fmt"`, `"check"`). Not a CLI argument: it's populated in
    /// [`Self::parse_from_smart`] from the parsed matches so telemetry can
    /// report the exact clap name without a hand-maintained mapping.
    #[arg(skip)]
    pub(crate) invoked_subcommand: Option<String>,
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct GlobalArgs {
    /// Discover the BAML project from this path.
    #[arg(
        long = "project-dir",
        value_name = "PATH",
        global = true,
        help_heading = "Global options",
        display_order = 10
    )]
    pub project_dir: Option<PathBuf>,

    /// Print no status or progress on stderr.
    #[arg(
        short,
        long,
        global = true,
        help_heading = "Global options",
        display_order = 20
    )]
    pub quiet: bool,
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct CompilerArgs {
    #[arg(
        short = 'F',
        long = "features",
        value_name = "FEATURES",
        value_delimiter = ',',
        action = clap::ArgAction::Append,
        help_heading = "Compiler options",
        help = "Enable compiler features; repeatable or comma-separated [possible values: beta, display_all_warnings]"
    )]
    pub features: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(about = "Create a new BAML project directory")]
    New(crate::init_command::NewArgs),

    #[command(about = "Initialize a new BAML project (creates baml.toml)")]
    Init(crate::init_command::InitArgs),

    #[command(about = "Check BAML source files for compiler errors")]
    Check(crate::check_command::CheckArgs),

    #[command(about = "Format BAML source files", name = "fmt")]
    Format(crate::format::FormatArgs),

    #[command(about = "Run BAML tests")]
    Test(crate::test_command::TestArgs),

    #[command(about = "Run a BAML function or script")]
    Run(crate::run_command::RunArgs),

    #[command(about = "Package a BAML target as a standalone executable")]
    Pack(crate::pack_command::PackArgs),

    #[command(about = "Generate client code from BAML definitions")]
    Generate(crate::generate::GenerateArgs),

    #[command(about = "Describe a BAML symbol")]
    Describe(crate::describe_command::DescribeArgs),

    #[command(name = "codequality", about = "Show code quality metrics")]
    CodeQuality(crate::codequality_command::CodeQualityArgs),

    #[command(
        about = "Open the BAML playground in a browser",
        after_long_help = "Examples:\n  Open the nearest project:\n    baml playground\n\n  Serve a project without opening a browser:\n    baml playground --project-dir ./my-project --no-open"
    )]
    Playground(crate::playground_command::PlaygroundArgs),

    #[command(
        subcommand,
        about = "Commands that also exist under `baml cloud`, run on this machine",
        after_long_help = "Examples:\n  Query this project's local recordings:\n    baml local recordings query \"SELECT span_name FROM spans LIMIT 5\""
    )]
    Local(LocalCommands),

    #[command(
        about = "Boundary Cloud: everything that needs an account",
        after_long_help = "Examples:\n  Log in:\n    baml cloud auth login\n\n  Query your cloud recordings:\n    baml cloud recordings query \"SELECT span_name FROM spans LIMIT 5\""
    )]
    Cloud(CloudArgs),

    #[command(
        about = "Shortcut for `recordings query`: cloud when logged in, otherwise local",
        after_long_help = crate::recordings::QUERY_HELP
    )]
    Query(crate::recordings::QueryArgs),

    #[command(about = "Shortcut for `baml cloud auth login`")]
    Login(crate::auth::LoginArgs),

    #[command(about = "Shortcut for `baml cloud auth logout`")]
    Logout(crate::auth::LogoutArgs),

    #[command(
        subcommand,
        about = "Install or manage IDE integration assets",
        after_long_help = "Examples:\n  Install into the detected editor:\n    baml ide install\n\n  Install into Cursor:\n    baml ide install --cursor"
    )]
    Ide(crate::ide_command::IdeCommand),

    #[command(
        subcommand,
        about = "Install the toolchain's BAML agent skill for this project",
        after_long_help = "Examples:\n  Install the bundled skill:\n    baml agent install\n\n  Install in a specific project:\n    baml agent install --project-dir ./my-project"
    )]
    Agent(crate::agent_command::AgentCommand),

    #[command(
        about = "Show or change BAML CLI telemetry preferences",
        after_long_help = "Examples:\n  Show the current setting:\n    baml telemetry\n\n  Opt out:\n    baml telemetry disable"
    )]
    Telemetry(crate::telemetry_command::TelemetryArgs),

    #[command(about = "Display documentation for a command")]
    Help(crate::help_command::HelpArgs),

    #[command(about = "Start a language server", name = "lsp")]
    LanguageServer(crate::lsp::LanguageServerArgs),

    // The detached telemetry flush child (see `telemetry::queue`). Spawned
    // by the CLI itself on exit / rotation; hidden from help but callable
    // by name.
    #[command(
        name = "__flush-telemetry",
        about = "(internal) drain the on-disk telemetry queue",
        hide = true
    )]
    FlushTelemetry(crate::telemetry_command::FlushTelemetryArgs),
}

/// Commands with a `baml cloud` twin. Nothing else belongs here.
#[derive(Subcommand, Debug)]
pub(crate) enum LocalCommands {
    #[command(
        subcommand,
        about = "Query this project's local recordings with SQL",
        after_long_help = "Examples:\n  Run a query:\n    baml local recordings query \"SELECT span_name FROM spans LIMIT 5\"\n\n  List the relations:\n    baml local recordings schema"
    )]
    Recordings(RecordingsCommands),
}

#[derive(Args, Debug)]
pub(crate) struct CloudArgs {
    #[command(flatten)]
    pub scope: CloudScope,

    #[command(subcommand)]
    pub command: CloudCommands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CloudCommands {
    #[command(
        subcommand,
        about = "Manage authentication",
        long_about = "Manage the identity used by Boundary Cloud.\n\nUse `baml cloud auth login` to authenticate, `baml cloud auth status` to verify authentication and inspect the selected project, and `baml cloud auth logout` to remove the authenticated session.",
        after_long_help = "Examples:\n  Log in:\n    baml cloud auth login\n\n  Verify authentication and show the selected project:\n    baml cloud auth status\n\n  Log out:\n    baml cloud auth logout"
    )]
    Auth(crate::auth::AuthCommands),

    #[command(
        subcommand,
        about = "Query your Boundary Cloud recordings with SQL",
        after_long_help = "Examples:\n  Run a query:\n    baml cloud recordings query \"SELECT span_name FROM spans LIMIT 5\"\n\n  Pick the project and environment:\n    baml cloud recordings query \"SELECT span_name FROM spans\" --project acme/app --env staging"
    )]
    Recordings(RecordingsCommands),
}

impl RuntimeCli {
    pub(crate) fn command() -> clap::Command {
        let mut command = <Self as CommandFactory>::command();
        configure_help_hints(&mut command, &[]);
        command
    }

    /// Parse CLI arguments. On a parse failure this prints clap's error and
    /// exits with status 2, matching normal CLI behavior.
    pub fn parse_from_smart(argv: Vec<String>) -> Self {
        use clap::FromArgMatches;

        let mut command = RuntimeCli::command();

        // BEP-027 "`--` separator": clap already prints a helpful "to pass
        // '<flag>' as a value, use '-- <flag>'" tip on an unknown argument.
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
            test.cli_log = matches
                .subcommand_matches("test")
                .filter(|matches| {
                    matches.value_source("log") == Some(clap::parser::ValueSource::CommandLine)
                })
                .map(|_| test.log);
        }

        cli
    }

    pub fn run(&self) -> Result<ExitCode> {
        crate::reporter::init(self.global.quiet);

        // The detached telemetry flush child must run before (and without)
        // `record_invocation` below: recording its own invocation would
        // seal a new queue file on drop and spawn another child, forever.
        if let Commands::FlushTelemetry(args) = &self.command {
            return args.run();
        }

        // Resolve every output dial once, before any subcommand writes.
        crate::output::init(self.output)?;
        let cx = crate::output::context(self.output.format, self.global.project_dir.clone());

        if self.command.requires_agent_skill() {
            crate::skill_check::check(cx.project_dir.as_deref())?;
        }

        // Fire anonymous, best-effort telemetry for this invocation. The
        // event is appended to an on-disk queue (one atomic write); on drop
        // of the guard (after the match below returns) the queue file is
        // sealed and a detached child process delivers it after this
        // process has already exited. It never fails or delays the command.
        // `help` only renders text, so it is not recorded.
        let _telemetry = (!matches!(self.command, Commands::Help(_))).then(|| {
            crate::telemetry::record_invocation(
                self.invoked_subcommand.as_deref().unwrap_or("unknown"),
            )
        });

        let cx = &cx;
        match &self.command {
            Commands::New(args) => run_command(args, cx),
            Commands::Init(args) => run_command(args, cx),
            Commands::Check(args) => run_command(args, cx),
            Commands::Format(args) => run_command(args, cx),
            Commands::Test(args) => run_command(args, cx),
            Commands::Run(args) => run_raw(args, cx),
            Commands::Pack(args) => run_command(args, cx),
            Commands::Generate(args) => run_command(args, cx),
            Commands::Describe(args) => run_command(args, cx),
            Commands::CodeQuality(args) => run_command(args, cx),
            Commands::Playground(args) => run_command(args, cx),
            Commands::Local(LocalCommands::Recordings(command)) => command.run(cx, Backend::Local),
            Commands::Cloud(cloud) => match &cloud.command {
                CloudCommands::Auth(command) => command.run(cx, &cloud.scope),
                CloudCommands::Recordings(command) => command.run(cx, Backend::Cloud(&cloud.scope)),
            },
            Commands::Query(args) => run_command(
                &Query {
                    args,
                    backend: Backend::Auto,
                },
                cx,
            ),
            Commands::Login(args) => crate::auth::AuthCommands::run_one(args, cx),
            Commands::Logout(args) => crate::auth::AuthCommands::run_one(args, cx),
            Commands::Ide(crate::ide_command::IdeCommand::Install(args)) => run_command(args, cx),
            Commands::Agent(crate::agent_command::AgentCommand::Install(args)) => {
                run_command(args, cx)
            }
            Commands::Telemetry(args) => run_command(args, cx),
            Commands::Help(args) => run_command(args, cx),
            Commands::LanguageServer(args) => run_raw(args, cx),
            // Handled by the early return above, before telemetry wiring.
            Commands::FlushTelemetry(args) => args.run(),
        }
    }
}

impl Commands {
    /// Commands an agent runs while writing BAML need the matching skill.
    fn requires_agent_skill(&self) -> bool {
        matches!(
            self,
            Self::New(_)
                | Self::Init(_)
                | Self::Check(_)
                | Self::Format(_)
                | Self::Test(_)
                | Self::Run(_)
                | Self::Pack(_)
                | Self::Generate(_)
                | Self::Describe(_)
                | Self::Playground(_)
                | Self::Query(_)
                | Self::Local(_)
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(argv: &[&str]) -> RuntimeCli {
        RuntimeCli::parse_from_smart(argv.iter().map(|arg| (*arg).to_owned()).collect())
    }

    fn help_for(args: &[&str]) -> String {
        RuntimeCli::command()
            .try_get_matches_from(args)
            .expect_err("help request should render clap help")
            .to_string()
    }

    /// Every visible command path, depth first.
    fn public_command_paths() -> Vec<Vec<String>> {
        fn walk(command: &clap::Command, path: &[String], out: &mut Vec<Vec<String>>) {
            out.push(path.to_vec());
            for child in command.get_subcommands().filter(|c| !c.is_hide_set()) {
                let mut child_path = path.to_vec();
                child_path.push(child.get_name().to_owned());
                walk(child, &child_path, out);
            }
        }
        let mut paths = Vec::new();
        walk(&RuntimeCli::command(), &[], &mut paths);
        paths
    }

    /// The whole public surface: every command, positional, flag, value
    /// name, default and env var. A change here is a change to the CLI's
    /// contract and must be reviewed as one.
    #[test]
    fn surface_is_snapshot_tested() {
        fn walk(command: &clap::Command, path: &str, inherited: &[clap::Id], out: &mut String) {
            use std::fmt::Write as _;
            let hidden = if command.is_hide_set() {
                " (hidden)"
            } else {
                ""
            };
            writeln!(out, "{path}{hidden}").unwrap();
            let mut args: Vec<String> = command
                .get_arguments()
                .filter(|arg| !inherited.contains(arg.get_id()))
                .map(|arg| {
                    let mut line = String::from("  ");
                    match (arg.get_short(), arg.get_long()) {
                        (Some(short), Some(long)) => write!(line, "-{short}, --{long}").unwrap(),
                        (None, Some(long)) => write!(line, "--{long}").unwrap(),
                        (Some(short), None) => write!(line, "-{short}").unwrap(),
                        (None, None) => write!(line, "<{}>", arg.get_id()).unwrap(),
                    }
                    if arg.get_action().takes_values() && arg.get_long().is_some() {
                        let names = arg.get_value_names().map_or_else(String::new, |names| {
                            names
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" ")
                        });
                        write!(line, " <{names}>").unwrap();
                    }
                    let defaults: Vec<_> = arg
                        .get_default_values()
                        .iter()
                        .map(|value| value.to_string_lossy())
                        .collect();
                    if !defaults.is_empty() {
                        write!(line, " [default: {}]", defaults.join(",")).unwrap();
                    }
                    if let Some(env) = arg.get_env() {
                        write!(line, " [env: {}]", env.to_string_lossy()).unwrap();
                    }
                    if arg.is_global_set() {
                        line.push_str(" [global]");
                    }
                    if arg.is_hide_set() {
                        line.push_str(" (hidden)");
                    }
                    line
                })
                .collect();
            args.sort();
            for arg in args {
                writeln!(out, "{arg}").unwrap();
            }
            let globals: Vec<clap::Id> = command
                .get_arguments()
                .filter(|arg| arg.is_global_set())
                .map(|arg| arg.get_id().clone())
                .collect();
            for child in command.get_subcommands() {
                walk(
                    child,
                    &format!("{path} {}", child.get_name()),
                    &globals,
                    out,
                );
            }
        }
        let mut command = RuntimeCli::command();
        command.build();
        let mut surface = String::new();
        walk(&command, "baml", &[], &mut surface);
        insta::assert_snapshot!("cli_surface", surface);
    }

    #[test]
    fn release_version_matches_compile_time_setting() {
        assert_eq!(release_version(), baml_version::CANONICAL_VERSION);
    }

    /// `parse_from_smart` records the invoked subcommand's clap name for
    /// telemetry, straight from the parsed matches.
    #[test]
    fn parse_records_invoked_subcommand_name() {
        assert_eq!(
            parse(&["baml", "fmt"]).invoked_subcommand.as_deref(),
            Some("fmt")
        );
        assert_eq!(
            parse(&["baml", "check"]).invoked_subcommand.as_deref(),
            Some("check")
        );
    }

    #[test]
    fn test_command_records_explicit_global_color_override() {
        let Commands::Test(args) = parse(&["baml", "test", "--color", "always"]).command else {
            panic!("expected test command")
        };
        assert_eq!(
            args.cli_output.color,
            Some(crate::output::ColorChoice::Always)
        );
    }

    #[test]
    fn help_presents_the_public_baml_command() {
        for (args, usage) in [
            (
                &["baml-cli", "--help"][..],
                "Usage: baml [OPTIONS] <COMMAND>",
            ),
            (
                &["baml-cli", "pack", "--help"],
                "Usage: baml pack [OPTIONS] [FUNCTION]",
            ),
            (
                &["baml-cli", "ide", "--help"],
                "Usage: baml ide [OPTIONS] <COMMAND>",
            ),
            (
                &["baml-cli", "check", "--help"],
                "Usage: baml check [OPTIONS]",
            ),
        ] {
            let help = help_for(args);
            assert!(help.contains(usage), "{help}");
            assert!(!help.contains("Usage: baml-cli"), "{help}");
        }
    }

    #[test]
    fn global_flags_work_before_or_after_the_subcommand() {
        for argv in [
            [
                "baml",
                "--project-dir",
                "workspace",
                "--output-format",
                "json",
                "check",
            ],
            [
                "baml",
                "check",
                "--project-dir",
                "workspace",
                "--output-format",
                "json",
            ],
        ] {
            let cli = parse(&argv);
            assert_eq!(cli.global.project_dir, Some(PathBuf::from("workspace")));
            assert_eq!(cli.output.format, crate::output::OutputFormat::Json);
        }
    }

    /// `--project-dir` is always the local path; under `baml cloud`,
    /// `--project` is always the cloud project.
    #[test]
    fn cloud_project_is_separate_from_the_local_project_dir() {
        let cli = parse(&[
            "baml",
            "--project-dir",
            "/tmp/source-app",
            "cloud",
            "recordings",
            "query",
            "SELECT 1",
            "--project",
            "acme/app",
            "--env",
            "staging",
        ]);
        assert_eq!(
            cli.global.project_dir,
            Some(PathBuf::from("/tmp/source-app"))
        );
        let Commands::Cloud(cloud) = cli.command else {
            panic!("expected cloud");
        };
        assert_eq!(cloud.scope.project.as_deref(), Some("acme/app"));
        assert_eq!(cloud.scope.env.as_deref(), Some("staging"));
    }

    /// The old spellings are gone, not deprecated.
    #[test]
    fn removed_spellings_do_not_parse() {
        for argv in [
            &["baml", "check", "--project", "x"][..],
            &["baml", "check", "--from", "x"],
            &["baml", "check", "-v"],
            &["baml", "check", "--no-progress"],
            &["baml", "auth", "login"],
            &["baml", "clean"],
            &["baml", "update"],
            &["baml", "upgrade"],
            &["baml", "query", "SELECT 1", "--local"],
            &["baml", "query", "--schema"],
            &["baml", "query", "SELECT 1", "--project", "acme/app"],
            &["baml", "query", "SELECT 1", "--format", "json"],
            &["baml", "cloud", "query", "SELECT 1"],
            &["baml", "describe", "--json"],
            &["baml", "describe", "--budget", "5"],
            &["baml", "describe", "--symbols"],
            &["baml", "describe", "baml", "--export"],
            &["baml", "pack", "main", "--output-format", "debug"],
            &["baml", "run", "main", "--output-format", "debug"],
            &["baml", "codequality", "--output-format", "table"],
            &["baml", "agent", "install", "--dir", "x"],
        ] {
            assert!(
                RuntimeCli::command().try_get_matches_from(argv).is_err(),
                "`{}` should not parse",
                argv.join(" ")
            );
        }
    }

    #[test]
    fn generate_add_help_lists_every_output_type() {
        let help = help_for(&["baml-cli", "generate", "add", "--help"]);
        for &output_type in baml_sdkgen_types::OutputType::all() {
            assert!(
                help.contains(output_type.add_name()),
                "missing {output_type:?} in:\n{help}"
            );
        }
    }

    #[test]
    fn generate_accepts_bare_and_add_forms() {
        let Commands::Generate(args) = parse(&["baml", "generate"]).command else {
            panic!("expected generate command");
        };
        assert!(args.command.is_none());

        let Commands::Generate(args) =
            parse(&["baml", "generate", "add", "python/pydantic2"]).command
        else {
            panic!("expected generate command");
        };
        let Some(crate::generate::GenerateCommand::Add(args)) = args.command else {
            panic!("expected generate add command");
        };
        assert_eq!(
            args.output_type,
            baml_sdkgen_types::OutputType::PythonPydantic
        );
    }

    #[test]
    fn output_dials_are_global_and_independent() {
        let cli = parse(&[
            "baml",
            "check",
            "--output-preset",
            "human",
            "--color",
            "never",
            "--hyperlinks",
            "always",
            "--diagnostic-format",
            "agent",
            "--agent-skill-check",
            "off",
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
        assert_eq!(
            cli.output.agent_skill_check,
            crate::output::AgentSkillCheckChoice::Off
        );
    }

    #[test]
    fn output_dials_have_no_environment_mirrors() {
        let command = RuntimeCli::command();
        for id in [
            "format",
            "preset",
            "color",
            "hyperlinks",
            "diagnostic_format",
            "agent_skill_check",
        ] {
            let arg = command
                .get_arguments()
                .find(|arg| arg.get_id() == id)
                .unwrap_or_else(|| panic!("missing argument {id}"));
            assert_eq!(arg.get_env(), None, "{id} must not read an env var");
        }
    }

    #[test]
    fn enum_options_use_compact_help() {
        let cases: &[(&[&str], &[&str])] = &[
            (
                &["run"],
                &[
                    "--output-format <FORMAT>\n          What goes to stdout [default: text] [possible values: text, json, jsonl]",
                    "--color <WHEN>\n          Control ANSI colors [possible values: auto, always, never]",
                    "--output-preset <PRESET>\n          Select output defaults [default: auto] [possible values: auto, human, agent]",
                    "--hyperlinks <WHEN>\n          Control terminal hyperlinks [possible values: auto, always, never]",
                    "--diagnostic-format <FORMAT>\n          Select the diagnostic format [possible values: human, agent, concise]",
                ],
            ),
            (
                &["test"],
                &[
                    "--log <LEVEL>\n          Set the BAML log level; overrides BAML_LOG [default: info] [possible values: off, error,\n          warn, info, debug, trace]",
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
        for path in public_command_paths() {
            let help = |flag: &str| {
                let mut args = vec!["baml-cli"];
                args.extend(path.iter().map(String::as_str));
                args.push(flag);
                help_for(&args)
            };
            assert_eq!(
                help("-h"),
                help("--help"),
                "help differs for `baml {}`",
                path.join(" ")
            );
        }
    }

    #[test]
    fn every_public_command_has_detailed_examples() {
        for path in public_command_paths()
            .iter()
            .filter(|path| !path.is_empty())
        {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            let help = crate::help_command::render_for_test(&path);
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

    /// Every `baml ...` line in every command's help examples parses, and
    /// none of them uses a shortcut (docs use the explicit commands).
    #[test]
    fn documented_examples_parse_and_avoid_shortcuts() {
        for path in public_command_paths() {
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            let help = crate::help_command::render_for_test(&path);
            let Some(examples) = help.split("Examples:").nth(1) else {
                continue;
            };
            let examples = examples.split("\n\n\n").next().unwrap_or(examples);
            for line in examples.lines().map(str::trim) {
                // Pipelines and elided values are illustrations, not argv.
                if !line.starts_with("baml ") || line.contains('|') || line.contains("...") {
                    continue;
                }
                let argv = shell_words(line);
                RuntimeCli::command()
                    .try_get_matches_from(&argv)
                    .unwrap_or_else(|error| panic!("`{line}` did not parse: {error}"));
                let is_shortcut_help =
                    matches!(path.first(), Some(&("query" | "login" | "logout")));
                let shortcut = matches!(argv[1].as_str(), "query" | "login" | "logout");
                // The one sanctioned mention: the query help shows its own shortcut.
                let allowed = argv[1] == "query" && help.contains("Whichever you are set up for");
                assert!(
                    is_shortcut_help || !shortcut || allowed,
                    "`baml help {}` documents the shortcut `{line}`",
                    path.join(" ")
                );
            }
        }
    }

    /// Splits an example line on whitespace, honoring single and double quotes.
    fn shell_words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quote = None;
        let mut in_word = false;
        for c in line.chars() {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => word.push(c),
                None if c == '"' || c == '\'' => {
                    quote = Some(c);
                    in_word = true;
                }
                None if c.is_whitespace() => {
                    if in_word {
                        words.push(std::mem::take(&mut word));
                        in_word = false;
                    }
                }
                None => {
                    word.push(c);
                    in_word = true;
                }
            }
        }
        if in_word {
            words.push(word);
        }
        words
    }

    /// `run -e` accepts hyphen-prefixed values without consuming run flags.
    #[test]
    fn run_expression_accepts_hyphen_prefixed_value_and_preserves_run_flags() {
        let cli = parse(&["baml", "run", "-e", "-7 % 3", "--project-dir", "project"]);
        assert_eq!(cli.global.project_dir, Some(PathBuf::from("project")));
        let Commands::Run(args) = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(args.expression.as_deref(), Some("-7 % 3"));
    }

    #[test]
    fn compiler_features_are_scoped_and_cargo_shaped() {
        let Commands::Check(args) =
            parse(&["baml", "check", "-F", "beta,display_all_warnings"]).command
        else {
            panic!("expected check command");
        };
        assert_eq!(args.compiler.features, ["beta", "display_all_warnings"]);

        let help = crate::help_command::render_for_test(&["check"]);
        assert!(
            help.contains("Enable compiler features; repeatable or comma-separated"),
            "{help}"
        );
        assert!(help.contains("[possible values: beta,"), "{help}");
        assert!(help.contains("display_all_warnings]"), "{help}");
        assert!(!help.contains("Available features:"), "{help}");
    }
}
