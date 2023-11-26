use std::path::PathBuf;

use include_dir::{include_dir, Dir};
use log::info;

use crate::{builder::get_baml_src, command::run_command_with_error, errors::CliError};

const SAMPLE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/sample");

pub fn init_command() -> Result<(), CliError> {
    if let Ok(_) = get_baml_src(&None) {
        return Err("Already in a baml project".into());
    }

    // Copy every file/dir in SAMPLE_DIR to the current directory.
    let cwd = PathBuf::from(std::env::current_dir().unwrap());
    for file in SAMPLE_DIR.find("**/*").unwrap() {
        let target = cwd.join(file.path().to_path_buf());
        match file.as_file() {
            Some(file) => {
                let content = file.contents();
                // Make sure the target directory exists.
                let _ = std::fs::create_dir_all(target.parent().unwrap());
                // If the pyproject.toml file already exists, don't overwrite it.
                if target.ends_with("pyproject.toml") && target.exists() {
                    match run_command_with_error("poetry", &["add", "baml"], "poetry add baml") {
                        Ok(_) => {}
                        Err(e) => {
                            info!("{}", e);
                        }
                    }
                    match run_command_with_error(
                        "poetry",
                        &["add", "termcolor"],
                        "poetry add termcolor",
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            info!("{}", e);
                        }
                    }
                } else {
                    let _ = std::fs::write(&target, content);
                }
            }
            None => {
                let _ = std::fs::create_dir_all(&target);
            }
        }
    }

    Ok(())
}
