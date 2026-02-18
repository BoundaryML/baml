use std::{fs, path::PathBuf};

use anyhow::Result;
use baml_fmt::FormatOptions;
// use baml_runtime::baml_src_files;
use clap::Args;

#[derive(Args, Debug)]
pub struct FormatArgs {
    // // default_value for --from is _usually_ the baml_src directory, but not for baml-cli fmt!
    // #[arg(long, help = "path/to/baml_src", default_value = ".")]
    // pub from: PathBuf,

    // #[arg(
    //     help = "Specific files to format. If none provided, formats all files in the baml_src directory"
    // )]
    #[arg(help = "Specific files to format.")]
    pub paths: Vec<PathBuf>,

    #[arg(
        short = 'n',
        long = "dry-run",
        help = "Write formatter changes to stdout instead of files.",
        default_value = "false"
    )]
    pub dry_run: bool,
}

impl FormatArgs {
    #[allow(clippy::print_stderr)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        if self.paths.is_empty() {
            return Ok(crate::ExitCode::Success);
        }

        let mut num_failures: usize = 0;
        for path in &self.paths {
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(err) => {
                    eprintln!("Failed to read {path:?}: {err}");
                    num_failures += 1;
                    continue;
                }
            };
            let options = FormatOptions::default();
            match baml_fmt::format(&source, &options) {
                Ok(formatted) =>
                {
                    #[allow(clippy::print_stdout)]
                    if self.dry_run {
                        println!("{formatted}");
                    } else if let Err(err) = fs::write(path, formatted) {
                        eprintln!("Failed to write formatted source to {path:?}: {err}");
                        num_failures += 1;
                    }
                }
                Err(err) => {
                    match err {
                        baml_fmt::FormatterError::ParseErrors(err) => {
                            eprintln!("Error formatting {}: {err:?}", path.display());
                        }
                        baml_fmt::FormatterError::StrongAstError(err) => {
                            let err = err.print_with_file_context(path, &source);
                            eprintln!("Error while formatting: {err}");
                        }
                    }
                    num_failures += 1;
                }
            }
        }

        if num_failures > 0 {
            eprintln!(
                "Successfully formatted {} files, but failed to format {} files",
                self.paths.len() - num_failures,
                num_failures
            );
            Ok(crate::ExitCode::Other)
        } else {
            eprintln!("Successfully formatted {} files", self.paths.len());
            Ok(crate::ExitCode::Success)
        }
    }
}
