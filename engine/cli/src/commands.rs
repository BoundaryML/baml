use anyhow::Result;
use baml_runtime::{cli::RuntimeCliDefaults, BamlRuntime};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool for working with BAML. Learn more at https://docs.boundaryml.com.", long_about = None)]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
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
    #[command(about = "Initialize a new BAML project.")]
    Init(baml_runtime::cli::init::InitArgs),

    #[command(about = "Runs all generators in the baml_src directory")]
    Generate(baml_runtime::cli::generate::GenerateArgs),

    #[command(about = "Checks for errors and warnings in the baml_src directory")]
    Check(baml_runtime::cli::check::CheckArgs),

    #[command(about = "Starts a server that translates LLM responses to BAML responses")]
    Serve(baml_runtime::cli::serve::ServeArgs),

    #[command(about = "Starts a development server")]
    Dev(baml_runtime::cli::dev::DevArgs),

    #[command(subcommand, about = "Authenticate with Boundary Cloud", hide = true)]
    Auth(crate::auth::AuthCommands),

    #[command(
        about = "Login to Boundary Cloud (alias for `baml auth login`)",
        hide = true
    )]
    Login(crate::auth::LoginArgs),

    #[command(about = "Deploy a BAML project to Boundary Cloud", hide = true)]
    Deploy(crate::deploy::DeployArgs),

    #[command(about = "Format BAML source files", name = "fmt", hide = true)]
    Format(crate::format::FormatArgs),

    #[command(about = "Run BAML tests")]
    Test(baml_runtime::cli::testing::TestArgs),

    #[command(about = "Print HIR from BAML files")]
    DumpHIR(baml_runtime::cli::dump_intermediate::DumpIntermediateArgs),

    #[command(about = "Print Bytecode from BAML files")]
    DumpBytecode(baml_runtime::cli::dump_intermediate::DumpIntermediateArgs),

    #[command(about = "Starts a language server", name = "lsp")]
    LanguageServer(crate::lsp::LanguageServerArgs),

    #[command(about = "Start an interactive REPL for BAML expressions")]
    Repl(baml_runtime::cli::repl::ReplArgs),
}

impl RuntimeCli {
    pub fn run(&mut self, defaults: RuntimeCliDefaults) -> Result<crate::ExitCode> {
        use internal_baml_core::feature_flags::FeatureFlags;

        // Parse feature flags once at the root level
        let feature_flags = match FeatureFlags::from_vec(self.features.clone()) {
            Ok(flags) => flags,
            Err(errors) => {
                for error in errors {
                    eprintln!("Error: {error}");
                }
                eprintln!("\nAvailable feature flags:");
                eprintln!("  beta - Enable beta features and suppress experimental warnings");
                eprintln!("  display_all_warnings - Display all warnings in CLI output");
                return Ok(crate::ExitCode::Other);
            }
        };

        // Log enabled features
        if feature_flags.is_beta_enabled() {
            baml_log::info!("Beta features enabled - experimental warnings will be suppressed");
        }
        if feature_flags.should_display_warnings() {
            baml_log::info!("Warning display enabled - all warnings will be shown");
        }

        // NB: we spawn a runtime here but block_on inside the match arms
        // because 'baml-cli dev' and 'baml-cli serve' cannot block_on
        let t = tokio::runtime::Runtime::new()?;
        let _ = t.enter();

        match &mut self.command {
            Commands::Generate(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(defaults, feature_flags.clone()) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::Check(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(defaults, feature_flags.clone()) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::Init(args) => match args.run(defaults) {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(e) => {
                    eprintln!("Error: {e}");
                    Ok(crate::ExitCode::Other)
                }
            },
            Commands::Serve(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(feature_flags.clone()) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::Dev(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(defaults, feature_flags.clone()) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::Auth(args) => match t.block_on(async { args.run_async().await }) {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(e) => {
                    eprintln!("Error: {e}");
                    Ok(crate::ExitCode::Other)
                }
            },
            Commands::Login(args) => match t.block_on(async { args.run_async().await }) {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(e) => {
                    eprintln!("Error: {e}");
                    Ok(crate::ExitCode::Other)
                }
            },
            Commands::Deploy(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match t.block_on(async { args.run_async(feature_flags.clone()).await }) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::Format(args) => {
                // We deliberately don't apply parse_baml_src_path here
                // see format.rs for more details
                // args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run() {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Test(args) => {
                let res = t.block_on(async { args.run(feature_flags.clone()).await })?;
                match res {
                    baml_runtime::cli::testing::TestRunResult::Success => {
                        Ok(crate::ExitCode::Success)
                    }
                    baml_runtime::cli::testing::TestRunResult::HumanEvalRequired => {
                        Ok(crate::ExitCode::HumanEvalRequired)
                    }
                    baml_runtime::cli::testing::TestRunResult::TestFailure => {
                        Ok(crate::ExitCode::TestFailure)
                    }
                    baml_runtime::cli::testing::TestRunResult::TestCancelled => {
                        Ok(crate::ExitCode::TestCancelled)
                    }
                    baml_runtime::cli::testing::TestRunResult::NoTestsRun => {
                        Ok(crate::ExitCode::NoTestsRun)
                    }
                }
            }
            Commands::DumpHIR(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(
                    baml_runtime::cli::dump_intermediate::DumpType::HIR,
                    feature_flags.clone(),
                ) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::DumpBytecode(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(
                    baml_runtime::cli::dump_intermediate::DumpType::Bytecode,
                    feature_flags.clone(),
                ) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        Ok(crate::ExitCode::Other)
                    }
                }
            }
            Commands::LanguageServer(args) => match args.run() {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(_) => Ok(crate::ExitCode::Other),
            },
            Commands::Repl(args) => match t.block_on(async { args.run().await }) {
                Ok(()) => Ok(crate::ExitCode::Success),
                Err(e) => {
                    eprintln!("Error: {e}");
                    Ok(crate::ExitCode::Other)
                }
            },
        }
    }
}
