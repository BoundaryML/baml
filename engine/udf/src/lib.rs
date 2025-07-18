pub mod config;
pub mod eval;

use std::path::Path;

use config::UDFConfig;

pub fn read_udf_config(path: impl AsRef<Path>) -> anyhow::Result<UDFConfig> {
    use anyhow::Context;
    let contents = std::fs::read_to_string(path).context("read UDF config from disk")?;

    serde_yaml::from_str(&contents).context("deserialize UDF config file")
}
