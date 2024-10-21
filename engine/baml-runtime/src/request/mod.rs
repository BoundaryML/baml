use anyhow::{Context, Result};
use web_time::Duration;

fn builder() -> reqwest::ClientBuilder {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            reqwest::Client::builder()
        } else {
            let danger_accept_invalid_certs = matches!(std::env::var("DANGER_ACCEPT_INVALID_CERTS").as_deref(), Ok("1"));
            reqwest::Client::builder()
                // NB: we can NOT set a total request timeout here: our users
                // regularly have requests that take multiple minutes, due to how
                // long LLMs take
                .connect_timeout(Duration::from_secs(10))
                .danger_accept_invalid_certs(danger_accept_invalid_certs)
                .http2_keep_alive_interval(Some(Duration::from_secs(10)))
                // We don't want to keep idle connections around due to sometimes
                // causing a stall in the connection pool across FFI boundaries
                // https://github.com/seanmonstar/reqwest/issues/600
                .pool_max_idle_per_host(0)

        }
    }
}

pub fn create_client() -> Result<reqwest::Client> {
    builder().build().context("Failed to create reqwest client")
}

pub(crate) fn create_tracing_client() -> Result<reqwest::Client> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            let cb = builder();
        } else {
            let cb = builder()
                // Wait up to 30s to send traces to the backend
                .read_timeout(Duration::from_secs(30));

        }
    }

    cb.build().context("Failed to create reqwest client")
}
