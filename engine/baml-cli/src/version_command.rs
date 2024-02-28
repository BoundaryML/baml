use crate::{builder::get_src_dir, shell::build_shell_command};
use crate::{command::run_command_with_error, errors::CliError, OutputType};
use clap;
use colored::Colorize;
use log;
use regex::Regex;
use serde;
use std::path::PathBuf;

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

    // NOTE(sam): no idea why this has to start in the cwd; this is copied from update_client.rs
    // according to vbv@ this had to be done for _some_ reason, so just preserving it as closely as i can
    let cwd = std::env::current_dir()?.canonicalize()?;
    let project_root = cwd.join(project_root);
    let project_root = project_root.canonicalize().map_err(|e| {
        CliError::StringError(format!(
            "{}\nDirectory error: {}:\n{}",
            "Failed!".red(),
            project_root.to_string_lossy(),
            e
        ))
    })?;
    cmd.current_dir(&project_root);

    let output = cmd
        .output()
        .map_err(|e| CliError::StringError(e.to_string()))
        .map_err(|e| CliError::StringError(format!("{} Error: {}", "Failed!".red(), e)))
        .and_then(|e| {
            if !e.status.success() {
                return Err(CliError::StringError(format!(
                    "{} Error: {}",
                    "Failed!".red(),
                    e.status
                )));
            }

            Ok(String::from_utf8(e.stdout)?)
        })?;

    let version_line_re = Regex::new(r#"(?i)\b(?:version)\b"#).map_err(|e| {
        CliError::StringError(format!("{} Error: {}", "Failed!".red(), e.to_string()))
    })?;

    let Some(version_line) = output.lines().find(|line| version_line_re.is_match(line)) else {
        return Err(CliError::StringError(format!(
            "{} Error: {}",
            "Failed!".red(),
            "Could not infer the version of the client"
        )));
    };

    let version_re = Regex::new("[0-9][^ ]*").map_err(|e| {
        CliError::StringError(format!("{} Error: {}", "Failed!".red(), e.to_string()))
    })?;

    let Some(version) = version_re.find(version_line) else {
        return Err(CliError::StringError(format!(
            "{} Error: {}",
            "Failed!".red(),
            "Could not parse the version of the client"
        )));
    };

    Ok(version.as_str().to_string())
}

pub fn current_should_update_to_latest(current: &str, latest: &str) -> bool {
    let Ok(current) = semver::Version::parse(current) else {
        // NB: this means we immediately return false if the current version is 0.14.0.dev0,
        // since that is not a valid semver, even though we publish it to PyPI
        return false;
    };
    let Ok(latest) = semver::Version::parse(latest) else {
        return false;
    };
    current < latest
}

/// Checks for updates to everything: CLI, client libraries, and vscode
///
///   - `baml_dir_override` is --baml-dir as passed in via the CLI, which overrides inference
///       for the nearest `baml_src` directory
///   - if we can't fetch updates, we fail explicitly
///   - if the latest versions are older than the current version, we ignore the latest version
///       and leave `$field.latest_version` unset
pub fn check_for_updates(baml_dir_override: &Option<String>) -> Result<CheckedVersions, CliError> {
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

    if let Some(cli_latest_version) = latest_versions.cli.clone() {
        if current_should_update_to_latest(&ret.cli.current_version, &cli_latest_version) {
            ret.cli.latest_version = latest_versions.cli;
        }
    };
    ret.vscode.latest_version = latest_versions.vscode;

    if let Ok((_, (config, _))) = get_src_dir(baml_dir_override) {
        for (gen, _) in config.generators {
            // every generator's client type needs to get attached to the output
            let current_version = get_client_version(
                gen.project_root.to_str().unwrap(),
                gen.package_version_command.as_str(),
            )?;
            let maybe_latest_version = match gen.language.as_str() {
                "python" => latest_versions.py_client.clone(),
                "typescript" => latest_versions.ts_client.clone(),
                _ => None,
            };
            let latest_version = if let Some(latest_version) = maybe_latest_version {
                if current_should_update_to_latest(&current_version, &latest_version) {
                    Some(latest_version)
                } else {
                    None
                }
            } else {
                None
            };

            ret.clients.push(ClientVersions {
                dir: gen.project_root.canonicalize()?,
                language: gen.language.as_str().to_string(),
                current_version: current_version,
                latest_version: latest_version,
            });
        }
    }

    Ok(ret)
}

#[derive(clap::Args, Debug)]
pub struct VersionArgs {
    /// Optional: Specifies the directory of the BAML project to test
    #[arg(long)]
    pub baml_dir: Option<String>,

    /// Check whether there are updates available for not only the CLI, but also client libraries and vscode
    #[arg(long, default_value = "false")]
    pub check: bool,

    /// Whether to output data in human or machine-readable formats
    #[arg(long = "output", default_value_t = OutputType::Human)]
    pub output_type: OutputType,
}

pub fn run(args: &VersionArgs) -> Result<(), CliError> {
    if args.check {
        // TODO- fix the ok_or
        let ret = check_for_updates(&args.baml_dir)?;
        match args.output_type {
            OutputType::Human => {
                println!(
                    "{} {} {}",
                    clap::crate_name!(),
                    ret.cli.current_version,
                    ret.cli
                        .latest_version
                        .map_or("".to_string(), |latest| format!(
                            "(update available: {})",
                            latest.green()
                        ))
                );
                for client in ret.clients {
                    println!(
                        "{} client {} {}",
                        client.language,
                        client.current_version,
                        client
                            .latest_version
                            .map_or("".to_string(), |latest| format!(
                                "(update available: {})",
                                latest.green()
                            ))
                    );
                }
                // Don't message about vscode: it's not useful in the context of human output
            }
            OutputType::Json => {
                println!("{}", serde_json::to_string_pretty(&ret)?);
            }
        }
        return Ok(());
    }

    match args.output_type {
        OutputType::Human => {
            println!("{} {}", clap::crate_name!(), clap::crate_version!());
        }
        OutputType::Json => {
            let ret = CheckedVersions {
                cli: CliVersion {
                    current_version: clap::crate_version!().to_string(),
                    latest_version: None,
                },
                clients: Vec::new(),
                vscode: VscodeVersion {
                    latest_version: None,
                },
            };
            println!("{}", serde_json::to_string_pretty(&ret)?);
        }
    }

    Ok(())
}
