use colored::*;
use log;
use std::io::Write;

mod builder;
mod command;
mod errors;
mod import_command;
mod init_command;
mod shell;
mod test_command;
mod update;
mod update_client;
mod version_command;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fmt;

/// A versatile CLI tool for managing BAML projects and their dependencies.
#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool for BAML project management.", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Specifies a subcommand to run.
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Builds a BAML project from the specified directory.
    Build(BuildArgs),
    /// Updates the CLI to the latest version.
    Update(UpdateArgs),
    /// Updates client libraries for a specified BAML project.
    UpdateClient(BuildArgs),
    /// Initializes a new BAML project in the current directory.
    Init(InitArgs),
    /// Runs tests for a BAML project.
    Test(TestArgs),
    /// Imports content into a BAML project.
    Import(ImportArgs),
    /// Reports the current and latest versions of everything.
    Version(version_command::VersionArgs),
}

#[derive(Args, Debug)]
struct BuildArgs {
    /// Optional: Specifies the directory of the BAML project to build.
    #[arg(long)]
    baml_dir: Option<String>,
}

#[derive(Args, Debug)]
struct InitArgs {
    /// Skips the interactive prompt and initializes the project with default settings.
    #[arg(long, short = 'n')]
    no_prompt: bool,
}

#[derive(Args, Debug)]
struct UpdateArgs {}

#[derive(Args, Debug)]
pub struct TestArgs {
    /// Optional: Specifies the directory of the BAML project to test.
    #[arg(long)]
    baml_dir: Option<String>,

    /// Includes specific tests or test groups in the execution.
    #[arg(long, short = 'i')]
    include: Vec<String>,

    /// Excludes specific tests or test groups from the execution.
    #[arg(long, short = 'x')]
    exclude: Vec<String>,

    /// Sets the default action to perform. Can be either 'run' to execute tests or 'list' to list available tests.
    #[arg(default_value_t = TestAction::List)]
    action: TestAction,

    /// Specifies a port for the test playground. Hidden from help text.
    #[arg(long, hide = true)]
    playground_port: Option<u16>,
}

impl fmt::Display for TestAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestAction::Run => write!(f, "run"),
            TestAction::List => write!(f, "list"),
        }
    }
}

#[derive(ValueEnum, Debug, Clone)]
enum TestAction {
    Run,
    List,
}

#[derive(Args, Debug)]
struct ImportArgs {
    /// Optional: Specifies the directory of the BAML project to which the content will be imported.
    #[arg(long)]
    baml_dir: Option<String>,

    /// Specifies the content to be imported into the BAML project.
    #[arg()]
    content: String,
}

impl fmt::Display for OutputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputType::Human => write!(f, "human"),
            OutputType::Json => write!(f, "json"),
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputType {
    Human,
    Json,
}

pub(crate) fn main() {
    const NAME: &str = concat!("[", clap::crate_name!(), "]");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level = record.level();
            let message = format!("{}", record.args());
            match level {
                log::Level::Info => writeln!(buf, "{} {}", NAME.dimmed(), message.dimmed()),
                log::Level::Warn => writeln!(buf, "{} {}", NAME.dimmed(), message.yellow()),
                log::Level::Error => writeln!(buf, "{} {}", "ERROR:".red().bold(), message.red()),
                _ => writeln!(buf, "{} {}: {}", NAME.dimmed(), level, message),
            }
        })
        .init();

    let args = Cli::parse();

    let response = match &args.command {
        Commands::Update(_args) => update::update(),
        Commands::Build(args) => builder::build(&args.baml_dir).map(|_| ()),
        Commands::UpdateClient(args) => update_client::update_client(&args.baml_dir),
        Commands::Init(args) => init_command::init_command(args.no_prompt)
            .and_then(|_| builder::build(&None).map(|_| ()))
            // Note: the update-client will run on the curr dir but perhaps we want to pass in the baml_src location that baml init got from the user.
            .and_then(|_| update_client::update_client(&None).map(|_| ()))
            .and_then(|_| {
                println!(
                    "\n{}\n{}\n{}",
                    "BAML Initialized successfully!".green(),
                    "Join the discord! https://discord.gg/yzaTpQ3tdT".cyan(),
                    "Documentation: https://docs.boundaryml.com".cyan()
                );
                Ok(())
            })
            .map(|_| ()),
        Commands::Test(args) => {
            builder::build(&args.baml_dir).and_then(|(baml_dir, config, schema)| {
                test_command::run(&args, &baml_dir, &config, schema)
            })
        }
        Commands::Import(args) => {
            builder::build(&args.baml_dir).and_then(|(baml_dir, config, schema)| {
                import_command::run(&args.content, &baml_dir, &config, schema)
            })
        }
        Commands::Version(args) => version_command::run(args),
    };

    if let Err(error) = response {
        log::error!("{}", error);
        std::process::exit(2);
    }
}
