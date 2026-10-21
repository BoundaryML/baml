use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Boundary {
    pub project: Option<String>,
    pub api_url: Option<String>,
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
        bcs_api::Endpoint::with_default(self.api_url.as_deref())
    }
}
pub(crate) fn login_endpoint() -> Result<bcs_api::Endpoint> {
    let root =
        crate::project_load::find_project_root_from(None)?.unwrap_or(std::env::current_dir()?);
    Ok(Boundary::read(&root)?.endpoint()?)
}
