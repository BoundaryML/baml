use crate::{builder::get_src_dir, shell::build_shell_command};
use crate::{command::run_command_with_error, errors::CliError, Cli};
use clap;
use colored::Colorize;
use log;
use semver;
use serde;
use std::path::PathBuf;
use std::thread::current;

#[derive(Debug, serde::Deserialize)]
pub struct LatestVersionsManifest {
    pub cli: Option<String>,
    pub py_client: Option<String>,
    pub ts_client: Option<String>,
    pub vscode: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct CheckedVersions {
    pub cli: CliVersion,
    pub clients: Vec<ClientVersions>,
    pub vscode: VscodeVersion,
}

#[derive(Debug, serde::Serialize)]
pub struct CliVersion {
    pub current_version: String,
    pub latest_version: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ClientVersions {
    pub dir: PathBuf,
    pub language: String,
    pub current_version: String,
    pub latest_version: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct VscodeVersion {
    pub latest_version: Option<String>,
}

pub fn get_client_version(
    project_root: &str,
    package_version_command: &str,
) -> Result<String, CliError> {
    let cmd = shellwords::split(package_version_command)
        .map_err(|e| CliError::StringError(e.to_string()))?;

    let mut cmd = build_shell_command(cmd);

    cmd.current_dir(std::env::current_dir()?.join(project_root).canonicalize()?);

    cmd.output()
        .map_err(|e| CliError::StringError(e.to_string()))
        .map_err(|e| CliError::StringError(format!("{} Error: {}", "Failed!".red(), e)))
        .and_then(|e| {
            if !e.status.success() {
                return Err(CliError::StringError(format!("failed {}", e.status)));
            }

            Ok(String::from_utf8(e.stdout)?)
        })
}

// TODO: require --check to query github
// TODO: check failure modes
pub fn run(baml_dir: &Option<String>) -> Result<(), CliError> {
    let mut ret = CheckedVersions {
        cli: CliVersion {
            current_version: clap::crate_version!().to_string(),
            latest_version: None,
        },
        clients: Vec::new(),
        vscode: VscodeVersion {
            latest_version: None,
        },
    };
    let url = "https://raw.githubusercontent.com/GlooHQ/homebrew-baml/main/version.json";
    log::debug!("Checking for updates at {}", url);
    let response = reqwest::blocking::get(url)?;
    if !response.status().is_success() {
        return Err(format!("Failed to get versions: {}", response.status()).into());
    }
    let latest_versions = response.json::<LatestVersionsManifest>()?;

    ret.cli.latest_version = latest_versions.cli;
    ret.vscode.latest_version = latest_versions.vscode;

    if let Ok((_, (config, _))) = get_src_dir(baml_dir) {
        for (gen, _) in config.generators {
            // every generator's client type needs to get attached to the output
            println!("version {:?}", gen);
            let current_version = get_client_version(
                gen.project_root.to_str().unwrap(),
                gen.package_version_command.as_str(),
            )?;
            ret.clients.push(ClientVersions {
                dir: gen.project_root.canonicalize()?,
                language: gen.language.as_str().to_string(),
                current_version: current_version,
                latest_version: match gen.language.as_str() {
                    "python" => latest_versions.py_client.clone(),
                    "typescript" => latest_versions.ts_client.clone(),
                    _ => None,
                },
            });
        }
    }

    println!("{}", serde_json::to_string_pretty(&ret)?);

    Ok(())
}
