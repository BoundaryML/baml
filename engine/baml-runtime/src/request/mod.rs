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
                // To prevent stalling in python, we set the pool to 0 and idle timeout to 0.
                // See:
                // https://github.com/seanmonstar/reqwest/issues/600
                // https://github.com/denoland/deno/issues/28853
                // https://github.com/hyperium/hyper/issues/2312
                // https://github.com/Azure/azure-sdk-for-rust/pull/1550
                .pool_max_idle_per_host(0)
                .pool_idle_timeout(std::time::Duration::from_nanos(1))
        }
    }
}

pub fn create_client() -> Result<reqwest::Client> {
    builder().build().context("Failed to create reqwest client")
}

pub fn create_http_client(
    http_config: &internal_llm_client::HttpConfig,
) -> Result<reqwest::Client> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            // WASM doesn't support timeouts, use default builder
            reqwest::Client::builder()
                .build()
                .context("Failed to create reqwest client")
        } else {
            let danger_accept_invalid_certs = matches!(std::env::var("DANGER_ACCEPT_INVALID_CERTS").as_deref(), Ok("1"));
            let mut builder = reqwest::Client::builder()
                .danger_accept_invalid_certs(danger_accept_invalid_certs)
                .http2_keep_alive_interval(Some(Duration::from_secs(10)))
                // To prevent stalling in python, we set the pool to 0 and idle timeout to 0.
                // See:
                // https://github.com/seanmonstar/reqwest/issues/600
                // https://github.com/denoland/deno/issues/28853
                // https://github.com/hyperium/hyper/issues/2312
                // https://github.com/Azure/azure-sdk-for-rust/pull/1550
                .pool_max_idle_per_host(0)
                .pool_idle_timeout(std::time::Duration::from_nanos(1));

            // Apply connect timeout if specified
            // Note: 0 means infinite timeout (no timeout)
            // Defaults were already applied during client creation
            if let Some(ms) = http_config.connect_timeout_ms {
                if ms > 0 {
                    builder = builder.connect_timeout(Duration::from_millis(ms));
                }
                // If ms == 0, don't set connect_timeout (infinite timeout)
            }

            // Note: request_timeout is applied per-request, not on client
            // We'll apply it when building individual requests

            builder.build().context("Failed to create reqwest client")
        }
    }
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
