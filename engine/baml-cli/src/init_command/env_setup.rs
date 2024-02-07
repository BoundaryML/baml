use std::path::PathBuf;

use crate::errors::CliError;

use super::{
    interact::{get_selection_or_default, get_value},
    traits::{WithLoader, Writer},
};

pub(super) enum EnvManager {
    Infisical,
    Doppler,
    EnvFile(PathBuf),
    Other(String),
}

impl EnvManager {
    pub fn command_prefix<'a>(&'a self) -> Option<&'a str> {
        match self {
            EnvManager::Infisical => Some("infisical run --"),
            EnvManager::Doppler => Some("doppler run --"),
            EnvManager::EnvFile(_) => None,
            EnvManager::Other(other) => Some(other.as_str()),
        }
    }
}

impl WithLoader<EnvManager> for EnvManager {
    fn from_dialoguer(
        no_prompt: bool,
        project_root: &PathBuf,
        _writer: &mut Writer,
    ) -> Result<Self, CliError> {
        // Ask the user if they want to use infisical or doppler

        let default_env_file = project_root.join(".env");
        let env_managers = [
            &format!(".env file ({})", default_env_file.display()),
            "Infisical (infisical run --)",
            "Doppler (doppler run --)",
            "other",
        ];

        match get_selection_or_default(
            "How do you set up environment variables?",
            &env_managers,
            default_env_manager(project_root),
            no_prompt,
        )? {
            0 => Ok(EnvManager::EnvFile(default_env_file)),
            1 => Ok(EnvManager::Infisical),
            2 => Ok(EnvManager::Doppler),
            _ => {
                let other = get_value("Whats the command prefix:", None, no_prompt)?;
                Ok(EnvManager::Other(other))
            }
        }
    }
}

fn default_env_manager(project_root: &PathBuf) -> usize {
    // Check if .env file exists in the project root
    if project_root.join(".env").exists() {
        return 0;
    }

    // Check if infisical is installed
    if std::process::Command::new("infisical")
        .arg("--version")
        .output()
        .is_ok()
    {
        return 1;
    }

    // Check if doppler is installed
    if std::process::Command::new("doppler")
        .arg("--version")
        .output()
        .is_ok()
    {
        return 2;
    }

    0
}
