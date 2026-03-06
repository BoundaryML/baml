use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, channel},
    time::Instant,
};

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};

/// Compiler crates that should trigger a hot-reload when changed
const COMPILER_CRATES: &[&str] = &[
    "baml_base",
    "baml_builtins",
    "baml_compiler_diagnostics",
    "baml_compiler_emit",
    "baml_compiler_hir",
    "baml_compiler_lexer",
    "baml_compiler_mir",
    "baml_compiler_parser",
    "baml_compiler_syntax",
    "baml_compiler_tir",
    "baml_compiler_vir",
    "baml_db",
    "baml_project",
    "bex_vm",
    "baml_workspace",
];

/// Type of change detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// A BAML source file changed
    BamlFile,
    /// A compiler source file changed (requires rebuild)
    CompilerSource,
}

/// Diagnostic counters for debugging watcher issues
#[derive(Debug, Default)]
pub(crate) struct WatcherDiagnostics {
    pub(crate) total_raw_events: usize,
    pub(crate) baml_events: usize,
    pub(crate) compiler_events: usize,
    pub(crate) filtered_events: usize,
    pub(crate) error_events: usize,
    pub(crate) last_event_time: Option<Instant>,
    pub(crate) last_event_paths: Vec<String>,
}

pub(crate) struct FileWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<notify::Event>>,
    /// Paths that are compiler source directories (for distinguishing change types)
    compiler_paths: HashSet<PathBuf>,
    /// Whether we're watching compiler sources
    watching_compiler: bool,
    /// The path being watched (for diagnostics)
    watched_path: PathBuf,
    /// Diagnostic counters
    pub(crate) diagnostics: WatcherDiagnostics,
}

impl FileWatcher {
    /// Create a watcher for just BAML files (no compiler hot-reload)
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self> {
        Self::new_with_options(path, None)
    }

    /// Create a watcher that also watches compiler sources for hot-reload
    pub(crate) fn new_with_compiler_watch(
        baml_path: impl AsRef<Path>,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::new_with_options(baml_path, Some(workspace_root.as_ref()))
    }

    fn new_with_options(path: impl AsRef<Path>, workspace_root: Option<&Path>) -> Result<Self> {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        // Watch BAML files
        let mode = if path.as_ref().is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher.watch(path.as_ref(), mode)?;

        let mut compiler_paths = HashSet::new();
        let watching_compiler = workspace_root.is_some();

        // Optionally watch compiler source directories
        if let Some(root) = workspace_root {
            let crates_dir = root.join("crates");
            if crates_dir.exists() {
                for crate_name in COMPILER_CRATES {
                    let crate_src = crates_dir.join(crate_name).join("src");
                    if crate_src.exists() {
                        if let Err(e) = watcher.watch(&crate_src, RecursiveMode::Recursive) {
                            eprintln!("Warning: Could not watch {}: {}", crate_src.display(), e);
                        } else {
                            compiler_paths.insert(crate_src);
                        }
                    }
                }
            }
        }

        Ok(Self {
            watcher,
            receiver: rx,
            compiler_paths,
            watching_compiler,
            watched_path: path.as_ref().to_path_buf(),
            diagnostics: WatcherDiagnostics::default(),
        })
    }

    /// Check for changes and return the type of change (if any)
    pub(crate) fn check_for_changes(&mut self) -> Option<ChangeKind> {
        let mut baml_changed = false;
        let mut compiler_changed = false;

        while let Ok(result) = self.receiver.try_recv() {
            match result {
                Ok(event) => {
                    self.diagnostics.total_raw_events += 1;
                    self.diagnostics.last_event_time = Some(Instant::now());
                    self.diagnostics.last_event_paths = event
                        .paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect();

                    for path in &event.paths {
                        if self.is_compiler_path(path) {
                            compiler_changed = true;
                            self.diagnostics.compiler_events += 1;
                        } else if path.extension().is_some_and(|ext| ext == "baml") {
                            baml_changed = true;
                            self.diagnostics.baml_events += 1;
                        } else {
                            self.diagnostics.filtered_events += 1;
                        }
                    }
                }
                Err(_) => {
                    self.diagnostics.error_events += 1;
                }
            }
        }

        if compiler_changed {
            Some(ChangeKind::CompilerSource)
        } else if baml_changed {
            Some(ChangeKind::BamlFile)
        } else {
            None
        }
    }

    /// Check if a path is within the compiler source directories
    fn is_compiler_path(&self, path: &Path) -> bool {
        // Check if the path is a Rust file
        if path.extension().is_none_or(|ext| ext != "rs") {
            return false;
        }

        // Check if it's under any of our watched compiler directories
        self.compiler_paths
            .iter()
            .any(|compiler_dir| path.starts_with(compiler_dir))
    }

    /// Returns true if we're watching compiler sources for hot-reload
    pub(crate) fn is_watching_compiler(&self) -> bool {
        self.watching_compiler
    }

    pub(crate) fn watched_path(&self) -> &Path {
        &self.watched_path
    }

    /// One-line diagnostic summary for the status bar
    pub(crate) fn diagnostic_summary(&self) -> String {
        let d = &self.diagnostics;
        let age = d
            .last_event_time
            .map(|t| format!("{:.1}s ago", t.elapsed().as_secs_f64()))
            .unwrap_or_else(|| "never".into());
        format!(
            "watch: {} | raw:{} baml:{} filt:{} err:{} | last:{}",
            self.watched_path.display(),
            d.total_raw_events,
            d.baml_events,
            d.filtered_events,
            d.error_events,
            age,
        )
    }
}
