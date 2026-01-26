use std::{
    ops::DerefMut,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Result;
use notify_debouncer_full::{new_debouncer, notify::*};

use super::serve::Server;
use crate::{
    cli::{dotenv::DotenvArgs, generate::GenerateArgs},
    BamlRuntime,
};

#[derive(clap::Args, Clone, Debug)]
pub struct DevArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub from: PathBuf,
    #[arg(long, help = "port to expose BAML on", default_value = "2024")]
    port: u16,
    #[command(flatten)]
    dotenv: DotenvArgs,
}

impl DevArgs {
    pub fn run(
        &self,
        defaults: crate::RuntimeCliDefaults,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<()> {
        self.dotenv.load()?;

        baml_log::info!("Starting BAML development server on port {}", self.port);

        let t = BamlRuntime::get_tokio_singleton()?;

        let (tx, rx) = std::sync::mpsc::channel();

        // no specific tickrate, max debounce time 2 seconds
        // See https://docs.rs/notify/latest/notify/#known-problems to understand
        // known issues etc of inotify and its ilk
        let mut debouncer = new_debouncer(Duration::from_millis(200), None, tx)?;

        debouncer
            .watcher()
            .watch(self.from.as_path(), RecursiveMode::Recursive)?;

        let (server, tcp_listener) = t.block_on(Server::new(
            self.from.clone(),
            self.port,
            feature_flags.clone(),
        ))?;

        let _ = GenerateArgs {
            from: self.from.clone(),
            no_version_check: false,
            no_tests: false,
        }
        .run(defaults, feature_flags.clone());
        t.spawn(server.clone().serve(tcp_listener));

        // print all events and errors
        t.block_on(async {
            for result in rx {
                match result {
                    Ok(events) => {
                        log::debug!(
                            "Reloading - {}",
                            match events.len() {
                                1 => "1 file changed".to_string(),
                                n => format!("{n} files changed"),
                            }
                        );
                        let start = Instant::now();
                        match BamlRuntime::from_directory(
                            &self.from,
                            std::env::vars().collect(),
                            feature_flags.clone(),
                        ) {
                            Ok(mut new_runtime) => {
                                let elapsed = start.elapsed();
                                let _ = GenerateArgs {
                                    from: self.from.clone(),
                                    no_version_check: false,
                                    no_tests: false,
                                }
                                .run(defaults, feature_flags.clone());

                                std::mem::swap(
                                    server.b.write().await.deref_mut(),
                                    &mut new_runtime,
                                );
                                baml_log::info!(
                                    "Reloaded runtime in {}ms ({})",
                                    elapsed.as_millis(),
                                    match events.len() {
                                        1 => "1 file changed".to_string(),
                                        n => format!("{n} files changed"),
                                    }
                                );
                            }
                            Err(e) => {
                                log::warn!("Failed to reload runtime: {e:?}");
                            }
                        }
                    }
                    Err(errors) => {
                        log::warn!(
                            "Encountered errors while watching {}: {:?}",
                            self.from.display(),
                            errors
                        );
                    }
                }
            }
        });

        Ok(())
    }
}
