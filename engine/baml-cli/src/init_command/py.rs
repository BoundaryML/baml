use std::path::PathBuf;

use crate::errors::CliError;
use colored::Colorize;
use walkdir::WalkDir;

use super::{
    interact::{get_selection_or_default, get_value, get_value_or_default},
    traits::{WithLanguage, WithLoader, Writer},
};

pub(super) struct PythonConfig {
    project_root: PathBuf,
    package_manager: PackageManager,
}

impl PythonConfig {
    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }
}

impl WithLoader<PythonConfig> for PythonConfig {
    fn from_dialoguer(
        no_prompt: bool,
        _project_root: &PathBuf,
        writer: &mut Writer,
    ) -> Result<Self, CliError> {
        let modified_prompt = format!(
            "What is the root of your {} project? {}",
            "Python".cyan().bold(),
            "(This is where we'll generate the baml python client)".dimmed()
        );
        let py_project_root =
            get_value_or_default(&modified_prompt, "./".to_string(), no_prompt)?.into();

        let package_manager = PackageManager::from_dialoguer(no_prompt, &py_project_root, writer)?;
        Ok(PythonConfig {
            project_root: py_project_root,
            package_manager,
        })
    }
}

enum PackageManager {
    Pip(String),
    Pip3(String),
    Poetry,
    // Path to the virtual environment (not the activate script, the directory itself)
    Venv(String),
    // Name of the conda environment
    Conda(String),
    Pipenv(String),
}

impl WithLanguage for PythonConfig {
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
            PackageManager::Pip(py_path) | PackageManager::Pip3(py_path) => {
                format!("{} -m pytest", py_path)
            }
            PackageManager::Poetry => "poetry run pytest".into(),
            PackageManager::Venv(env_path) => {
                // TODO: Use the best command for each os:
                // - POSIX (bash/zsh): . venv/bin/activate
                // - POSIX (fish): . venv/bin/activate.fish
                // - POSIX (csh/tcsh): . venv/bin/activate.csh
                // - POSIT (powershell): venv\Scripts\Activate.ps1
                // - Windows (cmd.ext): venv\Scripts\activate.bat
                // - Windows (powershell): venv\Scripts\Activate.ps1
                format!(". {}/bin/activate && python -m pytest", env_path)
            }
            PackageManager::Conda(name) => {
                format!("conda run -n {} pytest", name)
            }
        };

        match (prefix, self) {
            (Some(p), PackageManager::Venv(env_path)) => {
                format!(
                    "{env_path} && {} python -m pytest",
                    p.as_ref(),
                    env_path = env_path
                )
            }
            (Some(p), _) => format!("{} {}", p.as_ref(), res),
            (None, _) => res,
        }
    }

    fn install_command(&self) -> String {
        match self {
            PackageManager::Pip(_) => "pip install --upgrade baml".into(),
            PackageManager::Pip3(_) => "pip3 install --upgrade baml".into(),
            PackageManager::Poetry => "poetry add baml@latest".into(),
            PackageManager::Venv(env_path) => {
                format!(". {}/bin/activate && pip install --upgrade baml", env_path)
            }
            PackageManager::Conda(name) => {
                format!("conda run -n {} pip install --upgrade baml", name)
            }
            PackageManager::Pipenv(_) => "pipenv shell && pipenv install --upgrade baml".into(),
        }
    }

    fn package_version_command(&self) -> String {
        match self {
            PackageManager::Pip(_) => "pip show baml".into(),
            PackageManager::Pip3(_) => "pip3 show baml".into(),
            PackageManager::Poetry => "poetry show baml".into(),
            PackageManager::Venv(path) => format!(". {}/bin/activate && pip show baml", path),
            PackageManager::Conda(name) => format!("conda list -n {} baml", name),
            PackageManager::Pip(_) => "pipenv run pip show baml".into(),
        }
    }
}

impl WithLoader<PackageManager> for PackageManager {
    fn from_dialoguer(
        no_prompt: bool,
        py_project_root: &PathBuf,
        _writer: &mut Writer,
    ) -> Result<Self, CliError> {
        // Check if python is installed, if not return an error
        let python_path = python_cli()?;

        let package_managers = [
            "pip",
            "pip3",
            "poetry",
            "venv",
            "virtualenv",
            "conda",
            "pipenv",
            "other",
        ];

        match get_selection_or_default(
            "What package manager do you use?",
            &package_managers,
            default_package_manager(py_project_root, python_path),
            no_prompt,
        )? {
            0 => Ok(PackageManager::Pip(python_path.into())),
            1 => Ok(PackageManager::Pip3(python_path.into())),
            2 => Ok(PackageManager::Poetry),
            3 | 4 => {
                let default_venv_path =
                    is_venv(python_path, py_project_root).map(|p| p.to_string_lossy().to_string());

                let env_path = get_value(
                    "What is the path to your virtual environment:",
                    default_venv_path,
                    no_prompt,
                )?;
                Ok(PackageManager::Venv(env_path))
            }
            5 => {
                let default_conda_env = is_conda().unwrap_or("base".into());
                let env_name = get_value_or_default(
                    "What is the name of your conda environment?",
                    default_conda_env,
                    no_prompt,
                )?;
                Ok(PackageManager::Conda(env_name))
            }
            6 => Ok(PackageManager::Pipenv(python_path.into())),
            _ => Err(CliError::StringError(
                "Unsupported, see docs for manual setup".into(),
            )),
        }
    }
}

fn default_package_manager(project_root: &PathBuf, python_path: &str) -> usize {
    // Check if pyproject.toml exists in the project root && poetry is installed
    if is_poetry(project_root, python_path).is_some() {
        return 2;
    }

    // Check if venv is installed
    if is_venv(python_path, project_root).is_some() {
        return 3;
    }

    // Check if conda is installed
    if is_conda().is_some() {
        return 5;
    }

    // Check if pip3 is installed
    if is_pip3().is_some() {
        return 1;
    }

    if is_pipenv().is_some() {
        return 6;
    }

    0
}

fn python_cli() -> Result<&'static str, CliError> {
    // Determine the path to the python executable
    match std::process::Command::new("python3")
        .arg("-c")
        .arg("import sys; print(sys.executable)")
        .output()
    {
        Ok(_) => Ok("python3"),
        Err(_) => match std::process::Command::new("python")
            .arg("-c")
            .arg("import sys; print(sys.executable)")
            .output()
        {
            Ok(_) => Ok("python"),
            Err(_) => Err(CliError::StringError(
                "python or python3 not in PATH".into(),
            )),
        },
    }
}

fn is_poetry(project_root: &PathBuf, python_path: &str) -> Option<(&'static str, &'static str)> {
    if project_root.join("pyproject.toml").exists() {
        if std::process::Command::new(python_path)
            .arg("-m")
            .arg("poetry")
            .arg("--version")
            .output()
            .is_ok()
        {
            return Some(("python -m poetry", "poetry"));
        } else if std::process::Command::new("poetry")
            .arg("--version")
            .output()
            .is_ok()
        {
            return Some(("poetry", "poetry"));
        }
    }

    None
}

fn is_venv(_python_path: &str, project_root: &PathBuf) -> Option<PathBuf> {
    // Check if current python is in a virtual environment
    for entry in WalkDir::new(project_root) {
        let entry = entry.unwrap();
        if entry.file_type().is_dir()
            && (entry.path().ends_with("bin") || entry.path().ends_with("Scripts"))
        {
            let activate_path = entry.path().join(if cfg!(windows) {
                "activate.bat"
            } else {
                "activate"
            });
            if activate_path.exists() {
                return Some(
                    activate_path
                        .parent() // bin
                        .unwrap()
                        .parent() // venv
                        .unwrap()
                        .to_path_buf(),
                );
            }
        }
    }
    None
}

fn is_conda() -> Option<String> {
    // Check if CONDA_DEFAULT_ENV is set
    if let Ok(val) = std::env::var("CONDA_DEFAULT_ENV") {
        return Some(val);
    }

    // Check if conda is installed
    match std::process::Command::new("conda")
        .arg("--version")
        .output()
    {
        Ok(_) => Some("base".into()),
        Err(_) => None,
    }
}

fn is_pip3() -> Option<(&'static str, &'static str)> {
    // Check if pip3 is installed
    match std::process::Command::new("pip3").arg("--version").output() {
        Ok(_) => Some(("pip3", "pip3")),
        Err(_) => None,
    }
}

fn is_pipenv() -> Option<(&'static str, &'static str)> {
    // Check if pipenv is installed
    match std::process::Command::new("pipenv").arg("--version").output() {
        Ok(_) => Some(("pipenv", "pipenv")),
        Err(_) => None,
    }
}
