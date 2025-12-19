mod app;
mod compiler;
mod ui;
mod watcher;

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};

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
        Some(Command::Increment { before, after }) => run_increment_test(&before, &after),
        None => {
            let path = args
                .path
                .ok_or_else(|| anyhow::anyhow!("--from is required for TUI mode"))?;

            // Validate path exists
            if !path.exists() {
                anyhow::bail!("Path does not exist: {}", path.display());
            }

            // Set up panic hook to restore terminal
            let original_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic_info| {
                // Restore terminal before showing panic
                let _ = crossterm::terminal::disable_raw_mode();
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::terminal::LeaveAlternateScreen,
                    crossterm::event::DisableMouseCapture
                );
                // Then call the original panic handler
                original_hook(panic_info);
            }));

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

/// Run the compiler in headless mode (no TUI), and dump the results to the console.
/// Since there is no TUI no file watching cycle, we need to point to two separate
/// directories representing the before and after states.
fn run_increment_test(before: &Path, after: &Path) -> Result<()> {
    use compiler::{
        CompilerPhase, CompilerRunner, normalize_files_to_virtual_root, read_files_from_disk,
    };

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
    let start = std::time::Instant::now();
    let before_files = normalize_files_to_virtual_root(read_files_from_disk(before)?, before);

    let mut compiler = CompilerRunner::new(before);
    compiler.compile_from_filesystem(&before_files, None);

    let before_metrics = compiler.get_metrics_output();
    println!("{before_metrics}");
    eprintln!("[TIMING] Step 1 total: {:?}\n", start.elapsed());

    // Step 2: Read "after" files
    println!("Step 2: Simulating file changes");
    println!("--------------------------------");
    let after_files = normalize_files_to_virtual_root(read_files_from_disk(after)?, after);

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
    let start = std::time::Instant::now();
    compiler.compile_from_filesystem(&after_files, Some(&before_files));

    let after_metrics = compiler.get_metrics_output();
    println!("{after_metrics}");
    eprintln!("[TIMING] Step 3 total: {:?}\n", start.elapsed());

    // Step 4: Show annotated compiler outputs
    println!("Step 4: Compiler Output with Cache Status");
    println!("------------------------------------------------------------------");

    for &phase in &[
        CompilerPhase::Lexer,
        CompilerPhase::Parser,
        CompilerPhase::Ast,
        CompilerPhase::Hir,
        CompilerPhase::Thir,
        CompilerPhase::Diagnostics,
        CompilerPhase::Codegen,
    ] {
        println!("\n### {} ###", phase.name());
        let annotated = compiler.get_annotated_output(phase);

        // Show all lines (no limit)
        for (line, status) in annotated.iter() {
            let marker = match status {
                compiler::LineStatus::Recomputed => "(red)",
                compiler::LineStatus::Cached => "(green)",
                compiler::LineStatus::Unknown => "(white)",
            };
            println!("{marker} {line}");
        }
    }

    println!("\nDone!");

    Ok(())
}
