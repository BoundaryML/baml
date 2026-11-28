use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Boundary {
    pub project: Option<String>,
    pub api_url: Option<String>,
    /// The control plane the `baml cloud` management commands talk to.
    pub services_url: Option<String>,
    /// Builds sent to Boundary carry the package's source text (BEP-088).
    #[serde(default = "enabled")]
    pub upload_source: bool,
}

fn enabled() -> bool {
    true
}

impl Default for Boundary {
    fn default() -> Self {
        Self {
            project: None,
            api_url: None,
            services_url: None,
            upload_source: true,
        }
    }
}
impl Boundary {
    pub fn read(root: &Path) -> Result<Self> {
        let path = root.join("baml.toml");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).context("Could not read Boundary project configuration");
            }
        };
        let manifest: toml::Value = toml::from_str(&text).context("Invalid baml.toml")?;
        manifest
            .get("boundary")
            .map(|value| {
                value
                    .clone()
                    .try_into()
                    .context("Invalid [boundary] settings")
            })
            .unwrap_or_else(|| Ok(Self::default()))
    }
    pub fn endpoint(&self) -> bcs_api::error::Result<bcs_api::Endpoint> {
        bcs_api::endpoint(self.api_url.as_deref())
    }
    pub fn services_endpoint(
        &self,
        api: &bcs_api::Endpoint,
    ) -> bcs_api::error::Result<bcs_api::Endpoint> {
        bcs_api::services_endpoint(self.services_url.as_deref(), api)
    }
}
pub(crate) fn login_endpoint(project_dir: Option<&Path>) -> Result<bcs_api::Endpoint> {
    let root = crate::project_load::find_project_root_from(project_dir)?
        .unwrap_or(std::env::current_dir()?);
    Ok(Boundary::read(&root)?.endpoint()?)
}
