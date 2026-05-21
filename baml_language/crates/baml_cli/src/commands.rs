// Wires up the BAML CLI subcommands: Run, Describe, Generate, Grep, Test,
// Format, and LanguageServer. `baml run` is the top-level entry for
// standalone execution.

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

pub(crate) const fn release_version() -> &'static str {
    match option_env!("BAML_RELEASE_VERSION") {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    }
}

#[derive(Parser, Debug)]
#[command(author, version = release_version(), about = "A CLI tool for working with BAML. Learn more at https://docs.boundaryml.com.", long_about = None)]
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

    /// Specifies a subcommand to run.
    #[command(subcommand)]
    pub(crate) command: Commands,
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

    // #[command(about = "Starts a server that translates LLM responses to BAML responses")]
    // Serve(baml_runtime::cli::serve::ServeArgs),

    // #[command(about = "Starts a development server")]
    // Dev(baml_runtime::cli::dev::DevArgs),

    // #[command(subcommand, about = "Authenticate with Boundary Cloud", hide = true)]
    // Auth(crate::auth::AuthCommands),

    // #[command(about = "Login to Boundary Cloud (alias for `baml auth login`)", hide = true)]
    // Login(crate::auth::LoginArgs),
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

    #[command(about = "Package a BAML target as a standalone executable")]
    Pack(crate::pack_command::PackArgs),

    #[command(about = "Starts a language server", name = "lsp")]
    LanguageServer(crate::lsp::LanguageServerArgs),
    // #[command(about = "Start an interactive REPL for BAML expressions", hide = true)]
    // Repl(baml_runtime::cli::repl::ReplArgs),

    // #[command(about = "Optimize prompts using GEPA algorithm")]
    // Optimize(baml_runtime::cli::optimize::OptimizeArgs),
}

impl RuntimeCli {
    /// Parse CLI arguments, unhiding all subcommands if the BAML_INTERNAL environment variable is set.
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

        cli
    }

    pub fn run(&self) -> Result<crate::ExitCode> {
        match &self.command {
            Commands::Init(args) => args.run(),
            Commands::New(args) => args.run(),
            Commands::Run(args) => args.run(),
            Commands::Pack(args) => args.run(),
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
        let expected = option_env!("BAML_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
        assert_eq!(release_version(), expected);
    }
}
