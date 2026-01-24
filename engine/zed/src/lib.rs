use std::fs;

use zed_extension_api::{self as zed, LanguageServerId, Result};

#[derive(Debug)]
enum BamlExtensionLspSource {
    #[allow(dead_code)]
    LocalBuild,
    GithubRelease,
}

#[derive(Debug)]
struct HardcodedExtensionConfig {
    lsp_source: BamlExtensionLspSource,
}

const HARDCODED_EXTENSION_CONFIG: HardcodedExtensionConfig = HardcodedExtensionConfig {
    // uncomment this to use the local build
    // THIS MUST BE COMMENTED OUT WHEN MERGING
    // lsp_source: BamlExtensionLspSource::LocalBuild,
    lsp_source: BamlExtensionLspSource::GithubRelease,
};

const GITHUB_REPO: &str = "BoundaryML/baml";

fn language_server_binary(
    language_server_id: &LanguageServerId,
    _worktree: &zed::Worktree,
) -> Result<zed::Command> {
    log::info!(
        "Retrieving language server binary with settings: {:?}",
        HARDCODED_EXTENSION_CONFIG
    );
    // let binary_settings = LspSettings::for_worktree("baml", worktree)
    //     .ok()
    //     .and_then(|lsp_settings| lsp_settings.binary);
    // let binary_args = binary_settings
    //     .as_ref()
    //     .and_then(|binary_settings| binary_settings.arguments.clone());

    // if let Some(path) = binary_settings.and_then(|binary_settings| binary_settings.path) {
    //     return Ok(BamlBinary {
    //         path,
    //         args: binary_args,
    //     });
    // }

    match HARDCODED_EXTENSION_CONFIG.lsp_source {
        BamlExtensionLspSource::GithubRelease => {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::CheckingForUpdate,
            );

            let release = zed::latest_github_release(
                GITHUB_REPO,
                zed::GithubReleaseOptions {
                    require_assets: true,
                    pre_release: false,
                },
            )?;

            let (platform, arch) = zed::current_platform();
            let asset_name = format!(
                "baml-cli-{version}-{arch}-{os}{extension}",
                os = match platform {
                    zed::Os::Mac => "apple-darwin",
                    zed::Os::Linux => "unknown-linux-gnu",
                    zed::Os::Windows => "pc-windows-msvc",
                },
                arch = match arch {
                    zed::Architecture::Aarch64 => "aarch64",
                    zed::Architecture::X86 => "unsupported",
                    zed::Architecture::X8664 => "x86_64",
                },
                extension = match platform {
                    zed::Os::Mac | zed::Os::Linux => ".tar.gz",
                    zed::Os::Windows => ".zip",
                },
                version = release.version,
            );

            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == asset_name)
                .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

            let version_dir = format!("baml-cli-{}", release.version);
            let binary_path = format!(
                "{version_dir}/baml-cli{}",
                match platform {
                    zed::Os::Mac | zed::Os::Linux => "",
                    zed::Os::Windows => ".exe",
                },
            );

            if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Downloading,
                );

                zed::download_file(
                    &asset.download_url,
                    &version_dir,
                    match platform {
                        zed::Os::Mac | zed::Os::Linux => zed::DownloadedFileType::GzipTar,
                        zed::Os::Windows => zed::DownloadedFileType::Zip,
                    },
                )
                .map_err(|e| format!("failed to download file: {e}"))?;

                let entries = fs::read_dir(".")
                    .map_err(|e| format!("failed to list working directory {e}"))?;
                for entry in entries {
                    let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                    if entry.file_name().to_str() != Some(&version_dir) {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }

            Ok(zed::Command::new(binary_path).arg("lsp"))
        }
        BamlExtensionLspSource::LocalBuild => Ok(zed::Command::new(format!(
            "{}/../target/debug/language-server-hot-reload",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("lsp")
        .env("VSCODE_DEBUG_MODE", "true")),
    }
}

struct BamlExtension {}

impl zed::Extension for BamlExtension {
    fn new() -> Self {
        Self {}
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        language_server_binary(language_server_id, worktree)
    }

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(Some(zed::serde_json::json!({
            "settings": {
                "featureFlags": [],
                "generateCodeOnSave": "always",
                "lspMethodsToForwardToWebview": [
                    "runtime_updated",
                    // This allows us to update the currently shown fn/test in the webview when the
                    // user changes their cursor position in Zed.
                    // We use this instead of an "update_cursor" method because Zed doesn't have support
                    // for custom cursor update listeners.
                    "textDocument/codeAction"
                ]
            }
        })))
    }
}

zed::register_extension!(BamlExtension);
