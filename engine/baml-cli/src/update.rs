use check_latest::{crate_name, crate_version, user_agent, Versions};
use colored::*;
use log::{error, info, warn};
use semver;

use crate::{command::run_command_with_error, errors::CliError};

fn get_version() -> (semver::Version, Option<semver::Version>) {
    if let Ok(current_version) = semver::Version::parse(crate_version!()) {
        let mut update_available = None;
        if let Ok(available_versions) = Versions::new(crate_name!(), user_agent!()) {
            let is_yanked = available_versions
                .versions()
                .iter()
                .filter(|version| version.yanked)
                .any(|version| version == crate_version!());

            if is_yanked {
                warn!(
                    "Version {} is yanked, please contact us (contact@trygloo.com).",
                    crate_version!()
                );
            }

            if let Some(latest) = available_versions.max_version() {
                if let Ok(latest_version) = semver::Version::parse(&latest.to_string()) {
                    if latest_version > current_version {
                        update_available = Some(latest_version);
                    }
                }
            }
        } else {
            info!("{}", "Failed to check for updates.");
        }
        return (current_version, update_available);
    }
    panic!("Failed to parse current version.");
}

pub fn version_check() {
    let (current_version, update_available) = get_version();
    if let Some(latest_version) = update_available {
        warn!(
            "A new version of {} is available: {} -> {}",
            crate_name!(),
            current_version,
            latest_version
        );
        warn!("Run `{} update` to update.", crate_name!());
    }
}

pub fn update() -> Result<(), CliError> {
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
        impl_update()?;
    } else {
        println!(
            "{} {}:{}",
            "No updates available for".green(),
            crate_name!(),
            current_version
        );
    }
    Ok(())
}

fn impl_update() -> Result<(), CliError> {
    if cfg!(debug_assertions) {
        return Err("Not available for debug builds".into());
    }

    if cfg!(target_os = "macos") {
        update_macos()
    } else if cfg!(target_os = "windows") {
        update_windows()
    } else if cfg!(target_os = "linux") {
        update_linux()
    } else {
        Err("Unsupported platform".into())
    }
}

fn update_macos() -> Result<(), CliError> {
    run_command_with_error("brew", ["tap", "gloohq/gloo"], "brew tap gloohq/gloo")?;
    run_command_with_error("brew", ["update"], "brew update")?;
    run_command_with_error("brew", ["upgrade", "gloo"], "brew upgrade gloo")
}

fn update_windows() -> Result<(), CliError> {
    run_command_with_error("scoop", ["update"], "scoop update")?;
    run_command_with_error("scoop", ["update", "gloo"], "scoop update gloo")
}

fn update_linux() -> Result<(), CliError> {
    static LINUX_INSTALL_SCRIPT: &str =
        "https://raw.githubusercontent.com/GlooHQ/homebrew-gloo/main/install-gloo.sh";

    let command = ["-c", "curl", "-fsS", LINUX_INSTALL_SCRIPT, "|", "bash"];
    run_command_with_error("sh", &command, "install command")
}
