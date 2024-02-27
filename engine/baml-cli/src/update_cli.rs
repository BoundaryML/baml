use baml_lib::internal_baml_core::LockFileWrapper;
use check_latest::Version;
use colored::*;
use log::{info, warn};
use semver;
use serde::{Deserialize, Serialize};

use crate::{command::run_command_with_error, errors::CliError, Cli};

#[derive(Debug, Serialize, Deserialize)]
pub struct Versions {
    pub cli: semver::Version,
    pub py_client: Option<semver::Version>,
}

impl Versions {
    pub fn from_url() -> Result<Self, CliError> {
        let url = "https://raw.githubusercontent.com/GlooHQ/homebrew-baml/main/version.json";
        // info!("Checking for updates at {}", url);
        let response = reqwest::blocking::get(url)?;
        if !response.status().is_success() {
            return Err(format!("Failed to get versions: {}", response.status()).into());
        }
        let versions: Versions = response.json()?;
        Ok(versions)
    }

    pub fn from_lock_file(lockfile: &LockFileWrapper) -> Result<Self, CliError> {
        Ok(Versions {
            cli: lockfile
                .cli_version()
                .ok_or_else(|| CliError::StringError("No cli version found".into()))?
                .clone(),
            py_client: lockfile.client_version().cloned(),
        })
    }
}

fn get_version() -> (semver::Version, Option<semver::Version>) {
    if let Ok(current_version) = semver::Version::parse(env!("CARGO_PKG_VERSION")) {
        let mut update_available = None;
        if let Ok(available_versions) = Versions::from_url() {
            if available_versions.cli > current_version {
                update_available = Some(available_versions.cli);
            }
        } else {
            info!("{}", "Failed to check for updates.");
        }
        return (current_version, update_available);
    }
    panic!("Failed to parse current version.");
}

pub fn version_check(lockfile: &LockFileWrapper) {
    let current = Versions::from_lock_file(lockfile);
    let latest = Versions::from_url();
    match (current, latest) {
        (Ok(current), Ok(latest)) => {
            if latest.cli > current.cli {
                warn!(
                    "A new version of {} is available: {} -> {}",
                    env!("CARGO_PKG_NAME"),
                    current.cli,
                    latest.cli
                );
                warn!("Run `{} update` to update.", env!("CARGO_PKG_NAME"));
            }

            match (current.py_client, latest.py_client) {
                (Some(current), Some(latest)) => {
                    if latest > current {
                        warn!(
                            "A new version of {} is available: {} -> {}",
                            "Python client".green(),
                            current,
                            latest
                        );
                        warn!("Run `{} update-client` to update.", env!("CARGO_PKG_NAME"));
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

pub enum UpdateOutputFormat {
    Human,
    Json,
}

pub struct Opts {
    pub dry_run: bool,
    pub output_format: UpdateOutputFormat,
    pub baml_dir: Option<String>,
}

pub enum UpdateResult {
    UpdatedCurrentToLatest(semver::Version, semver::Version),
    DryRunWithCurrentAndLatest(semver::Version, semver::Version),
    NoUpdateAvailableForCurrentVersion(semver::Version),
}

pub fn update(opts: Opts) -> Result<(), CliError> {
    // DO NOT LAND: user should be able to baml update outside of a baml directory
    let mut diagnostics = baml_lib::Diagnostics::new(
        opts.baml_dir
            .ok_or(CliError::StringError("not in a baml directory".to_string()))?
            .into(),
    );
    return Err(CliError::BAMLError(diagnostics));

    let update_result = impl_update(opts.dry_run)?;

    #[derive(Serialize)]
    struct VersionStatus<'a> {
        /// Version of $thing going into `baml update`
        was_version: &'a semver::Version,

        /// Version of $thing at the end of `baml update`
        now_version: &'a semver::Version,

        /// Latest available version of $thing as determined by the `baml update` query
        latest_version: &'a semver::Version,
    };

    #[derive(Serialize)]
    struct VersionStatusByComponent<'a> {
        cli: VersionStatus<'a>,
        client: VersionStatus<'a>,
    }

    match update_result {
        UpdateResult::DryRunWithCurrentAndLatest(current, latest) => match opts.output_format {
            UpdateOutputFormat::Human => (),
            UpdateOutputFormat::Json => {
                println!("json dry-run-with-current-and-latest");
            }
        },
        UpdateResult::UpdatedCurrentToLatest(current, latest) => {}
        UpdateResult::NoUpdateAvailableForCurrentVersion(current) => match opts.output_format {
            UpdateOutputFormat::Human => {
                println!(
                    "{} {}:{}",
                    "No updates available for".green(),
                    env!("CARGO_PKG_NAME"),
                    current,
                );
            }
            UpdateOutputFormat::Json => {
                println!("json no-update-available",);
            }
        },
    }

    Ok(())
}

pub fn impl_update(dry_run: bool) -> Result<UpdateResult, CliError> {
    // Check if the latest version is installed
    let (current_version, update_available) = get_version();
    // Update to the latest version
    if let Some(latest_version) = update_available {
        info!(
            "{} {} -> {}",
            "Updating".green(),
            current_version,
            latest_version
        );
        if dry_run {
            return Ok(UpdateResult::DryRunWithCurrentAndLatest(
                current_version,
                latest_version,
            ));
        }

        execute_update()?;
        return Ok(UpdateResult::UpdatedCurrentToLatest(
            current_version,
            latest_version,
        ));
    }

    return Ok(UpdateResult::NoUpdateAvailableForCurrentVersion(
        current_version,
    ));
}

fn execute_update() -> Result<(), CliError> {
    if cfg!(debug_assertions) {
        return Err("Not available for debug builds".into());
    }

    if cfg!(target_os = "macos") {
        if is_installed_from_brew() {
            update_brew()
        } else {
            update_shell_install()
        }
    } else if cfg!(target_os = "windows") {
        update_windows()
    } else if cfg!(target_os = "linux") {
        update_shell_install()
    } else {
        Err("Unsupported platform".into())
    }
}

fn is_installed_from_brew() -> bool {
    std::process::Command::new("brew")
        .args(&["list", "baml"])
        .output()
        .map(|o| o.status.success())
        .ok()
        .unwrap_or(false)
}

fn update_brew() -> Result<(), CliError> {
    run_command_with_error("brew", ["tap", "gloohq/baml"], "brew tap gloohq/baml")?;
    run_command_with_error("brew", ["update"], "brew update")?;
    run_command_with_error("brew", ["upgrade", "baml"], "brew upgrade gloo")
}

fn update_windows() -> Result<(), CliError> {
    run_command_with_error("scoop", ["update"], "scoop update")?;
    run_command_with_error("scoop", ["update", "baml"], "scoop update gloo")
}

fn update_shell_install() -> Result<(), CliError> {
    static INSTALL_SCRIPT: &str =
        "https://raw.githubusercontent.com/GlooHQ/homebrew-baml/main/install-baml.sh";

    let command = ["-c", "curl", "-fsS", INSTALL_SCRIPT, "|", "sh"];
    run_command_with_error("sh", &command, "install command")
}
