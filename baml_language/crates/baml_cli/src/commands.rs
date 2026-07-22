// Wires up the BAML CLI subcommands: Run, Describe, Generate, Grep, Test,
// Format, and LanguageServer. `baml run` is the top-level entry for
// standalone execution.

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

pub(crate) const fn release_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}

#[derive(Parser, Debug)]
#[command(
    name = "baml-cli",
    bin_name = "baml",
    author,
    version = release_version(),
    about = "A CLI tool for working with BAML. Learn more at https://docs.boundaryml.com.",
    long_about = None,
    after_help = "Manage installed BAML toolchains:\n  baml toolchain --help"
)]
#[command(styles = crate::reporter::CLAP_STYLING)]
#[command(propagate_version = true)]
pub(crate) struct RuntimeCli {
    /// Enable specific features (can be specified multiple times)
    ///
    /// Available features:
    ///   beta - Enable beta features and suppress experimental warnings
    ///   display_all_warnings - Show all warnings in CLI output
    #[arg(long = "features", value_name = "FEATURE", global = true)]
    pub features: Vec<String>,

    /// When to use colored / hyperlinked output: auto (default), always, or never.
    ///
    /// `auto` enables color on an interactive terminal and disables it when the
    /// output is piped or captured by a known AI coding agent.
    #[arg(
        long,
        value_enum,
        default_value_t = crate::paint::ColorChoice::Auto,
        global = true
    )]
    pub color: crate::paint::ColorChoice,

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

    #[command(subcommand, about = "Manage authentication and claim your project")]
    Auth(crate::auth::AuthCommands),

    #[command(about = "Start an anonymous project (claim it later with `baml auth login`)")]
    Login(crate::auth::LoginArgs),

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

    #[command(about = "Semantic code search for BAML files", name = "grep")]
    Grep(crate::grep_command::GrepArgs),

    #[command(about = "Run BAML tests")]
    Test(crate::test_command::TestArgs),

    #[command(about = "Initialize a new BAML project (creates baml.toml)")]
    Init(crate::init_command::InitArgs),

    #[command(about = "Create a new BAML project directory")]
    New(crate::init_command::NewArgs),

    #[command(about = "Run a BAML function or script", disable_help_flag = true)]
    Run(crate::run_command::RunArgs),

    #[command(about = "Open the BAML playground in your browser")]
    Playground(crate::playground_command::PlaygroundArgs),

    #[command(about = "Package a BAML target as a standalone executable")]
    Pack(crate::pack_command::PackArgs),

    #[command(about = "Install or manage IDE integration assets")]
    Ide(crate::ide_command::IdeArgs),

    #[command(about = "Install BAML agent skills for this project")]
    Agent(crate::agent_command::AgentArgs),

    #[command(about = "Starts a language server", name = "lsp")]
    LanguageServer(crate::lsp::LanguageServerArgs),

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

        if baml_internal_env_is_truthy() {
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

        if let Err(err) = RuntimeCli::update_from_arg_matches(&mut cli, &matches) {
            err.exit();
        }

        // Record the invoked subcommand's clap name for telemetry, straight
        // from the parsed matches so it always matches what clap registered.
        cli.invoked_subcommand = matches.subcommand_name().map(str::to_string);

        cli
    }

    pub fn run(&self) -> Result<crate::ExitCode> {
        // The detached telemetry flush child must run before (and without)
        // `record_invocation` below: recording its own invocation would
        // seal a new queue file on drop and spawn another child, forever.
        if let Commands::FlushTelemetry(args) = &self.command {
            return args.run();
        }

        // Fire anonymous, best-effort telemetry for this invocation. The
        // event is appended to an on-disk queue (one atomic write); on drop
        // of the guard (after the match below returns) the queue file is
        // sealed and a detached child process delivers it after this
        // process has already exited. It never fails or delays the command.
        let _telemetry = crate::telemetry::record_invocation(
            self.invoked_subcommand.as_deref().unwrap_or("unknown"),
        );

        // Resolve color/hyperlink output once, before any subcommand writes.
        crate::paint::init_color(self.color);

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

        match &self.command {
            Commands::Init(args) => args.run(),
            Commands::New(args) => args.run(),
            Commands::Check(args) => args.run(),
            Commands::Run(args) => args.run(),
            Commands::Playground(args) => args.run(),
            Commands::Pack(args) => args.run(),
            Commands::Ide(args) => args.run(),
            Commands::Agent(args) => args.run(),
            Commands::Describe(args) => args.run(),
            Commands::Generate(args) => args.run(),
            Commands::Grep(args) => args.run(),
            Commands::Test(args) => args.run(),
            Commands::LanguageServer(args) => match args.run() {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(e) => {
                    crate::reporter::print_error(e);
                    Ok(crate::ExitCode::Other)
                }
            },
            Commands::Auth(args) => args.run(),
            Commands::Login(args) => args.run(),
            Commands::Telemetry(args) => args.run(),
            // Handled by the early return above, before telemetry wiring.
            Commands::FlushTelemetry(args) => args.run(),
            Commands::Format(args) => args.run(),
        }
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

    fn help_for(args: &[&str]) -> String {
        let mut command = RuntimeCli::command();
        command
            .try_get_matches_from_mut(args)
            .expect_err("help request should render clap help")
            .to_string()
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
            help.contains("Usage: baml pack [OPTIONS] [TARGET]"),
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
    fn check_help_mentions_default_search_start() {
        let help = help_for(&["baml-cli", "check", "--help"]);
        assert!(help.contains("Usage: baml check [OPTIONS]"), "{help}");
        assert!(
            help.contains("Project search starting point. Defaults to the current directory"),
            "{help}"
        );
    }

    #[test]
    fn root_help_lists_playground_command() {
        let help = help_for(&["baml-cli", "--help"]);
        assert!(help.contains("playground"), "{help}");
        assert!(help.contains("Open the BAML playground"), "{help}");
    }

    #[test]
    fn playground_help_presents_public_baml_command() {
        let help = help_for(&["baml-cli", "playground", "--help"]);
        assert!(help.contains("Usage: baml playground [OPTIONS]"), "{help}");
        assert!(help.contains("--file <PATH>"), "{help}");
        assert!(help.contains("--from <PATH>"), "{help}");
        assert!(help.contains("--port <PORT>"), "{help}");
        assert!(help.contains("--no-open"), "{help}");
    }

    /// `run -e` accepts hyphen-prefixed values without consuming run flags.
    #[test]
    fn run_expression_accepts_hyphen_prefixed_value_and_preserves_run_flags() {
        let cli = RuntimeCli::parse_from_smart(vec![
            "baml-cli".into(),
            "run".into(),
            "-e".into(),
            "-7 % 3".into(),
            "--from".into(),
            "project".into(),
        ]);
        let Commands::Run(args) = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(args.expression.as_deref(), Some("-7 % 3"));
        assert_eq!(args.from, Some(std::path::PathBuf::from("project")));
    }
}
