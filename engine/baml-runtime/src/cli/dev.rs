use anyhow::{Result};
use notify_debouncer_full::{new_debouncer, notify::*};
use std::time::{Duration, Instant};
use std::{path::PathBuf};

use crate::{cli::generate::GenerateArgs, BamlRuntime};

use super::serve::Server;

#[derive(clap::Args, Clone, Debug)]
pub struct DevArgs {
    #[arg(long, help = "path/to/baml_src", default_value = "./baml_src")]
    pub(super) from: PathBuf,
    #[arg(long, help = "port to expose BAML on", default_value = "2024")]
    port: u16,
    #[arg(long, help = "turn on preview features", default_value = "false")]
    preview: bool,
}

impl DevArgs {
    pub fn run(&self, defaults: crate::RuntimeCliDefaults) -> Result<()> {
        if !self.preview {
            log::warn!(
                r#"Development mode is a preview feature.
                
Please run with --preview, like so:

    {} dev --preview

Please provide feedback and let us know if you run into any issues:

    - join our Discord at https://docs.boundaryml.com/discord, or
    - comment on https://github.com/BoundaryML/baml/issues/892

We expect to stabilize this feature over the next few weeks, but we need
your feedback to do so.

Thanks for trying out BAML!
"#,
                if matches!(
                    std::env::var("npm_lifecycle_event").ok().as_deref(),
                    Some("npx")
                ) {
                    "npx @boundaryml/baml"
                } else {
                    "baml-cli"
                }
            );
            anyhow::bail!("--preview is not set")
        }
        log::info!("Starting BAML development server on port {}", self.port);

        let t = BamlRuntime::get_tokio_singleton()?;

        let (tx, rx) = std::sync::mpsc::channel();

        // no specific tickrate, max debounce time 2 seconds
        // See https://docs.rs/notify/latest/notify/#known-problems to understand
        // known issues etc of inotify and its ilk
        let mut debouncer = new_debouncer(Duration::from_millis(200), None, tx)?;

        debouncer
            .watcher()
            .watch(self.from.as_path(), RecursiveMode::Recursive)?;

        let (server, tcp_listener) = t.block_on(Server::new(self.from.clone(), self.port))?;

        let _ = GenerateArgs {
            from: self.from.clone(),
            no_version_check: false,
        }
        .run(defaults);
        t.spawn(server.clone().serve(tcp_listener));

        // print all events and errors
        t.block_on(async {
            log::warn!(
                r#"Development mode is a preview feature.

Please provide feedback and let us know if you run into any issues:

    - join our Discord at https://docs.boundaryml.com/discord, or
    - comment on https://github.com/BoundaryML/baml/issues/892

We expect to stabilize this feature over the next few weeks, but we need
your feedback to do so.

Thanks for trying out BAML!
"#
            );
            //             log::info!(
            //                 r#"BAML development server listening on localhost:{}, watching {}

            // Tip: test that the server is up using `curl http://localhost:{}/_debug/ping`
            // "#,
            //                 self.port,
            //                 self.from.display(),
            //                 self.port
            //             );

            for result in rx {
                match result {
                    Ok(events) => {
                        log::debug!(
                            "Reloading - {}",
                            match events.len() {
                                1 => "1 file changed".to_string(),
                                n => format!("{} files changed", n),
                            }
                        );
                        let start = Instant::now();
                        match BamlRuntime::from_directory(&self.from, std::env::vars().collect()) {
                            Ok(mut new_runtime) => {
                                let elapsed = start.elapsed();
                                let _ = GenerateArgs {
                                    from: self.from.clone(),
                                    no_version_check: false,
                                }
                                .run(defaults);

                                std::mem::swap(&mut *server.b.lock().await, &mut new_runtime);
                                log::info!(
                                    "Reloaded runtime in {}ms ({})",
                                    elapsed.as_millis(),
                                    match events.len() {
                                        1 => "1 file changed".to_string(),
                                        n => format!("{} files changed", n),
                                    }
                                );
                            }
                            Err(e) => {
                                log::warn!("Failed to reload runtime: {:?}", e);
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
