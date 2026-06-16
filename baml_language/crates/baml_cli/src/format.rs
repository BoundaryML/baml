use std::{
    collections::HashSet,
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
    /// passed. Mirrors `baml run`/`baml pack`'s `--from`. When the
    /// directory has neither a `baml.toml` nor a `baml_src/` subdirectory,
    /// there's nothing to format and `baml fmt` is a no-op success.
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
        // `.baml` file under the project root and format the lot. The
        // project-marker rule keeps `baml fmt` from silently rewriting
        // every `.baml` under cwd from an unrelated directory; with no
        // marker there's simply nothing to format (a no-op success).
        let paths = if self.paths.is_empty() {
            match discover_project_files(&self.from)? {
                Some(files) => files,
                None => {
                    // No `baml.toml` / `baml_src/` here — there's nothing to
                    // format, so don't fail. A no-op success beats a hard
                    // error for a command agents run reflexively; pass
                    // explicit file paths to format loose `.baml` files.
                    Reporter::new().finish("Finished", "no BAML project found; nothing to format");
                    return Ok(crate::ExitCode::Success);
                }
            }
        } else {
            expand_explicit_paths(&self.paths)
        };

        if paths.is_empty() {
            let search_root = if self.paths.is_empty() {
                self.from.display().to_string()
            } else {
                self.paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            crate::reporter::print_error(format_args!("no .baml files found in {}", search_root));
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
        for path in &paths {
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

fn expand_explicit_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::new();

    for path in paths {
        if path.is_dir() {
            expanded.extend(discover_baml_files(path));
        } else {
            expanded.push(path.clone());
        }
    }

    let mut seen = HashSet::new();
    expanded.retain(|path| seen.insert(path.clone()));
    expanded
}

/// Walk a project root and return every `.baml` file inside it. Requires a
/// `baml.toml` or `baml_src/` marker so `baml fmt` doesn't accidentally
/// rewrite every `.baml` under cwd from an unrelated directory, and prefers
/// the `baml_src/` subtree when present so loose top-level fixtures aren't
/// touched.
///
/// Returns `Ok(None)` when neither marker is present — `baml fmt` mutates
/// files, so unlike the read-only `describe`/`grep` introspection path it
/// does **not** walk up to adopt a distant ancestor project (that could
/// silently rewrite files far from the cwd). The caller turns `None` into a
/// no-op success rather than a hard error.
fn discover_project_files(from: &Path) -> Result<Option<Vec<PathBuf>>> {
    let canonical = fs::canonicalize(from)
        .with_context(|| format!("Could not resolve path: {}", from.display()))?;

    let has_baml_toml = canonical.join("baml.toml").exists();
    let baml_src = canonical.join("baml_src");
    let has_baml_src = baml_src.is_dir();
    if !has_baml_toml && !has_baml_src {
        return Ok(None);
    }

    let walk_root = if has_baml_src { baml_src } else { canonical };
    Ok(Some(discover_baml_files(&walk_root)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn explicit_directory_formats_baml_files_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        let baml_src = tmp.path().join("baml_src");
        let nested_dir = baml_src.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();

        let main = baml_src.join("main.baml");
        let nested = nested_dir.join("nested.baml");
        let ignored = nested_dir.join("ignored.txt");
        let main_source = "function main() -> string { \"hello\" }\n";
        let nested_source = "function nested() -> int { 1 }\n";
        let ignored_source = "function ignored() -> int { 1 }\n";

        fs::write(&main, main_source).unwrap();
        fs::write(&nested, nested_source).unwrap();
        fs::write(&ignored, ignored_source).unwrap();

        let args = FormatArgs {
            paths: vec![baml_src],
            from: tmp.path().to_path_buf(),
            dry_run: false,
        };
        let exit_code = args.run().unwrap();

        assert!(matches!(exit_code, crate::ExitCode::Success));
        assert_eq!(
            fs::read_to_string(&main).unwrap(),
            baml_fmt::format(main_source, &FormatOptions::default()).unwrap()
        );
        assert_eq!(
            fs::read_to_string(&nested).unwrap(),
            baml_fmt::format(nested_source, &FormatOptions::default()).unwrap()
        );
        assert_eq!(fs::read_to_string(ignored).unwrap(), ignored_source);
    }

    #[test]
    fn explicit_overlapping_paths_are_deduplicated() {
        let tmp = tempfile::tempdir().unwrap();
        let baml_src = tmp.path().join("baml_src");
        let nested_dir = baml_src.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();

        let main = baml_src.join("main.baml");
        let nested = nested_dir.join("nested.baml");
        fs::write(&main, "function main() -> string { \"hello\" }\n").unwrap();
        fs::write(&nested, "function nested() -> int { 1 }\n").unwrap();

        let expanded = expand_explicit_paths(&[
            baml_src.clone(),
            main.clone(),
            nested_dir.clone(),
            nested.clone(),
        ]);

        assert_eq!(expanded, vec![main, nested]);
    }
}
