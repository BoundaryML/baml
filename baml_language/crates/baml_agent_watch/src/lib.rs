//! `baml agent watch`: a live view of what AI agents do to a project.
//!
//! Watches every file under a directory (not just `.baml` files), records
//! each settled write with its content, follows git as the agent commits,
//! reads the agent's transcript for what it said and ran, keeps the BAML
//! project's workflow graphs current, and streams all of it to the Agent
//! Activity page (`/agent-activity`, served from the playground's web bundle).

mod content;
mod flows;
mod git;
mod media;
mod protocol;
mod server;
mod session;
mod transcript;
mod watcher;

use std::{path::PathBuf, sync::Arc, time::Duration};

pub use flows::{FlowProvider, ProjectWorkflow, workflows_json};
pub use protocol::{LlmCall, RunFrame, RunInfo, RunKind, RunStatus};

/// First port tried when none is given; distinct from the playground's 4265.
pub const DEFAULT_PORT: u16 = 4285;
const GIT_POLL: Duration = Duration::from_secs(2);

pub struct WatchOptions {
    /// The directory to watch.
    pub root: PathBuf,
    /// Bind exactly this port; `None` scans from [`DEFAULT_PORT`].
    pub port: Option<u16>,
    /// A built web bundle; `None` uses the playground's dev-server or
    /// `BAML_PLAYGROUND_DIR` resolution.
    pub bundle_dir: Option<PathBuf>,
    /// Read agent transcripts for captions and command runs.
    pub transcripts: bool,
    /// Computes BAML workflow graphs; `None` leaves the Flow view empty.
    pub flows: Option<Arc<dyn FlowProvider>>,
    /// Called once with the bound port, before anything is served.
    pub on_listening: Option<Box<dyn FnOnce(u16) + Send>>,
}

impl std::fmt::Debug for WatchOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchOptions")
            .field("root", &self.root)
            .field("port", &self.port)
            .field("bundle_dir", &self.bundle_dir)
            .field("transcripts", &self.transcripts)
            .field("flows", &self.flows.is_some())
            .field("on_listening", &self.on_listening.is_some())
            .finish()
    }
}

/// Watch `options.root` and serve the Agent Activity page until interrupted.
pub fn run(options: WatchOptions) -> anyhow::Result<()> {
    let root = options
        .root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot watch {}: {e}", options.root.display()))?;
    let name = root.file_name().map_or_else(
        || root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    let git = git::Git::open(&root);
    let has_git = git.is_some();
    let baselines = git::Baselines::capture(&root, git);
    let session = session::Session::new(&root, name, baselines);

    let flows_tx = options.flows.clone().map(|provider| {
        // Workflows of the project the root sits in, before anything changes.
        let (first_session, first_provider) = (session.clone(), provider.clone());
        let initial = root.join(".");
        std::thread::spawn(move || {
            if let Some(project) = first_provider.project_root(&initial) {
                flows::refresh(&first_session, first_provider.as_ref(), &project);
            }
        });
        flows::spawn(session.clone(), provider)
    });
    let _watcher = watcher::spawn(session.clone(), has_git, move |files| {
        if let Some(tx) = &flows_tx {
            let _ = tx.send(files);
        }
    })?;

    if has_git {
        let session = session.clone();
        let git = git::Git::open(&root);
        std::thread::Builder::new()
            .name("agent-watch-git".into())
            .spawn(move || {
                let (Some(git), Some(start)) = (git, session.start_head().map(str::to_string))
                else {
                    return;
                };
                loop {
                    std::thread::sleep(GIT_POLL);
                    if let Some(state) = git.state(&start) {
                        session.set_git(state);
                    }
                }
            })?;
    }
    if options.transcripts {
        transcript::spawn(session.clone())?;
    }

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        use baml_lsp_server::playground_server::{bind_exact_port, pick_port};
        let (listener, port) = match options.port {
            Some(port) => (bind_exact_port(port).await?, port),
            None => pick_port(DEFAULT_PORT, 100).await?,
        };
        if let Some(announce) = options.on_listening {
            announce(port);
        }
        server::serve(listener, session, options.bundle_dir).await
    })
}
