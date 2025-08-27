use std::path::PathBuf;

use lsp_types::Url;
use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Maps a workspace URI to its associated client settings. Used during server initialization.
pub(crate) type WorkspaceSettingsMap = FxHashMap<Url, ClientSettings>;

#[derive(Debug, Deserialize, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub struct BamlSettings {
    pub(crate) cli_path: Option<String>,
    pub(crate) generate_code_on_save: Option<String>,
    #[serde(default = "default_feature_flags")]
    pub(crate) feature_flags: Option<Vec<String>>,
    pub(crate) client_version: Option<String>,
}

impl Default for BamlSettings {
    fn default() -> Self {
        BamlSettings {
            cli_path: None,
            generate_code_on_save: None,
            feature_flags: Some(vec!["beta".to_string()]),
            client_version: None,
        }
    }
}

impl BamlSettings {
    pub(crate) fn with_client_version(self, client_version: Option<String>) -> Self {
        Self {
            client_version,
            ..self
        }
    }

    pub fn get_client_version(&self) -> Option<&str> {
        self.client_version.as_ref().map(AsRef::as_ref)
    }
}

fn default_feature_flags() -> Option<Vec<String>> {
    Some(vec!["beta".to_string()])
}

/// This is a direct representation of the settings schema sent by the client.
#[derive(Debug, Deserialize, Default, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub struct ClientSettings {
    // These settings are only needed for tracing, and are only read from the global configuration.
    // These will not be in the resolved settings.
    #[serde(flatten)]
    pub(crate) tracing: TracingSettings,

    // BAML settings that can be provided during initialization
    #[serde(flatten)]
    pub(crate) baml: Option<BamlSettings>,
}

/// Settings needed to initialize tracing. These will only be
/// read from the global configuration.
#[derive(Debug, Deserialize, Default, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TracingSettings {
    pub(crate) log_level: Option<crate::logging::LogLevel>,
    /// Path to the log file - tildes and environment variables are supported.
    pub(crate) log_file: Option<PathBuf>,
}

/// This is a direct representation of the workspace settings schema,
/// which inherits the schema of [`ClientSettings`] and adds extra fields
/// to describe the workspace it applies to.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
struct WorkspaceSettings {
    #[serde(flatten)]
    settings: ClientSettings,
    workspace: Url,
}

/// This is the exact schema for initialization options sent in by the client
/// during initialization.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(untagged)]
enum InitializationOptions {
    #[serde(rename_all = "camelCase")]
    HasWorkspaces {
        global_settings: ClientSettings,
        #[serde(rename = "settings")]
        workspace_settings: Vec<WorkspaceSettings>,
    },
    GlobalOnly {
        #[serde(default)]
        settings: ClientSettings,
    },
}

/// Built from the initialization options provided by the client.
#[derive(Debug)]
pub(crate) struct AllSettings {
    pub(crate) global_settings: ClientSettings,
    /// If this is `None`, the client only passed in global settings.
    pub(crate) workspace_settings: Option<WorkspaceSettingsMap>,
}

impl AllSettings {
    /// Initializes the controller from the serialized initialization options.
    /// This fails if `options` are not valid initialization options.
    pub(crate) fn from_value(options: serde_json::Value) -> Self {
        tracing::info!("--- AllSettings::from_value called with: {:?}", options);
        let init_options = serde_json::from_value(options)
            .map_err(|err| {
                tracing::error!("Failed to deserialize initialization options: {err}. Falling back to default client settings...");
                show_err_msg!("Baml received invalid client settings - falling back to default client settings.");
            })
            .unwrap_or_default();
        tracing::info!(
            "--- AllSettings::from_value deserialized to: {:?}",
            init_options
        );
        Self::from_init_options(init_options)
    }

    fn from_init_options(options: InitializationOptions) -> Self {
        tracing::info!("--- from_init_options called with: {:?}", options);
        let (global_settings, workspace_settings) = match options {
            InitializationOptions::GlobalOnly { settings } => {
                tracing::info!("--- Using GlobalOnly settings: {:?}", settings);
                (settings, None)
            }
            InitializationOptions::HasWorkspaces {
                global_settings,
                workspace_settings,
            } => {
                tracing::info!(
                    "--- Using HasWorkspaces - global: {:?}, workspace: {:?}",
                    global_settings,
                    workspace_settings
                );
                (global_settings, Some(workspace_settings))
            }
        };

        tracing::info!("--- workspace_settings: {:?}", workspace_settings);
        tracing::info!("--- global_settings after match: {:?}", global_settings);

        Self {
            global_settings,
            workspace_settings: workspace_settings.map(|workspace_settings| {
                workspace_settings
                    .into_iter()
                    .map(|settings| (settings.workspace, settings.settings))
                    .collect()
            }),
        }
    }
}

impl Default for InitializationOptions {
    fn default() -> Self {
        Self::GlobalOnly {
            settings: ClientSettings::default(),
        }
    }
}
