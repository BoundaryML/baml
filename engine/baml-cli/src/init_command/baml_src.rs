use core::hash;
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

impl WithLoader<Vec<LanguageConfig>> for LanguageConfig {
    fn from_dialoguer(
        no_prompt: bool,
        project_root: &PathBuf,
        writer: &mut Writer,
    ) -> Result<Vec<LanguageConfig>, CliError> {
        let mut has_python = false;
        let mut has_typescript = false;

        // Iterate upwards, starting from the project root all the way to the root dir
        for p in project_root
            .clone()
            .canonicalize()
            .unwrap_or(project_root.clone())
            .ancestors()
        {
            let Ok(dir) = std::fs::read_dir(p) else {
                continue;
            };
            let files = dir
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect::<Vec<_>>();

            has_python = has_python
                || files
                    .iter()
                    .any(|f| f == "pyproject.toml" || f.ends_with(".py"));
            has_typescript = has_typescript
                || files
                    .iter()
                    .any(|f| f == "package.json" || f.ends_with(".ts") || f.ends_with(".js"));

            // If we know we're going to suggest both, exit
            if has_python && has_typescript {
                break;
            }

            // If we reach the root of a git repo (or worktree, or submodule), exit
            if std::fs::metadata(p.join(".git")).is_ok() {
                break;
            }
        }

        let default_selection = if has_python || has_typescript {
            [has_python, has_typescript]
        } else {
            [true, true]
        };
        let languages = get_multi_selection_or_default(
            "What language(s) do you want to use with BAML?",
            &["Python", "TypeScript"],
            &default_selection,
            no_prompt,
        )?;

        writer.write_fmt(format_args!("\nGreat choice!\n"))?;

        languages
            .iter()
            .map(|&lang| match lang {
                0 => PythonConfig::from_dialoguer(no_prompt, project_root, writer)
                    .map(LanguageConfig::Python),
                1 => TypeScriptConfig::from_dialoguer(no_prompt, project_root, writer)
                    .map(LanguageConfig::TypeScript),
                _ => unreachable!(),
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
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
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
