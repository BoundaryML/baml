use colored::*;
use log;
use std::io::Write;

mod builder;
mod command;
mod errors;
mod update;
mod update_client;

use clap::{Args, Parser, Subcommand};

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
}

#[derive(Args, Debug)]
struct BuildArgs {
    #[arg(long)]
    baml_dir: Option<String>,
}

#[derive(Args, Debug)]
struct UpdateArgs {}

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
        Commands::Build(args) => builder::build(&args.baml_dir),
        Commands::UpdateClient(args) => update_client::update_client(&args.baml_dir),
    };

    if let Err(error) = response {
        log::error!("{}", error);
        std::process::exit(1);
    }
}
