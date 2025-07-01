use std::{fs, path::PathBuf};

use anyhow::Result;
use baml_runtime::baml_src_files;
use clap::Args;
use internal_baml_core::internal_baml_ast::{format_schema, FormatOptions};

#[derive(Args, Debug)]
pub struct FormatArgs {
    // default_value for --from is _usually_ the baml_src directory, but not for baml-cli fmt!
    #[arg(long, help = "path/to/baml_src", default_value = ".")]
    pub from: PathBuf,

    #[arg(
        help = "Specific files to format. If none provided, formats all files in the baml_src directory"
    )]
    pub paths: Vec<PathBuf>,

    #[arg(
        short = 'n',
        long = "dry-run",
        help = "Write formatter changes to stdout instead of files",
        default_value = "false"
    )]
    pub dry_run: bool,
}

impl FormatArgs {
    pub fn run(&self) -> Result<()> {
        let paths = if self.paths.is_empty() {
            // Usually this is done in commands.rs, but fmt is a special case
            // because it doesn't need to actually load the BAML runtime to parse
            // BAML files.
            // let from = BamlRuntime::parse_baml_src_path(&self.from)?;
            baml_src_files(&self.from)?
        } else {
            self.paths.clone()
        };

        for path in paths.iter() {
            let source = fs::read_to_string(path)?;
            match format_schema(
                &source,
                FormatOptions {
                    indent_width: 2,
                    fail_on_unhandled_rule: false,
                },
            ) {
                Ok(formatted) => {
                    if self.dry_run {
                        println!("{formatted}");
                    } else {
                        fs::write(path, formatted)?;
                    }
                }
                Err(e) => {
                    log::error!("Failed to format {}: {:?}", path.display(), e);
                }
            }
        }

        log::info!("Formatted {} files", paths.len());

        Ok(())
    }
}
