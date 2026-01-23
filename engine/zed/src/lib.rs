use std::fs;

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

enum BamlExtensionLspSource {
    LocalBuild,
    GithubRelease,
}

struct HardcodedExtensionConfig {
    lsp_source: BamlExtensionLspSource,
}

const HARDCODED_EXTENSION_CONFIG: HardcodedExtensionConfig = HardcodedExtensionConfig {
    #[cfg(feature = "debug")]
    lsp_source: BamlExtensionLspSource::LocalBuild,
    // lsp_source: BamlExtensionLspSource::GithubRelease,
    #[cfg(not(feature = "debug"))]
    lsp_source: BamlExtensionLspSource::GithubRelease,
};

// Follows csharp extension as a template:
// https://github.com/zed-extensions/csharp/blob/main/src/csharp.rs

const GITHUB_REPO: &str = "BoundaryML/baml";
struct BamlBinary {
    path: String,
    args: Option<Vec<String>>,
}

struct BamlExtension {}

impl BamlExtension {
    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
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
                println!("Checking for Github release");
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
                        let entry =
                            entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                        if entry.file_name().to_str() != Some(&version_dir) {
                            fs::remove_dir_all(entry.path()).ok();
                        }
                    }
                }

                Ok(zed::Command {
                    command: binary_path,
                    args: vec!["lsp".into()],
                    env: Default::default(),
                })
            }
            BamlExtensionLspSource::LocalBuild => Ok(zed::Command {
                command: format!(
                    "{}/../target/debug/language-server-hot-reload",
                    env!("CARGO_MANIFEST_DIR")
                ),
                args: vec!["lsp".into()],
                env: vec![("VSCODE_DEBUG_MODE".to_string(), "true".to_string())],
            }),
        }
    }
}

impl zed::Extension for BamlExtension {
    fn new() -> Self {
        Self {}
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // self.language_server_binary(language_server_id, worktree)
        // {
        //     zed::Command::new("echo")
        //         .arg("goddamn fkn echo just lemme println")
        //         .output()
        //         .unwrap();
        // }
        Ok(zed::Command::new(format!(
            "{}/../target/debug/language-server-hot-reload",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("lsp")
        .env("VSCODE_DEBUG_MODE", "true"))
        // Ok(zed::Command {
        //     command: format!(
        //         "echo",
        //         env!("CARGO_MANIFEST_DIR")
        //     ),
        //     args: vec!["lsp".into()],
        //     env: vec![("VSCODE_DEBUG_MODE".to_string(), "true".to_string())],
        // })
        // Ok(zed::Command {
        //     command: format!(
        //         "{}/../target/debug/language-server-hot-reload",
        //         env!("CARGO_MANIFEST_DIR")
        //     ),
        //     args: vec!["lsp".into()],
        //     env: vec![("VSCODE_DEBUG_MODE".to_string(), "true".to_string())],
        // })
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
