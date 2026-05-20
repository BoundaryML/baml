use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use baml_fmt::FormatOptions;
use baml_workspace::discover_baml_files;
use clap::Args;

use crate::reporter::Reporter;

#[derive(Args, Debug)]
pub struct FormatArgs {
    #[arg(
        help = "Specific files to format. If omitted, all `.baml` files in the project are formatted."
    )]
    pub paths: Vec<PathBuf>,

    /// Project root to discover files from when no explicit paths are
    /// passed. Mirrors `baml run`/`baml pack`'s `--from`; the directory
    /// must contain a `baml.toml` or a `baml_src/` subdirectory.
    #[arg(long, default_value = ".")]
    pub from: PathBuf,

    #[arg(
        short = 'n',
        long = "dry-run",
        help = "Write formatter changes to stdout instead of files.",
        default_value = "false"
    )]
    pub dry_run: bool,
}

impl FormatArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        // Cargo-style default: with no positional paths, discover every
        // `.baml` file under the project root and format the lot. Same
        // project-marker rule the rest of the CLI uses, so `baml fmt`
        // in an unrelated directory fails fast instead of silently
        // walking the cwd subtree.
        let discovered;
        let paths: &[PathBuf] = if self.paths.is_empty() {
            discovered = discover_project_files(&self.from)?;
            &discovered
        } else {
            &self.paths
        };

        if paths.is_empty() {
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                self.from.display()
            ));
            return Ok(crate::ExitCode::Other);
        }

        // In dry-run mode the formatted source goes to stdout — don't
        // start a spinner that would compete with it. Outside dry-run
        // we get the standard cargo-style verb sequence (one
        // `Formatting <path>` per file, persisting to scrollback like
        // cargo's `Compiling foo v0.1.0` lines).
        let reporter = if self.dry_run {
            None
        } else {
            Some(Reporter::new())
        };

        let mut num_failures: usize = 0;
        for path in paths {
            if let Some(r) = &reporter {
                r.spin("Formatting", path.display().to_string());
            }
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(err) => {
                    crate::reporter::print_error(format_args!(
                        "failed to read {}: {err}",
                        path.display()
                    ));
                    num_failures += 1;
                    continue;
                }
            };
            let options = FormatOptions::default();
            match baml_fmt::format(&source, &options) {
                Ok(formatted) => {
                    if self.dry_run {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("{formatted}");
                        }
                    } else if let Err(err) = fs::write(path, formatted) {
                        crate::reporter::print_error(format_args!(
                            "failed to write formatted source to {}: {err}",
                            path.display()
                        ));
                        num_failures += 1;
                    }
                }
                Err(err) => {
                    match err {
                        baml_fmt::FormatterError::ParseErrors(err) => {
                            crate::reporter::print_error(format_args!(
                                "formatting {}: {err:?}",
                                path.display()
                            ));
                        }
                        baml_fmt::FormatterError::StrongAstError(err) => {
                            let err = err.print_with_file_context(path, &source);
                            crate::reporter::print_error(format_args!("while formatting: {err}"));
                        }
                    }
                    num_failures += 1;
                }
            }
        }

        let total = paths.len();
        let ok = total - num_failures;
        if num_failures > 0 {
            if let Some(r) = &reporter {
                r.abandon();
            }
            crate::reporter::print_error(format_args!(
                "formatted {ok} of {total} file(s); {num_failures} failed"
            ));
            Ok(crate::ExitCode::Other)
        } else {
            if let Some(r) = &reporter {
                r.finish("Finished", format!("formatted {ok} file(s)"));
            }
            Ok(crate::ExitCode::Success)
        }
    }
}

/// Walk a project root and return every `.baml` file inside it. Same
/// rules as `project_load::load_project_from`: require a `baml.toml` or
/// `baml_src/` marker so `baml fmt` doesn't accidentally rewrite every
/// `.baml` under cwd from an unrelated directory, and prefer the
/// `baml_src/` subtree when present so loose top-level fixtures aren't
/// touched.
fn discover_project_files(from: &Path) -> Result<Vec<PathBuf>> {
    let canonical = fs::canonicalize(from)
        .with_context(|| format!("Could not resolve path: {}", from.display()))?;

    let has_baml_toml = canonical.join("baml.toml").exists();
    let baml_src = canonical.join("baml_src");
    let has_baml_src = baml_src.is_dir();
    if !has_baml_toml && !has_baml_src {
        anyhow::bail!(
            "`{}` doesn't look like a BAML project.\n\
             Expected `baml.toml` or a `baml_src/` directory at the project root.\n\
             Pass `--from <project-dir>` or specific files to format.",
            canonical.display()
        );
    }

    let walk_root = if has_baml_src { baml_src } else { canonical };
    Ok(discover_baml_files(&walk_root))
}
