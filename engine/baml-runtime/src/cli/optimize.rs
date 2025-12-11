#![allow(clippy::print_stdout)]
/// CLI Options and top-level implementation for the
/// `baml-cli optimize` command.
use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};

use crate::{cli::dotenv, BamlRuntime};

#[derive(Args, Clone, Debug)]
pub struct OptimizeArgs {
    #[arg(long, help = "path/to/baml_src", default_value = ".", global = true)]
    pub from: PathBuf,

    #[arg(long, short = 'f', help = "Specific function(s) to optimize")]
    /// Optimize specific function(s). Can be specified multiple times.
    ///
    /// Examples:
    ///   --function ExtractReceipt
    ///   -f ExtractReceipt -f ClassifyEmail
    pub function: Vec<String>,

    #[arg(long, help = "Enable beta features")]
    pub beta: bool,

    #[arg(long, short = 't', help = "Test filter pattern")]
    /// Filter which tests to use for optimization.
    /// Uses the same syntax as `baml-cli test --include`.
    ///
    /// Examples:
    ///   --test "ExtractReceipt::*"
    ///   --test "::ImportantTest"
    pub test: Vec<String>,

    #[arg(
        long,
        default_value_t = 50,
        help = "Maximum number of test evaluations"
    )]
    pub max_evals: usize,

    #[arg(long, default_value_t = 20, help = "Number of optimization iterations")]
    pub trials: usize,

    #[arg(long, value_enum, help = "Auto-sized optimization budget")]
    /// Automatically set optimization budget based on preset:
    ///   light  - Quick exploration (6 candidates)
    ///   medium - Balanced (12 candidates)
    ///   heavy  - Thorough (18 candidates)
    pub auto: Option<AutoBudget>,

    #[arg(long, help = "Objective weights (e.g., accuracy=0.8,tokens=0.2)")]
    /// Multi-objective optimization weights.
    /// Supported objectives: accuracy, tokens, latency, prompt_tokens, completion_tokens
    ///
    /// Examples:
    ///   --weight accuracy=0.8,tokens=0.2
    ///   --weight accuracy=0.7,latency=0.2,prompt_tokens=0.1
    pub weight: Vec<String>,

    #[arg(long, help = "Resume from a previous optimization run")]
    /// Path to a previous optimization run directory to resume from.
    ///
    /// Example:
    ///   --resume .baml_optimize/run_20250106_143022
    pub resume: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "Reset GEPA prompts to defaults")]
    /// Reset the GEPA reflection prompts in .baml_optimize/gepa/ to the
    /// default versions bundled with this version of baml-cli.
    pub reset_gepa_prompts: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Apply winning candidate to source files"
    )]
    /// Automatically apply the best candidate's changes to your BAML source files.
    /// Without this flag, optimized candidates are saved to .baml_optimize/ only.
    ///
    /// The original files will be overwritten. Use version control (git) to review
    /// and revert changes if needed.
    pub apply: bool,

    #[arg(long, default_value_t = 4, help = "Number of parallel test executions")]
    pub parallel: usize,

    #[arg(long, help = "Custom output directory for optimization artifacts")]
    pub output_dir: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "Enable verbose output")]
    pub verbose: bool,

    #[arg(
        long,
        short = 'v',
        default_value_t = false,
        help = "View optimization results in TUI"
    )]
    /// Open an interactive TUI to browse optimization results.
    /// Use with --run-dir to view a specific run, or without to view the most recent run.
    ///
    /// Examples:
    ///   baml-cli optimize --view
    ///   baml-cli optimize --view --run-dir .baml_optimize/run_20250106_143022
    pub view: bool,

    #[arg(long, default_value_t = false, help = "Disable the live TUI viewer")]
    /// Disable the TUI viewer that normally launches during optimization.
    /// Use this for CI/CD pipelines or when you prefer text-only output.
    ///
    /// Example:
    ///   baml-cli optimize --beta -f MyFunction --no-ui
    pub no_ui: bool,

    #[arg(long, help = "Path to optimization run directory to view")]
    /// Specify a specific optimization run directory to view.
    /// Only used with --view flag.
    ///
    /// Example:
    ///   --run-dir .baml_optimize/run_20250106_143022
    pub run_dir: Option<PathBuf>,

    #[command(flatten)]
    pub dotenv: dotenv::DotenvArgs,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum AutoBudget {
    /// Quick exploration (6 candidates)
    Light,
    /// Balanced (12 candidates)
    Medium,
    /// Thorough (18 candidates)
    Heavy,
}

impl AutoBudget {
    pub fn trials(&self) -> usize {
        match self {
            AutoBudget::Light => 6,
            AutoBudget::Medium => 12,
            AutoBudget::Heavy => 18,
        }
    }
}

/// Result of an optimization run
pub enum OptimizeRunResult {
    /// Optimization completed successfully
    Success,
    /// No functions with tests found to optimize
    NoFunctionsToOptimize,
    /// Optimization was cancelled (e.g., Ctrl+C)
    Cancelled,
    /// Optimization failed
    Failed,
    /// GEPA prompts were reset (no optimization run)
    GepaPromptsReset,
    /// TUI viewer was opened and closed successfully
    ViewCompleted,
}

impl OptimizeArgs {
    pub async fn run(
        &self,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<OptimizeRunResult> {
        // Handle --view mode first (doesn't require beta flag)
        if self.view {
            return self.run_view_mode();
        }

        if !(self.beta) {
            println!(
                "`baml-cli optimize` is still in beta. Please use --beta flag and proceed with caution."
            );
            std::process::exit(1);
        }
        let from = BamlRuntime::parse_baml_src_path(&self.from)?;

        // Determine the optimization directory (parent of baml_src)
        let optimize_base_dir = from
            .parent()
            .map(|p| p.join(".baml_optimize"))
            .unwrap_or_else(|| PathBuf::from(".baml_optimize"));

        let gepa_dir = optimize_base_dir.join("gepa");

        // Create/update .gitignore in the parent directory to ignore .baml_optimize
        if let Some(parent_dir) = optimize_base_dir.parent() {
            let gitignore_path = parent_dir.join(".gitignore");

            // Read existing .gitignore or create empty string
            let mut gitignore_content = if gitignore_path.exists() {
                std::fs::read_to_string(&gitignore_path).unwrap_or_default()
            } else {
                String::new()
            };

            // Check if .baml_optimize is already in the .gitignore
            let entry = ".baml_optimize/";
            if !gitignore_content.lines().any(|line| line.trim() == entry) {
                // Add .baml_optimize/ to .gitignore
                if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
                    gitignore_content.push('\n');
                }
                gitignore_content.push_str(entry);
                gitignore_content.push('\n');

                let _ = std::fs::write(&gitignore_path, gitignore_content);
            }
        }

        // If --reset-gepa-prompts is specified alone, just reset and exit
        if self.reset_gepa_prompts {
            // Check if custom gepa.baml exists
            let baml_src_dir = gepa_dir.join("baml_src");
            let gepa_file = baml_src_dir.join("gepa.baml");

            if gepa_file.exists() {
                // Compute hash of existing files to see if they differ from defaults
                let current_hash = {
                    use std::{
                        collections::hash_map::DefaultHasher,
                        hash::{Hash, Hasher},
                    };

                    let gepa_content = std::fs::read_to_string(&gepa_file).unwrap_or_default();
                    let clients_content =
                        std::fs::read_to_string(baml_src_dir.join("clients.baml"))
                            .unwrap_or_default();

                    let mut hasher = DefaultHasher::new();
                    gepa_content.hash(&mut hasher);
                    clients_content.hash(&mut hasher);
                    format!("{:x}", hasher.finish())
                };

                let default_hash = crate::optimize::gepa_defaults::default_gepa_hash();

                // Only prompt if the existing files differ from defaults (i.e., have been customized)
                if current_hash != default_hash {
                    println!("This will erase your custom gepa.baml. Are you sure? [Y/n]");
                    let mut input = String::new();
                    std::io::stdin()
                        .read_line(&mut input)
                        .context("Failed to read user input")?;
                    let input = input.trim().to_lowercase();

                    // Accept Y, y, yes, or empty (default to yes)
                    if !input.is_empty() && input != "y" && input != "yes" {
                        println!("Cancelled.");
                        return Ok(OptimizeRunResult::Cancelled);
                    }
                }
            }

            println!("Resetting GEPA prompts to defaults...");
            crate::optimize::gepa_runtime::reset_gepa_prompts(&gepa_dir)?;
            println!("GEPA prompts reset successfully.");
            println!("  Location: {}", gepa_dir.join("baml_src").display());
            return Ok(OptimizeRunResult::GepaPromptsReset);
        }

        self.dotenv.load()?;

        // Suppress BAML logging and tracing when TUI is active
        if !self.no_ui {
            // Set BAML_LOG=off to disable all logging output
            std::env::set_var("BAML_LOG", "off");
            // Remove BAML_TRACE_FILE to prevent trace output during TUI
            std::env::remove_var("BAML_TRACE_FILE");
        }

        let env_vars = std::env::vars().collect::<HashMap<String, String>>();
        let runtime = BamlRuntime::from_directory(&from, env_vars.clone(), feature_flags.clone())?;
        let runtime = std::sync::Arc::new(runtime);

        // Resolve trials from --auto or --trials
        let trials = self.auto.map(|a| a.trials()).unwrap_or(self.trials);

        println!("Starting prompt optimization...");
        println!("  Source: {}", from.display());
        println!("  Trials: {}", trials);
        println!("  Max evaluations: {}", self.max_evals);
        println!("  Parallel: {}", self.parallel);

        if !self.function.is_empty() {
            println!("  Functions: {}", self.function.join(", "));
        }
        if !self.test.is_empty() {
            println!("  Test filter: {}", self.test.join(", "));
        }

        // Initialize the GEPA runtime
        let gepa_runtime = match crate::optimize::gepa_runtime::GEPARuntime::new(
            &gepa_dir,
            env_vars.clone(),
            false, // Don't reset here, we handle it above
            feature_flags.clone(),
        ) {
            Ok(runtime) => runtime,
            Err(e) => {
                // Provide detailed error information
                eprintln!("\nFailed to initialize GEPA runtime.");
                eprintln!("GEPA directory: {}", gepa_dir.display());
                eprintln!("\nError details:");
                for (i, cause) in e.chain().enumerate() {
                    if i == 0 {
                        eprintln!("  {}", cause);
                    } else {
                        eprintln!("  Caused by: {}", cause);
                    }
                }
                eprintln!("\nTroubleshooting:");
                eprintln!("  1. Try running: baml-cli optimize --reset-gepa-prompts");
                eprintln!(
                    "  2. Check the BAML files in: {}",
                    gepa_dir.join("baml_src").display()
                );
                eprintln!("  3. Ensure you have a valid client configuration");
                return Err(e.context("Failed to initialize GEPA runtime"));
            }
        };

        // Check for version mismatch
        match gepa_runtime.check_version() {
            crate::optimize::gepa_runtime::VersionStatus::Current => {}
            crate::optimize::gepa_runtime::VersionStatus::Outdated { current, bundled } => {
                eprintln!("\nWarning: Your GEPA implementation is from baml-cli {current}");
                eprintln!("         Current baml-cli version is {bundled}");
                eprintln!("         Run 'baml-cli optimize --reset-gepa-prompts' to upgrade\n");
            }
            crate::optimize::gepa_runtime::VersionStatus::Modified => {
                if self.verbose {
                    println!("Note: Using customized GEPA implementation");
                }
            }
        }

        // Parse objective weights
        let objectives = crate::optimize::pareto::parse_weight_args(&self.weight)?;

        // Create or resume optimization run
        let run_dir = if let Some(resume_path) = &self.resume {
            println!("Resuming optimization from: {}", resume_path.display());
            resume_path.clone()
        } else {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            self.output_dir
                .clone()
                .unwrap_or_else(|| optimize_base_dir.join(format!("run_{}", timestamp)))
        };

        let storage = crate::optimize::storage::OptimizationStorage::new(&run_dir)
            .context("Failed to create optimization storage")?;

        // Build test filter
        let test_filter = crate::test_executor::TestFilter::from(
            self.test.iter().map(|s| s.as_str()),
            std::iter::empty::<&str>(),
        );

        // Create the orchestrator
        let mut orchestrator = crate::optimize::orchestrator::GEPAOrchestrator::new(
            runtime.clone(),
            gepa_runtime,
            storage,
            crate::optimize::orchestrator::OrchestratorConfig {
                function_filter: self.function.clone(),
                test_filter,
                trials,
                max_evals: self.max_evals,
                parallel: self.parallel,
                objectives,
                verbose: self.verbose,
                quiet: !self.no_ui, // Suppress output when TUI is active
                env_vars,
                baml_src_path: from.clone(),
                feature_flags,
            },
        )?;

        // Launch live TUI in a separate thread (default behavior, unless --no-ui)
        let tui_handle = if !self.no_ui {
            let run_dir_clone = run_dir.clone();
            println!("Launching live TUI viewer...");
            println!("(Press 'q' to close TUI, 'Enter' to stop optimization and apply selected candidate)\n");

            // Give a moment for the storage directory to be fully created
            std::thread::sleep(std::time::Duration::from_millis(100));

            Some(std::thread::spawn(move || {
                if let Err(e) = crate::optimize::tui::run_tui_live(&run_dir_clone) {
                    eprintln!("TUI error: {}", e);
                }
            }))
        } else {
            None
        };

        // Run optimization
        match orchestrator.run().await {
            Ok(result) => {
                // If TUI was launched in live mode, wait for it to close and get the selected candidate
                if let Some(handle) = tui_handle {
                    // Wait for TUI to close - user can press Enter to apply a candidate
                    let tui_result = handle.join();

                    // Check if user selected a candidate to apply via TUI
                    if let Ok(()) = tui_result {
                        // Read back the apply request from storage
                        if let Some(candidate_id) =
                            crate::optimize::tui::read_apply_request(&run_dir)
                        {
                            println!(
                                "\nApplying candidate #{} from TUI selection...",
                                candidate_id
                            );
                            self.apply_candidate(&from, &runtime, &result, candidate_id)?;
                        } else {
                            println!("\nNo candidate selected for application.");
                            println!("Results saved to: {}", run_dir.display());
                        }
                    }

                    return Ok(OptimizeRunResult::Success);
                }

                // Non-live mode: show completion message and Pareto selection
                println!("\n{}", "=".repeat(60));
                println!("Optimization Complete!");
                println!("{}", "=".repeat(60));

                // Load objectives for display
                let objectives = result
                    .storage
                    .load_config()
                    .map(|c| c.objectives)
                    .unwrap_or_default();

                // Display Pareto frontier and let user select
                let selected_id = if result.pareto_frontier_size() > 0 {
                    crate::optimize::tui::display_pareto_and_select(
                        &result.candidates,
                        &result.pareto_frontier,
                        &objectives,
                        &result.function_name,
                    )
                } else if let Some(best) = result.best_candidate() {
                    // Fallback to best candidate if no Pareto frontier
                    println!("\nBest candidate: #{}", best.id);
                    println!(
                        "  Test pass rate: {:.1}%",
                        best.scores
                            .as_ref()
                            .map(|s| s.test_pass_rate * 100.0)
                            .unwrap_or(0.0)
                    );
                    Some(best.id)
                } else {
                    None
                };

                // Apply the selected candidate
                if let Some(candidate_id) = selected_id {
                    println!(
                        "\nCandidate #{} saved to: {}",
                        candidate_id,
                        result.candidate_path(candidate_id).display()
                    );

                    if self.apply {
                        self.apply_candidate(&from, &runtime, &result, candidate_id)?;
                    } else {
                        println!("\nTo apply this optimization, either:");
                        println!("  1. Re-run with --apply flag to write changes directly");
                        println!("  2. Manually copy changes from the candidate file above");
                    }
                } else {
                    println!("\nNo candidate selected for application.");
                    println!("Results saved to: {}", run_dir.display());
                }

                Ok(OptimizeRunResult::Success)
            }
            Err(e) => {
                // If TUI was launched, wait for it to close even on error
                if let Some(handle) = tui_handle {
                    let _ = handle.join();
                }

                eprintln!("\nOptimization failed: {e}");
                Ok(OptimizeRunResult::Failed)
            }
        }
    }

    /// Apply a candidate's changes to source files
    fn apply_candidate(
        &self,
        baml_src_path: &std::path::Path,
        runtime: &std::sync::Arc<BamlRuntime>,
        result: &crate::optimize::orchestrator::OptimizationRunResult,
        candidate_id: usize,
    ) -> Result<()> {
        let candidate = result
            .candidates
            .iter()
            .find(|c| c.id == candidate_id)
            .context(format!("Candidate #{} not found", candidate_id))?;

        // Create an ImprovedFunction from the candidate
        let improved = crate::optimize::candidate::ImprovedFunction {
            prompt_text: candidate.function.prompt_text.clone(),
            classes: candidate.function.classes.clone(),
            enums: candidate.function.enums.clone(),
            rationale: String::new(),
        };

        // Generate the changes
        let changes = crate::optimize::applier::apply_to_source_files(
            baml_src_path,
            runtime,
            &candidate.function.function_name,
            &improved,
        )?;

        if changes.is_empty() {
            println!("\nNo changes to apply (candidate is same as original).");
            return Ok(());
        }

        // Show diff
        println!("\n{}", "=".repeat(60));
        println!("Changes to apply:");
        println!("{}", "=".repeat(60));
        for change in &changes {
            println!("\n{}", change.diff());
        }

        // Write changes
        println!("\n{}", "=".repeat(60));
        println!("Applying changes...");
        crate::optimize::applier::write_changes_to_disk(&changes)?;

        for change in &changes {
            println!("  Updated: {}", change.relative_path);
        }
        println!("\nChanges applied successfully!");
        println!("Use 'git diff' to review changes, or 'git checkout .' to revert.");

        Ok(())
    }

    /// Run the TUI viewer mode
    fn run_view_mode(&self) -> Result<OptimizeRunResult> {
        let run_dir = if let Some(ref dir) = self.run_dir {
            // Use explicitly specified run directory
            if !dir.exists() {
                anyhow::bail!("Specified run directory does not exist: {}", dir.display());
            }
            dir.clone()
        } else {
            // Find the most recent run in .baml_optimize/
            let from = BamlRuntime::parse_baml_src_path(&self.from)?;
            let optimize_base_dir = from
                .parent()
                .map(|p| p.join(".baml_optimize"))
                .unwrap_or_else(|| PathBuf::from(".baml_optimize"));

            if !optimize_base_dir.exists() {
                anyhow::bail!(
                    "No optimization runs found. Run 'baml-cli optimize' first.\n\
                     Expected directory: {}",
                    optimize_base_dir.display()
                );
            }

            // Find the most recent run_* directory
            let mut runs: Vec<_> = std::fs::read_dir(&optimize_base_dir)?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.is_dir() {
                        let name = path.file_name()?.to_string_lossy().to_string();
                        if name.starts_with("run_") {
                            return Some(path);
                        }
                    }
                    None
                })
                .collect();

            if runs.is_empty() {
                anyhow::bail!(
                    "No optimization runs found in {}.\n\
                     Run 'baml-cli optimize' first to create an optimization run.",
                    optimize_base_dir.display()
                );
            }

            // Sort by name (which includes timestamp) and take the most recent
            runs.sort();
            runs.pop().unwrap()
        };

        println!("Opening optimization viewer for: {}", run_dir.display());

        // Launch the TUI
        crate::optimize::tui::run_tui(&run_dir)?;

        Ok(OptimizeRunResult::ViewCompleted)
    }
}
