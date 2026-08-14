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
                .http2_keep_alive_interval(Some(Duration::from_secs(10)));

            if !http_config.enable_connection_pooling {
                builder = builder
                // To prevent stalling in python, we set the pool to 0 and idle timeout to 0.
                // See:
                // https://github.com/seanmonstar/reqwest/issues/600
                // https://github.com/denoland/deno/issues/28853
                // https://github.com/hyperium/hyper/issues/2312
                // https://github.com/Azure/azure-sdk-for-rust/pull/1550
                .pool_max_idle_per_host(0)
                .pool_idle_timeout(std::time::Duration::from_nanos(1));
            }

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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    use super::create_http_client;

    const RESPONSE: &[u8] =
        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: keep-alive\r\n\r\nok";

    async fn serve_connection(mut socket: TcpStream) {
        let mut pending = Vec::new();
        let mut buffer = [0_u8; 4096];

        loop {
            let bytes_read = socket.read(&mut buffer).await.unwrap();
            if bytes_read == 0 {
                return;
            }
            pending.extend_from_slice(&buffer[..bytes_read]);

            while let Some(header_end) = pending.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                pending.drain(..header_end + 4);
                socket.write_all(RESPONSE).await.unwrap();
            }
        }
    }

    async fn accepted_connections(enable_connection_pooling: bool) -> usize {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let connection_count = Arc::new(AtomicUsize::new(0));
        let server_count = Arc::clone(&connection_count);
        let server = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                server_count.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(serve_connection(socket));
            }
        });

        let client = create_http_client(&internal_llm_client::HttpConfig {
            enable_connection_pooling,
            ..Default::default()
        })
        .unwrap();

        for _ in 0..3 {
            let response = client
                .get(format!("http://{address}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "ok");
        }

        let accepted = connection_count.load(Ordering::Relaxed);
        server.abort();
        accepted
    }

    #[tokio::test]
    async fn connection_pooling_is_opt_in() {
        assert_eq!(accepted_connections(false).await, 3);
        assert_eq!(accepted_connections(true).await, 1);
    }
}
