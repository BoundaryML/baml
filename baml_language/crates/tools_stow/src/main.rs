//! cargo-stow: workspace structure linting for Rust monorepos.
//!
//! The behaviour is specified in `SPEC.md` next to this crate. `main` is the
//! IO shell; every other module is a function from data to data.

// A command-line tool reports through stdout and stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod check;
mod config;
mod diagram;
mod error;
mod fix;
mod glob;
mod graph;
mod load;
mod manifest;
mod mermaid;
mod workspace;

use std::{
    fmt::Write,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;

use crate::error::{Error, Result};

const TEMPLATE: &str = include_str!("stow_template.toml");
const TEMPLATE_MINIMAL: &str = "[[namespaces]]\nname = \"myproject\"\n";

#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cargo {
    Stow(Args),
}

#[derive(clap::Args)]
#[command(
    version,
    about = "Workspace linting and structure validation for Rust monorepos"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Rewrite manifests to workspace form and sort them, then validate
    #[arg(long, conflicts_with = "graph")]
    fix: bool,

    /// Write the dependency graph as Mermaid text instead of validating
    #[arg(long, value_name = "FILE")]
    graph: Option<PathBuf>,

    /// Include test crates in the graph
    #[arg(long, requires = "graph")]
    include_tests: bool,

    /// Also print each crate as it is checked
    #[arg(short, long)]
    verbose: bool,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Write a template stow.toml at the workspace root
    Init {
        /// Write only the required keys
        #[arg(long)]
        minimal: bool,
    },
}

struct Outcome {
    stdout: String,
    success: bool,
}

fn main() -> ExitCode {
    let Cargo::Stow(args) = Cargo::parse();
    match run(&args) {
        Ok(outcome) => {
            print!("{}", outcome.stdout);
            if outcome.success {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<Outcome> {
    let cwd = std::env::current_dir().map_err(Error::CurrentDir)?;
    let metadata = load::metadata(&load::locate(&cwd)?)?;
    let root = metadata.workspace_root.as_std_path();

    if let Some(Command::Init { minimal }) = args.command {
        return init(root, minimal);
    }

    let config = load::config(root)?;
    let mut stdout = String::new();

    if args.fix {
        let workspace = load::workspace(&metadata, &config)?;
        for krate in &workspace.crates {
            let path = root.join(&krate.manifest);
            let fixed = fix::fix(&load::read(&path)?, &workspace, &config).map_err(|message| {
                Error::Parse {
                    path: path.clone(),
                    message,
                }
            })?;
            if let Some(text) = fixed {
                write(&path, &text)?;
                let _ = writeln!(stdout, "fixed {}", krate.manifest);
            }
        }
    }

    let workspace = load::workspace(&metadata, &config)?;

    if let Some(file) = &args.graph {
        let graph = graph::collect(&workspace, &config);
        let graph = graph::reduce(&graph, &workspace, &config, args.include_tests);
        let diagram = diagram::plan(&graph, &workspace, &config);
        write(&cwd.join(file), &mermaid::render(&diagram)?)?;
        return Ok(Outcome {
            stdout,
            success: true,
        });
    }

    if args.verbose {
        for krate in &workspace.crates {
            let _ = writeln!(stdout, "checking {}", krate.name);
        }
    }
    let diagnostics = check::check(&workspace, &config);
    stdout.push_str(&check::report(&diagnostics));
    Ok(Outcome {
        stdout,
        success: diagnostics.is_empty(),
    })
}

fn init(root: &Path, minimal: bool) -> Result<Outcome> {
    let path = root.join(load::CONFIG_FILE);
    if path.exists() {
        return Err(Error::ConfigExists(path));
    }
    write(&path, if minimal { TEMPLATE_MINIMAL } else { TEMPLATE })?;
    Ok(Outcome {
        stdout: format!("created {}\n", load::CONFIG_FILE),
        success: true,
    })
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|source| Error::Write {
        path: path.to_owned(),
        source,
    })
}
