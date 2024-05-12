use dunce::canonicalize;
/// File to convert types to baml code.
use std::path::PathBuf;

use crate::errors::CliError;

use super::{
    interact::get_multi_selection_or_default,
    py::PythonConfig,
    traits::{ToBamlSrc, WithLanguage, WithLoader, Writer},
    ts::TypeScriptConfig,
};

pub(super) enum LanguageConfig {
    Python(PythonConfig),
    TypeScript(TypeScriptConfig),
}

impl LanguageConfig {
    pub fn project_root(&self) -> &PathBuf {
        match self {
            LanguageConfig::Python(py) => py.project_root(),
            LanguageConfig::TypeScript(ts) => ts.project_root(),
        }
    }

    pub fn name(&self) -> String {
        match self {
            LanguageConfig::Python(_) => "python".into(),
            LanguageConfig::TypeScript(_) => "typescript".into(),
        }
    }
}

impl WithLanguage for LanguageConfig {
    fn install_command(&self) -> String {
        match self {
            LanguageConfig::Python(py) => py.install_command(),
            LanguageConfig::TypeScript(ts) => ts.install_command(),
        }
    }

    fn test_command<T: AsRef<str>>(&self, prefix: Option<T>) -> String {
        match self {
            LanguageConfig::Python(py) => py.test_command(prefix),
            LanguageConfig::TypeScript(ts) => ts.test_command(prefix),
        }
    }

    fn package_version_command(&self) -> String {
        match self {
            LanguageConfig::Python(py) => py.package_version_command(),
            LanguageConfig::TypeScript(ts) => ts.package_version_command(),
        }
    }
}

fn find_folder_with(base_path: &str, patterns: &[&str]) -> Option<String> {
    for pattern in patterns {
        let full_pattern = format!("{}/{}", base_path, pattern);
        for entry in glob::glob(&full_pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    if path.is_file() {
                        return Some(path.to_string_lossy().into_owned());
                    }
                }
                Err(e) => println!("Error while reading path: {:?}", e),
            }
        }
    }
    None
}

impl WithLoader<Vec<LanguageConfig>> for LanguageConfig {
    fn from_dialoguer(
        no_prompt: bool,
        project_root: &PathBuf,
        writer: &mut Writer,
    ) -> Result<Vec<LanguageConfig>, CliError> {
        let project_root_str = match canonicalize(project_root) {
            Ok(p) => p,
            Err(_) => project_root.clone(),
        };
        let project_root_str = project_root_str.to_string_lossy();
        let has_python = find_folder_with(
            project_root_str.as_ref(),
            &[
                "**/*.py",
                "**/requirements.txt",
                "**/Pipfile",
                "**/setup.py",
                "**/pyproject.toml",
                "**/.python-version",
            ],
        )
        .is_some();
        let has_typescript = find_folder_with(
            project_root_str.as_ref(),
            &[
                "**/*.ts",
                "**/package.json",
                "**/tsconfig.json",
                "**/yarn.lock",
                "**/package-lock.json",
            ],
        )
        .is_some();

        let default_selection = [has_python, has_typescript];
        let languages = get_multi_selection_or_default(
            "What language(s) do you want to use with BAML?",
            &["Python", "TypeScript"],
            &default_selection,
            no_prompt,
        )?;

        if languages.is_empty() {
            if no_prompt {
                return Err(CliError::StringError(
                    r#"Failed to detect any Python or Typescript project. 
                    
`baml init` command must be run within an existing Python or TypeScript project directory. It cannot be run in a new or empty directory. Please navigate to a valid project directory or initialize a new project using the appropriate tools for Python or TypeScript before running `baml init`.
                    "#.into(),
                ));
            }

            return Err(CliError::StringError("No language selected.".into()));
        }

        if !no_prompt {
            writer.write_fmt(format_args!("\nGreat choice!\n"))?;
        }

        languages
            .iter()
            .map(|&lang| match lang {
                0 => PythonConfig::from_dialoguer(no_prompt, project_root, writer)
                    .map(LanguageConfig::Python),
                1 => TypeScriptConfig::from_dialoguer(no_prompt, project_root, writer)
                    .map(LanguageConfig::TypeScript),
                _ => unreachable!("Invalid language selection"),
            })
            .collect()
    }
}

pub(super) struct Generator {
    language: String,
    project_root: PathBuf,
    test_command: String,
    install_command: String,
    package_version_command: String,
}

impl Generator {
    pub fn new(
        language: String,
        project_root: PathBuf,
        test_command: String,
        install_command: String,
        package_version_command: String,
    ) -> Self {
        Self {
            language,
            project_root,
            test_command,
            install_command,
            package_version_command,
        }
    }
}

impl ToBamlSrc for Generator {
    fn to_baml(&self) -> String {
        format!(
            r#"
generator lang_{} {{
  language {}
  // This is where your baml_client will be generated
  // Usually the root of your source code relative to this file
  project_root "{}"
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "{}"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "{}"
  package_version_command "{}"
}}
        "#,
            self.language,
            self.language,
            self.project_root.to_string_lossy(),
            self.test_command,
            self.install_command,
            self.package_version_command
        )
        .trim()
        .into()
    }
}
