use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use baml_db::discover_baml_files;
use baml_fmt::FormatOptions;
use clap::Args;

use crate::{project_load::resolve_project_layout, reporter::Reporter};

/// Format BAML source files.
///
/// With explicit paths, formats those files or directories. With no paths,
/// discovers the nearest BAML project and formats all of its `.baml` files.
/// If no project is found, the command succeeds without changing anything.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Format the nearest project:
    baml fmt

  Format a specific file:
    baml fmt baml_src/main.baml

  Preview formatted output:
    baml fmt --dry-run

  Fail if any file needs formatting, without writing (for CI):
    baml fmt --check")]
pub struct FormatArgs {
    #[arg(
        help = "Specific files to format. If omitted, all `.baml` files in the project are formatted."
    )]
    pub paths: Vec<PathBuf>,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    #[arg(
        short = 'n',
        long = "dry-run",
        help = "Write formatter changes to stdout instead of files.",
        default_value = "false",
        help_heading = "Output options"
    )]
    pub dry_run: bool,

    #[arg(
        long = "check",
        help = "Write nothing; list files that would be reformatted and exit with status 1 if there are any.",
        conflicts_with = "dry_run",
        help_heading = "Output options"
    )]
    pub check: bool,
}

impl FormatArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        // Cargo-style default: with no positional paths, discover every
        // `.baml` file under the project root and format the lot. The
        // project-marker rule keeps `baml fmt` from silently rewriting
        // every `.baml` under cwd from an unrelated directory; with no
        // marker there's simply nothing to format (a no-op success).
        let paths = if self.paths.is_empty() {
            match discover_project_files(self.from.as_deref())? {
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
                self.from
                    .as_deref()
                    .unwrap_or_else(|| Path::new("."))
                    .display()
                    .to_string()
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

        let reporter = Reporter::new();
        // In dry-run and check mode stdout carries the result (formatted
        // source, or the unformatted paths), so skip the per-file
        // `Formatting <path>` lines.
        let show_progress = !self.dry_run && !self.check;

        let mut num_failures: usize = 0;
        let mut num_unformatted: usize = 0;
        for path in &paths {
            if show_progress {
                reporter.spin("Formatting", path.display().to_string());
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
                    if self.check {
                        if formatted != source {
                            num_unformatted += 1;
                            #[allow(clippy::print_stdout)]
                            {
                                println!("{}", path.display());
                            }
                        }
                    } else if self.dry_run {
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
        let verb = if self.check { "checked" } else { "formatted" };
        if num_failures > 0 {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "{verb} {ok} of {total} file(s); {num_failures} failed"
            ));
            Ok(crate::ExitCode::Other)
        } else if num_unformatted > 0 {
            crate::reporter::print_error(format_args!(
                "{num_unformatted} of {total} file(s) would be reformatted; run `baml fmt` without `--check` to fix"
            ));
            Ok(crate::ExitCode::Unformatted)
        } else if self.check {
            reporter.finish("Finished", format!("{total} file(s) already formatted"));
            Ok(crate::ExitCode::Success)
        } else {
            if !self.dry_run {
                reporter.finish("Finished", format!("formatted {ok} file(s)"));
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

/// Walk a resolved source root and return every `.baml` file inside it.
/// Omitted `--from` requires a `baml.toml` or `baml_src/` marker so `baml fmt`
/// doesn't accidentally rewrite every `.baml` below an unrelated cwd.
/// An explicit `--from` is itself a safe opt-in to format that source tree.
///
/// Returns `Ok(None)` when neither marker is present. The caller turns `None`
/// into a no-op success rather than a hard error.
fn discover_project_files(from: Option<&Path>) -> Result<Option<Vec<PathBuf>>> {
    let Some(layout) = resolve_project_layout(from)? else {
        return Ok(None);
    };
    Ok(Some(discover_baml_files(&layout.source_root)))
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
            from: Some(tmp.path().to_path_buf()),
            dry_run: false,
            check: false,
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

    #[test]
    fn default_discovery_walks_up_from_nested_dir() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"fmt-test\"\n",
        )
        .unwrap();
        let nested_dir = tmp.path().join("baml_src/nested");
        fs::create_dir_all(&nested_dir).unwrap();
        let main = tmp.path().join("baml_src/main.baml");
        fs::write(&main, "function main() -> string { \"hello\" }\n").unwrap();
        let main = fs::canonicalize(main).unwrap();

        let files = discover_project_files(Some(&nested_dir)).unwrap().unwrap();
        assert_eq!(files, vec![main]);
    }

    #[test]
    fn explicit_sibling_source_is_not_redirected_to_primary_baml_src() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"fmt-test\"\n",
        )
        .unwrap();
        let primary = tmp.path().join("baml_src");
        let alternate = tmp.path().join("baml_src_temp2");
        fs::create_dir(&primary).unwrap();
        fs::create_dir(&alternate).unwrap();
        fs::write(
            primary.join("primary.baml"),
            "function primary() -> int { 1 }\n",
        )
        .unwrap();
        let alternate_file = alternate.join("alternate.baml");
        fs::write(&alternate_file, "function alternate() -> int { 2 }\n").unwrap();

        let files = discover_project_files(Some(&alternate)).unwrap().unwrap();
        assert_eq!(files, vec![fs::canonicalize(alternate_file).unwrap()]);
    }
}
