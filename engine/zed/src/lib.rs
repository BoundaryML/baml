use std::fs;

use zed_extension_api::{
    self as zed, LanguageServerId, Result,
    http_client::{HttpMethod, HttpRequest, RedirectPolicy},
    settings::LspSettings,
};

const MANIFEST_BASE_URL: &str = "https://pkg.boundaryml.com/manifest/v1";
const DEFAULT_CHANNEL: &str = "canary";
const LANGUAGE_SERVER_ID: &str = "baml-language-server";

struct BamlExtension {}

impl zed::Extension for BamlExtension {
    fn new() -> Self {
        Self {}
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        if let Some(command) = configured_command(worktree) {
            return Ok(command);
        }
        if let Some(baml) = worktree.which("baml") {
            return Ok(zed::Command::new(baml).arg("lsp").envs(worktree.shell_env()));
        }
        downloaded_command(language_server_id, worktree)
    }

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(None)
    }
}

/// Honour an explicit `lsp.baml-language-server.binary` override from Zed settings.
fn configured_command(worktree: &zed::Worktree) -> Option<zed::Command> {
    let binary = LspSettings::for_worktree(LANGUAGE_SERVER_ID, worktree)
        .ok()?
        .binary?;
    let path = binary.path?;
    let arguments = binary.arguments.unwrap_or_else(|| vec!["lsp".to_string()]);
    let mut command = zed::Command::new(path)
        .args(arguments)
        .envs(worktree.shell_env());
    if let Some(env) = binary.env {
        command = command.envs(env);
    }
    Some(command)
}

enum Selector {
    Channel(String),
    Version(String),
}

/// Last resort when no `baml` wrapper is installed: fetch a toolchain ourselves.
fn downloaded_command(
    language_server_id: &LanguageServerId,
    worktree: &zed::Worktree,
) -> Result<zed::Command> {
    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::CheckingForUpdate,
    );

    let target = host_target()?;
    let manifest = fetch_manifest(&manifest_url(&resolve_selector(worktree)))?;
    let version = manifest
        .get("version")
        .and_then(zed::serde_json::Value::as_str)
        .ok_or_else(|| "BAML manifest is missing a version".to_string())?;
    let url = manifest
        .get("artifacts")
        .and_then(|artifacts| artifacts.get(target.as_str()))
        .and_then(|artifact| artifact.get("url"))
        .and_then(zed::serde_json::Value::as_str)
        .ok_or_else(|| format!("BAML manifest has no artifact for {target}"))?;

    let (platform, _) = zed::current_platform();
    let exe = if platform == zed::Os::Windows { ".exe" } else { "" };
    let version_dir = format!("baml-language-{version}");
    let binary_path = format!("{version_dir}/bin/baml-cli{exe}");

    if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );
        let file_type = match platform {
            zed::Os::Windows => zed::DownloadedFileType::Zip,
            _ => zed::DownloadedFileType::GzipTar,
        };
        zed::download_file(url, &version_dir, file_type)
            .map_err(|err| format!("failed to download BAML toolchain {version}: {err}"))?;
        remove_other_entries(&version_dir);
    }

    Ok(zed::Command::new(binary_path)
        .arg("lsp")
        .envs(worktree.shell_env()))
}

/// Mirror the wrapper's resolution (BAML_VERSION, then baml.toml) so a downloaded toolchain honours a pin.
fn resolve_selector(worktree: &zed::Worktree) -> Selector {
    if let Some(value) = env_value(worktree, "BAML_VERSION") {
        return classify(value);
    }
    if let Some(selector) = project_selector(worktree) {
        return selector;
    }
    Selector::Channel(DEFAULT_CHANNEL.to_string())
}

fn classify(selector: String) -> Selector {
    if selector == "canary" || selector == "nightly" {
        Selector::Channel(selector)
    } else {
        Selector::Version(selector)
    }
}

fn env_value(worktree: &zed::Worktree, key: &str) -> Option<String> {
    worktree
        .shell_env()
        .into_iter()
        .find(|(name, _)| name.as_str() == key)
        .map(|(_, value)| value)
        .filter(|value| !value.trim().is_empty())
}

fn project_selector(worktree: &zed::Worktree) -> Option<Selector> {
    let text = worktree.read_text_file("baml.toml").ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let toolchain = value.get("toolchain")?;
    if let Some(version) = toolchain.get("version").and_then(toml::Value::as_str) {
        return Some(Selector::Version(version.to_string()));
    }
    let channel = toolchain.get("channel").and_then(toml::Value::as_str)?;
    Some(classify(channel.to_string()))
}

fn manifest_url(selector: &Selector) -> String {
    match selector {
        Selector::Channel(channel) => format!("{MANIFEST_BASE_URL}/{channel}.json"),
        Selector::Version(version) => format!("{MANIFEST_BASE_URL}/version/{version}.json"),
    }
}

fn fetch_manifest(url: &str) -> Result<zed::serde_json::Value> {
    let request = HttpRequest::builder()
        .method(HttpMethod::Get)
        .url(url)
        .redirect_policy(RedirectPolicy::FollowAll)
        .build()?;
    let response = zed::http_client::fetch(&request)?;
    zed::serde_json::from_slice(&response.body)
        .map_err(|err| format!("failed to parse BAML manifest from {url}: {err}"))
}

fn host_target() -> Result<String> {
    let (platform, arch) = zed::current_platform();
    let os = match platform {
        zed::Os::Mac => "apple-darwin",
        zed::Os::Linux => "unknown-linux-gnu",
        zed::Os::Windows => "pc-windows-msvc",
    };
    let arch = match arch {
        zed::Architecture::Aarch64 => "aarch64",
        zed::Architecture::X8664 => "x86_64",
        zed::Architecture::X86 => return Err("BAML has no 32-bit x86 toolchain".to_string()),
    };
    Ok(format!("{arch}-{os}"))
}

fn remove_other_entries(keep: &str) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_str() != Some(keep) {
            fs::remove_dir_all(entry.path()).ok();
        }
    }
}

zed::register_extension!(BamlExtension);
