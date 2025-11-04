mod app;
mod compiler;
mod ui;
mod watcher;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "baml_onionskin")]
#[command(about = "A live TUI for exploring BAML compiler phases with snapshot diffing")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to the BAML file or directory to watch (for TUI mode)
    #[arg(long = "from")]
    path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Test incremental compilation by comparing before/after states
    /// Skips running the TUI and just dumps compiler phase outputs.
    Increment {
        /// Path to the "before" state directory/file
        #[arg(long)]
        before: PathBuf,

        /// Path to the "after" state directory/file  
        #[arg(long)]
        after: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Command::Increment { before, after }) => run_increment_test(before, after),
        None => {
            let path = args
                .path
                .ok_or_else(|| anyhow::anyhow!("--from is required for TUI mode"))?;

            // Validate path exists
            if !path.exists() {
                anyhow::bail!("Path does not exist: {}", path.display());
            }

            // Initialize terminal
            let mut terminal = ui::init_terminal()?;

            // Create and run the app
            let mut app = app::App::new(path)?;
            let result = app.run(&mut terminal);

            // Restore terminal
            ui::restore_terminal(&mut terminal)?;

            result
        }
    }
}

fn run_increment_test(before: PathBuf, after: PathBuf) -> Result<()> {
    use compiler::{CompilerPhase, CompilerRunner, read_files_from_disk};

    if !before.is_dir() || !after.is_dir() {
        anyhow::bail!("Both --before and --after must be directories for increment testing");
    }

    println!("=== INCREMENTAL COMPILATION TEST ===\n");
    println!("This test simulates:");
    println!("1. Fresh compilation of 'before' directory");
    println!("2. File modification (reading 'after' files)");
    println!("3. Incremental recompilation with 'before' as snapshot\n");

    // Step 1: Read "before" files (snapshot)
    println!("Step 1: Fresh compilation (BEFORE state)");
    println!("----------------------------------------");
    let before_files = read_files_from_disk(&before)?;

    let mut compiler = CompilerRunner::new(&before);
    compiler.compile_from_filesystem(&before_files, None)?;

    let before_metrics = compiler.get_metrics_output()?;
    println!("{}\n", before_metrics);

    // Step 2: Read "after" files
    println!("Step 2: Simulating file changes");
    println!("--------------------------------");
    let after_files = read_files_from_disk(&after)?;

    // Find changed files
    for (path, after_content) in &after_files {
        if let Some(before_content) = before_files.get(path) {
            if before_content != after_content {
                println!("  Modified: {}", path.display());
            }
        } else {
            println!("  Added: {}", path.display());
        }
    }
    println!();

    // Step 3: Compile "after" state using "before" as snapshot
    println!("Step 3: Incremental compilation (AFTER modification on same DB)");
    println!("----------------------------------------------------------------");
    compiler.compile_from_filesystem(&after_files, Some(&before_files))?;

    let after_metrics = compiler.get_metrics_output()?;
    println!("{}\n", after_metrics);

    // Step 4: Show annotated compiler outputs
    println!("Step 4: Compiler Output with Cache Status (Lexer + Parser only)");
    println!("------------------------------------------------------------------");

    for &phase in &[
        CompilerPhase::Lexer,
        CompilerPhase::Parser,
        CompilerPhase::Hir,
    ] {
        println!("\n### {} ###", phase.name());
        let annotated = compiler.get_annotated_output(phase);

        // Show first 40 lines
        for (line, status) in annotated.iter().take(40) {
            let marker = match status {
                compiler::LineStatus::Recomputed => "(red)",
                compiler::LineStatus::Cached => "(green)",
                compiler::LineStatus::Unknown => "(white)",
            };
            println!("{} {}", marker, line);
        }
    }

    println!("\nDone!");

    Ok(())
}
