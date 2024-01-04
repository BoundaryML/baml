use colored::*;
use log;
use std::io::Write;

mod builder;
mod command;
mod errors;
mod import_command;
mod init_command;
mod test_command;
mod update;
mod update_client;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fmt;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // Build a BAML project
    Build(BuildArgs),
    // Update this cli
    Update(UpdateArgs),
    // Update client libraries for a BAML project
    UpdateClient(BuildArgs),
    Init(InitArgs),
    Test(TestArgs),
    Import(ImportArgs),
}

#[derive(Args, Debug)]
struct BuildArgs {
    #[arg(long)]
    baml_dir: Option<String>,
}

#[derive(Args, Debug)]
struct InitArgs {}

#[derive(Args, Debug)]
struct UpdateArgs {}

#[derive(Args, Debug)]
pub struct TestArgs {
    #[arg(long)]
    baml_dir: Option<String>,

    #[arg(long, short = 'i')]
    include: Vec<String>,

    #[arg(long, short = 'x')]
    exclude: Vec<String>,

    // The `default_value_t` is used to set a default value for the `action` field.
    // `action` is now an optional positional argument.
    #[arg(default_value_t = TestAction::List)]
    action: TestAction,

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
    #[arg(long)]
    baml_dir: Option<String>,

    #[arg()]
    content: String,
}

pub(crate) fn main() {
    const NAME: &str = concat!("[", env!("CARGO_PKG_NAME"), "]");
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
        Commands::Init(_args) => {
            init_command::init_command().and_then(|_| builder::build(&None).map(|_| ()))
        }
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
    };

    if let Err(error) = response {
        log::error!("{}", error);
        std::process::exit(1);
    }
}

enum ImportItems {
    Version1(ImportItemsVersion1),
}

struct ImportItemsVersion1 {
    functionName: String,
    test_input: TestInput,
}

enum TestInput {}
