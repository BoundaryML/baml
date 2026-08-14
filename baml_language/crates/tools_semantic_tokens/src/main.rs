//! Local web viewer for BAML semantic tokens.
//!
//! A dev tool (sibling to `tools_sap_visualizer`) that serves
//! the `pkg-grammar`-style preview for *semantic* tokens instead of `TextMate`
//! scopes. Because semantic tokens are computed by the Rust compiler (not a
//! portable grammar), this tool embeds the compiler directly and serves a small
//! web UI — no VS Code, no playground, no node toolchain.
//!
//!   `cargo run -p tools_semantic_tokens`
//!
//! Browse the committed `semantic_tokens` test fixtures (diff current vs the
//! expected snapshot, accept to rewrite it), or use the scratchpad to paste
//! arbitrary BAML and see live tokens.

// Dev CLI: it prints the local URL to stdout (workspace lints deny print_*).
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod analysis;
mod server;
mod staleness;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::Result;
use clap::Parser;
use tokio::net::TcpListener;

#[derive(Parser, Debug)]
#[command(
    about = "Web viewer for BAML semantic tokens (dev tool)",
    long_about = None,
)]
struct Args {
    /// Preferred port; the next free port is used if it is taken.
    #[arg(long, default_value_t = 4319)]
    port: u16,

    /// Directory of `*.baml` semantic-token fixtures to browse.
    #[arg(long)]
    fixtures_dir: Option<PathBuf>,

    /// Do not try to open a browser on startup.
    #[arg(long)]
    no_open: bool,
}

/// The committed semantic-token fixtures, located relative to this crate so the
/// tool works regardless of the current working directory.
fn default_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../baml_lsp2_actions_tests/test_files/semantic_tokens")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    let fixtures_dir = args.fixtures_dir.unwrap_or_else(default_fixtures_dir);

    let (listener, port) = bind(args.port).await?;
    let url = format!("http://127.0.0.1:{port}");

    println!("semantic-tokens viewer  ->  {url}");
    println!("fixtures: {}", fixtures_dir.display());

    if !args.no_open {
        open_browser(&url);
    }

    // Auto-rebuild + restart when the classifier or viewer source changes.
    let started_exe_mtime = staleness::exe_mtime();
    let rebuilding = Arc::new(AtomicBool::new(false));
    staleness::spawn_watcher(started_exe_mtime, rebuilding.clone());

    axum::serve(
        listener,
        server::router(fixtures_dir, started_exe_mtime, rebuilding),
    )
    .await?;
    Ok(())
}

/// Bind to `base`, falling back to the next free port within a small range.
async fn bind(base: u16) -> Result<(TcpListener, u16)> {
    for port in base..=base.saturating_add(49) {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        if let Ok(listener) = TcpListener::bind(addr).await {
            return Ok((listener, port));
        }
    }
    anyhow::bail!("no free port in {base}..={}", base.saturating_add(49))
}

/// Best-effort open the default browser; failure is silently ignored.
fn open_browser(url: &str) {
    let command = if cfg!(target_os = "macos") {
        Some("open")
    } else if cfg!(target_os = "windows") {
        Some("explorer")
    } else if cfg!(target_os = "linux") {
        Some("xdg-open")
    } else {
        None
    };
    if let Some(command) = command {
        let _ = std::process::Command::new(command).arg(url).spawn();
    }
}
