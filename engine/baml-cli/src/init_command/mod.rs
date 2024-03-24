mod baml_src;
mod clients;
mod config;
mod env_setup;
mod interact;
mod py;
mod traits;
mod ts;

use std::{fs, path::PathBuf};

use colored::Colorize;
use serde_json::Value;

use crate::{builder::get_baml_src, errors::CliError};

use self::{
    baml_src::LanguageConfig,
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
    if get_baml_src(&None).is_ok() {
        return Err("Already in a baml project".into());
    }

    let proj = walkthrough(no_prompt)?;

    let source_files = proj.to_project_dir()?;

    let res: Result<(), CliError> = proj.languages.iter().try_for_each(|lang| match lang {
        LanguageConfig::TypeScript(_) => {
            println!("Adding testing script to your Typescript project's package.json...");
            let ts_dir = &proj.project_root;
            let package_json_path = ts_dir.join("package.json");
            let package_json_content = fs::read_to_string(&package_json_path)
                .map_err(|e| format!("Failed to read package.json: {}", e))?;
            let mut package_json: Value = serde_json::from_str(&package_json_content)
                .map_err(|e| format!("Failed to parse package.json: {}", e))?;

            let updated_scripts = match package_json.get_mut("scripts") {
                Some(scripts) => {
                    if let Some(scripts) = scripts.as_object_mut() {
                        if !scripts.contains_key("baml-test") {
                            scripts
                                .insert("baml-test".to_owned(), Value::String("jest".to_owned()));
                            true
                        } else {
                            false
                        }
                    } else {
                        println!("'scripts' section in package.json is not an object. Ensure there is a `\"baml-test\": \"jest\"` entry in the 'scripts' section of your package.json");
                        false
                    }
                }
                None => {
                    package_json.as_object_mut().unwrap().insert(
                        "scripts".to_owned(),
                        Value::Object({
                            let mut scripts = serde_json::Map::new();
                            scripts
                                .insert("baml-test".to_owned(), Value::String("jest".to_owned()));
                            scripts
                        }),
                    );
                    true
                }
            };

            if updated_scripts {
                fs::write(
                    &package_json_path,
                    serde_json::to_string_pretty(&package_json)?,
                )
                .map_err(|e| format!("Failed to write to package.json: {}", e))?;
            }

            Ok(())
        }
        _ => Ok(()),
    });

    // User will need to rerun the CLI if there is an error
    res?;

    // Write all the files to disk
    for (path, content) in &source_files {
        // Ensure the directory exists
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, content)?;
    }

    Ok(())
}
