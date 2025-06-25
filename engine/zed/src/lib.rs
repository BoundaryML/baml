use std::fs;

use zed_extension_api::{self as zed, LanguageServerId, Result, settings::LspSettings};

// Follows csharp extension as a template:
// https://github.com/zed-extensions/csharp/blob/main/src/csharp.rs

const GITHUB_REPO: &str = "BoundaryML/baml";

// Embed the binary for debug mode
#[cfg(feature = "debug")]
const BAML_CLI_BINARY: &[u8] = include_bytes!("../../target/debug/baml-cli");
// const BAML_CLI_BINARY: &[u8] = include_bytes!("../baml-cli");
struct BamlBinary {
    path: String,
    args: Option<Vec<String>>,
}

struct BamlExtension {
    cached_binary_path: Option<String>,
}

impl BamlExtension {
    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<BamlBinary> {
        let binary_settings = LspSettings::for_worktree("baml", worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.binary);
        let binary_args = binary_settings
            .as_ref()
            .and_then(|binary_settings| binary_settings.arguments.clone());

        if let Some(path) = binary_settings.and_then(|binary_settings| binary_settings.path) {
            return Ok(BamlBinary {
                path,
                args: binary_args,
            });
        }

        if let Some(path) = &self.cached_binary_path {
            if let Ok(stat) = fs::metadata(path) {
                if stat.is_file() {
                    return Ok(BamlBinary {
                        path: path.clone(),
                        args: binary_args,
                    });
                }
            }
        }

        #[cfg(feature = "debug")]
        {
            let binary_path = "baml-cli";
            fs::write(binary_path, BAML_CLI_BINARY)
                .map_err(|e| format!("failed to write embedded binary: {}", e))?;
            zed::make_file_executable(binary_path)?;
            self.cached_binary_path = Some(binary_path.to_string());
            return Ok(BamlBinary {
                path: binary_path.to_string(),
                args: binary_args,
            });
        }

        #[cfg(not(feature = "debug"))]
        {
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
                    zed::Os::Windows => ".exe",
                },
                version = release.version,
            );

            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == asset_name)
                .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

            let version_dir = format!("baml-cli-{}", release.version);
            let binary_path = format!("{version_dir}/baml-cli");

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

            self.cached_binary_path = Some(binary_path.clone());
            Ok(BamlBinary {
                path: binary_path,
                args: binary_args,
            })
        }
    }
}

impl zed::Extension for BamlExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let baml_binary = self.language_server_binary(language_server_id, worktree)?;
        Ok(zed::Command {
            command: baml_binary.path,
            args: baml_binary.args.unwrap_or_else(|| vec!["lsp".into()]),
            env: Default::default(),
        })
    }

    // fn language_server_initialization_options(
    //     &mut self,
    //     _language_server_id: &LanguageServerId,
    //     _worktree: &zed::Worktree,
    // ) -> Result<Option<zed::serde_json::Value>> {
    //     Ok(Some(zed::serde_json::json!({
    //         "watchPatterns": ["**/baml_src/**/*.baml", "**/baml_src/**/*.json"]
    //     })))
    // }
}

zed::register_extension!(BamlExtension);
