use std::path::PathBuf;

use colored::Colorize;

use super::{
    interact::{get_selection_or_default, get_value_or_default},
    traits::{WithLanguage, WithLoader, Writer},
};
use crate::errors::CliError;

pub(super) struct TypeScriptConfig {
    project_root: PathBuf,
    package_manager: PackageManager,
}

impl TypeScriptConfig {
    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }
}

impl WithLoader<TypeScriptConfig> for TypeScriptConfig {
    fn from_dialoguer(
        no_prompt: bool,
        _project_root: &PathBuf,
        writer: &mut Writer,
    ) -> Result<Self, CliError> {
        let modified_prompt = format!(
            "What is the root of your {} project (where package.json lives)?",
            "TypeScript".blue().bold()
        );
        let ts_project_root: PathBuf =
            get_value_or_default(&modified_prompt, "./".to_string(), no_prompt)?.into();

        // Ensure a package.json exists
        if !ts_project_root.join("package.json").exists() {
            return Err(CliError::StringError(format!(
                "No package.json found in {}",
                ts_project_root.display()
            )));
        }

        let package_manager = PackageManager::from_dialoguer(no_prompt, &ts_project_root, writer)?;
        Ok(TypeScriptConfig {
            project_root: ts_project_root,
            package_manager,
        })
    }
}

enum PackageManager {
    Yarn,
    Pnpm,
    Npm,
}

impl WithLanguage for TypeScriptConfig {
    fn install_command(&self) -> String {
        self.package_manager.install_command()
    }

    fn test_command<T: AsRef<str>>(&self, prefix: Option<T>) -> String {
        self.package_manager.test_command(prefix)
    }

    fn package_version_command(&self) -> String {
        self.package_manager.package_version_command()
    }
}

impl WithLanguage for PackageManager {
    fn test_command<T: AsRef<str>>(&self, prefix: Option<T>) -> String {
        let res = match self {
            PackageManager::Yarn => "yarn test".into(),
            PackageManager::Pnpm => "pnpm test --".into(),
            PackageManager::Npm => "npm test --".into(),
        };

        prefix
            .map(|p| format!("{} {}", p.as_ref(), res))
            .unwrap_or(res)
    }

    fn install_command(&self) -> String {
        match self {
            PackageManager::Yarn => "yarn add".into(),
            PackageManager::Pnpm => "pnpm add".into(),
            PackageManager::Npm => "npm install".into(),
        }
    }

    fn package_version_command(&self) -> String {
        match self {
            PackageManager::Npm => "npm list".into(),
            PackageManager::Yarn => "yarn list".into(),
            PackageManager::Pnpm => "pnpm list".into(),
        }
    }
}

impl WithLoader<PackageManager> for PackageManager {
    fn from_dialoguer(
        no_prompt: bool,
        ts_project_root: &PathBuf,
        _writer: &mut Writer,
    ) -> Result<Self, CliError> {
        let package_managers = ["yarn", "pnpm", "npm"];
        match get_selection_or_default(
            "Which package manager do you use:",
            &package_managers,
            default_package_manager(&ts_project_root),
            no_prompt,
        )? {
            0 => Ok(PackageManager::Yarn),
            1 => Ok(PackageManager::Pnpm),
            2 => Ok(PackageManager::Npm),
            _ => Err(CliError::StringError(
                "Unsupported, see docs for manual setup".into(),
            )),
        }
    }
}

fn default_package_manager(ts_project_root: &PathBuf) -> usize {
    if ts_project_root.join("yarn.lock").exists() {
        return 0;
    }

    if ts_project_root.join("pnpm-lock.yaml").exists() {
        return 1;
    }

    2
}
