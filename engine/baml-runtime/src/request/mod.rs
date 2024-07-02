use anyhow::{Context, Result};
use web_time::Duration;

fn builder() -> reqwest::ClientBuilder {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            let cb = reqwest::Client::builder();
        } else {
            let cb = reqwest::Client::builder()
                .http2_keep_alive_interval(Some(Duration::from_secs(10)));

        }
    }

    // NB: we can NOT set a total request timeout here: our users
    // regularly have requests that take multiple minutes, due to how
    // long LLMs take
    cb.connect_timeout(Duration::from_secs(10))
}

pub(crate) fn create_client() -> Result<reqwest::Client> {
    builder().build().context("Failed to create reqwest client")
}

pub(crate) fn create_tracing_client() -> Result<reqwest::Client> {
    builder()
        // Wait up to 30s to send traces to the backend
        .read_timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create reqwest client")
}
