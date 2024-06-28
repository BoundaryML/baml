use anyhow::{Context, Result};
use web_time::Duration;

pub(crate) fn create_client() -> Result<reqwest::Client> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            Ok(reqwest::Client::new())
        } else {
            reqwest::Client::builder()
                .http2_keep_alive_interval(Some(Duration::from_secs(10)))
                .build()
                .context("Failed to create reqwest client")
        }
    }
}
