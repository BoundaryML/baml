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
            "What is the root of your {} project (where tsconfig.json lives)?",
            "TypeScript".cyan().bold()
        );

        let ts_project_root: PathBuf =
            get_value_or_default(&modified_prompt, "./".to_string(), no_prompt)?.into();

        // Ensure a package.json exists
        if !ts_project_root.join("tsconfig.json").exists() {
            return Err(CliError::StringError(format!(
                "No tsconfig.json found in {}",
                ts_project_root.display()
            )));
        }

        let target_dir = match find_source_directory(&ts_project_root.join("tsconfig.json")) {
            Ok(dir) => dir,
            Err(e) => {
                println!("Warning: {}", e);
                ts_project_root.clone()
            }
        };

        let package_manager = PackageManager::from_dialoguer(no_prompt, &target_dir, writer)?;
        Ok(TypeScriptConfig {
            project_root: target_dir,
            package_manager,
        })
    }
}

pub(super) enum PackageManager {
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
            // The baml-test is a script we automatically add to the package.json that just runs jest
            PackageManager::Yarn => "yarn baml-test".into(),
            PackageManager::Pnpm => "pnpm baml-test".into(),
            PackageManager::Npm => "npm run baml-test --".into(),
        };

        prefix
            .map(|p| format!("{} {}", p.as_ref(), res))
            .unwrap_or(res)
    }

    fn install_command(&self) -> String {
        let allow_prerelease = std::env::var("BAML_ALLOW_PRERELEASE")
            .map(|v| v == "1")
            .unwrap_or(false);

        let pkgname = if allow_prerelease {
            "@boundaryml/baml-core@next @boundaryml/baml-core-ffi@next"
        } else {
            "@boundaryml/baml-core"
        };

        match self {
            PackageManager::Yarn => {
                format!("yarn add -D jest ts-jest @types/jest && yarn add {pkgname}")
            }
            PackageManager::Pnpm => {
                format!("pnpm add -D jest ts-jest @types/jest && pnpm add {pkgname}")
            }
            PackageManager::Npm => {
                format!("npm install -D jest ts-jest @types/jest && npm install {pkgname}")
            }
        }
    }

    fn package_version_command(&self) -> String {
        match self {
            PackageManager::Npm => "npm list @boundaryml/baml-core".into(),
            PackageManager::Yarn => "yarn list @boundaryml/baml-core".into(),
            PackageManager::Pnpm => "pnpm list @boundaryml/baml-core".into(),
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
            default_package_manager(ts_project_root),
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

#[derive(serde::Deserialize, Debug)]
struct CompilerOptions {
    #[serde(rename = "rootDir")]
    root_dir: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct TsConfig {
    #[serde(rename = "compilerOptions")]
    compiler_options: Option<CompilerOptions>,
    include: Option<Vec<String>>,
}

fn find_common_root(paths: Vec<PathBuf>) -> Result<PathBuf, CliError> {
    let mut common_path: PathBuf = PathBuf::new();
    for path in paths {
        if common_path.as_os_str().is_empty() {
            common_path = path.clone();
        } else {
            while !path.starts_with(&common_path) {
                if !common_path.pop() {
                    return Err(CliError::StringError("Could not find common root".into()));
                }
            }
        }
    }

    if common_path.is_file() {
        return common_path
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| CliError::StringError("Could not find common root".into()));
    }
    Ok(common_path)
}

fn find_source_directory(tsconfig_path: &std::path::Path) -> Result<PathBuf, CliError> {
    let file_content = std::fs::read_to_string(tsconfig_path)?;
    let stripped = json_comments::StripComments::new(file_content.as_bytes());
    let tsconfig: TsConfig = serde_json::from_reader(stripped)?;

    if let Some(compiler_options) = tsconfig.compiler_options {
        if let Some(root_dir) = compiler_options.root_dir {
            return Ok(tsconfig_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""))
                .join(root_dir));
        }
    }

    if let Some(include) = tsconfig.include {
        let base_path = tsconfig_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let paths: Vec<PathBuf> = include
            .iter()
            .filter_map(|pattern| {
                match glob::glob(&format!("{}/**/{}", base_path.display(), pattern)) {
                    Ok(paths) => Some(paths.filter_map(Result::ok)),
                    Err(_) => None,
                }
            })
            .flatten()
            .collect();

        return find_common_root(paths);
    }

    Err(CliError::StringError(
        "Could not find source directory".into(),
    ))
}
