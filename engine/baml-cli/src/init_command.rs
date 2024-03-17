mod baml_src;
mod clients;
mod config;
mod env_setup;
mod interact;
mod py;
mod traits;
mod ts;

use std::path::PathBuf;

use colored::Colorize;

use crate::{builder::get_baml_src, errors::CliError};

use self::{
    config::ProjectConfig,
    interact::get_value_or_default,
    traits::{WithLoader, Writer},
};

fn walkthrough(no_prompt: bool) -> Result<ProjectConfig, CliError> {
    // Using the CLI, ask the user for values for the following:
    // - Language (Python and/or TypeScript)

    let project_root = get_value_or_default(
        &format!(
            "What is the root of your project? {}",
            "This is where baml_src/ and .baml files will live".dimmed()
        ),
        "./".to_string(),
        no_prompt,
    )?;

    let project_root = PathBuf::from(project_root);
    if project_root.join("baml_src").exists() {
        return Err("baml_src already exists".into());
    }

    let mut writer = Writer::new(no_prompt);
    ProjectConfig::from_dialoguer(no_prompt, &project_root, &mut writer)
}

pub fn init_command(no_prompt: bool) -> Result<(), CliError> {
    if let Ok(_) = get_baml_src(&None) {
        return Err("Already in a baml project".into());
    }

    let proj = walkthrough(no_prompt)?;

    let source_files = proj.to_project_dir()?;

    // Write all the files to disk
    for (path, content) in &source_files {
        // Ensure the directory exists
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, content)?;
    }

    Ok(())
}
