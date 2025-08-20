#![allow(dead_code)]

use std::num::NonZeroUsize;

use anyhow::Context;
pub use edit::{DocumentKey, PositionEncoding, TextDocument};
pub use session::{ClientSettings, DocumentQuery, DocumentSnapshot, Session};

use crate::server::Server;

#[macro_use]
mod message;

pub mod edit;
pub mod logging;
#[cfg(feature = "playground-server")]
pub mod playground;
pub mod playground2;
pub mod server;
pub mod session;
#[cfg(test)]
mod tests;

// additional baml modules
mod baml_project;
mod baml_source_file;
mod baml_text_size;

pub(crate) const SERVER_NAME: &str = "baml-lsp";
pub(crate) const DIAGNOSTIC_NAME: &str = "BAML";

pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub struct ServerRegistry {
    tokio_runtime: tokio::runtime::Runtime,
}

impl ServerRegistry {
    pub fn new() -> anyhow::Result<Self> {
        let tokio_runtime = tokio::runtime::Runtime::new()?;
        Ok(Self { tokio_runtime })
    }

    pub fn start_servers(&self) -> anyhow::Result<()> {
        let four = NonZeroUsize::new(4).unwrap();

        // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
        let worker_threads = std::thread::available_parallelism()
            .unwrap_or(four)
            .max(four);

        Server::new(worker_threads)
            .context("Failed to start server")?
            .run()
            .context("Failed to run server")?;

        Ok(())
    }
}

pub fn run_server() -> anyhow::Result<()> {
    ServerRegistry::new()?
        .start_servers()
        .context("Failed to start servers")?;
    Ok(())
}
