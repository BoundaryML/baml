use std::{collections::HashMap, path::PathBuf};

use colored::Colorize;
use include_dir::{include_dir, Dir};
use regex::Regex;

use crate::errors::CliError;

use super::{
    baml_src::{Generator, LanguageConfig},
    clients::ClientConfig,
    env_setup::EnvManager,
    interact::get_value_or_default,
    traits::{ToBamlSrc, WithLanguage, WithLoader, Writer},
};



pub(super) struct ProjectConfig {
    pub project_root: PathBuf,
    pub generators: Vec<Generator>,
    pub clients: Vec<ClientConfig<String>>,
    #[allow(dead_code)]
    secrets_manager: EnvManager,
    #[allow(dead_code)]
    pub languages: Vec<LanguageConfig>,
}

impl WithLoader<ProjectConfig> for ProjectConfig {
    fn from_dialoguer(
        no_prompt: bool,
        project_root: &PathBuf,
        writer: &mut Writer,
    ) -> Result<ProjectConfig, CliError> {
        // Using the CLI, ask the user for values for the following:

        let languages = LanguageConfig::from_dialoguer(no_prompt, project_root, writer)?;

        // If no languages are selected, return an error
        if languages.is_empty() {
            return Err("At least one language must be selected".into());
        }

        let clients = ClientConfig::from_dialoguer(no_prompt, project_root, writer)?;

        writer.write_fmt(format_args!(
            "\n{}\n{}\n\n",
            "Sweet! BAML is also able to run tests for you in the language of your choice.".green(),
            "Let's set that up next.".dimmed()
        ))?;

        let secrets_manager = EnvManager::from_dialoguer(no_prompt, project_root, writer)?;

        let generators = languages
            .iter()
            .map(|l| -> Result<Generator, CliError> {
                // Skip this and go with defaults since we seem to be getting it right
                // writer.write_fmt(format_args!("Setting up {}...\n", l.name().green()))?;

                let install_command = get_value_or_default(
                    "What command do you use to install dependencies?",
                    l.install_command(),
                    true, // hardcoded for now.
                )?;

                let test_command = get_value_or_default(
                    "What command do you use to run tests?",
                    l.test_command(secrets_manager.command_prefix()),
                    true,
                )?;

                let package_version_command = get_value_or_default(
                    "What command do you use to check package versions?",
                    l.package_version_command(),
                    true,
                )?;

                // Convert l.project_root() as relative to project_root
                let relative_project_root_res = l.project_root().strip_prefix(project_root);
                // strip prefix fails if the user chose
                // the default project root ./
                // and a child dir for the language, like "python", which has no ./ prefix.
                // So we just ignore it and use the language root as is.
                let relative_project_root = match relative_project_root_res {
                    Ok(p) => p,
                    Err(_e) => l.project_root().as_path(),
                };

                Ok(Generator::new(
                    l.name(),
                    PathBuf::from("../").join(relative_project_root),
                    test_command,
                    install_command,
                    package_version_command,
                ))
            })
            .filter_map(Result::ok)
            .collect();

        Ok(ProjectConfig {
            project_root: project_root.clone(),
            generators,
            clients,
            secrets_manager,
            languages,
        })
    }
}

struct SampleDir<'a> {
    baml: Dir<'a>,
    python: Dir<'a>,
    typescript: Dir<'a>,
}

// This is conceptual; Rust does not allow `include_dir!` in const/static directly in this way
static SAMPLE_DIR: SampleDir = SampleDir {
    baml: include_dir!("baml-cli/src/init_command/sample/baml"),
    python: include_dir!("baml-cli/src/init_command/sample/lang/python"),
    typescript: include_dir!("baml-cli/src/init_command/sample/lang/typescript"),
};

fn replace_vars(content: &str, vars: &HashMap<&str, &str>) -> Result<String, String> {
    // Compile a regex to find all instances of "$var$"
    let re = Regex::new(r"\$(\w+)\$").map_err(|e| e.to_string())?;

    let mut result = content.to_string();

    // Use a for loop to find and replace all matches
    for cap in re.captures_iter(content) {
        let var_name = &cap[1]; // Capture the variable name without the dollar signs
        match vars.get(var_name) {
            Some(replacement) => {
                // Replace the whole match (including the dollar signs) with the replacement
                let whole_match = &cap[0];
                result = result.replacen(whole_match, replacement, 1);
            }
            None => return Err(format!("Variable '{}' not found in vars", var_name)),
        }
    }

    Ok(result)
}

impl ProjectConfig {
    pub fn to_project_dir(&self) -> Result<HashMap<PathBuf, String>, CliError> {
        let mut source_files = HashMap::new();

        // The main file contains only the generators for the clients
        source_files.insert(
            self.project_root.join("baml_src/main.baml"),
            self.generators
                .iter()
                .map(|g| g.to_baml())
                .collect::<Vec<String>>()
                .join("\n\n"),
        );

        // Now add all the clients
        source_files.insert(
            self.project_root.join("baml_src/clients.baml"),
            self.clients
                .iter()
                .map(|c| c.to_baml())
                .collect::<Vec<String>>()
                .join("\n\n"),
        );

        let template_vars = {
            let mut vars = HashMap::new();
            vars.insert(
                "GENERATED_BAML_CLIENT",
                self.clients.first().unwrap().name.as_str(),
            );
            vars
        };

        fn write_files(
            dir: &Dir<'_>,
            target_root: &PathBuf,
            template_vars: &HashMap<&str, &str>,
            source_map: &mut HashMap<PathBuf, String>,
        ) -> Result<(), CliError> {
            for (path, content) in dir.find("**/*").unwrap().filter_map(|entry| {
                if let Some(f) = entry.as_file() {
                    let path = target_root.join(f.path());
                    let content = f.contents_utf8().unwrap();
                    Some((path, content))
                } else {
                    None
                }
            }) {
                source_map.insert(path, replace_vars(content, template_vars)?);
            }
            Ok(())
        }

        // Insert all the sample files to the appropriate locations
        write_files(
            &SAMPLE_DIR.baml,
            &self.project_root,
            &template_vars,
            &mut source_files,
        )?;

        // Insert all the language files to the appropriate locations
        for l in &self.languages {
            let lang_dir = match l {
                LanguageConfig::Python(_) => &SAMPLE_DIR.python,
                LanguageConfig::TypeScript(_) => &SAMPLE_DIR.typescript,
            };

            write_files(
                lang_dir,
                l.project_root(),
                &template_vars,
                &mut source_files,
            )?;
        }

        Ok(source_files)
    }
}
