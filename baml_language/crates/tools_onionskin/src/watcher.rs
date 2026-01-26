use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, channel},
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

pub(crate) struct FileWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<notify::Event>>,
    /// Paths that are compiler source directories (for distinguishing change types)
    compiler_paths: HashSet<PathBuf>,
    /// Whether we're watching compiler sources
    watching_compiler: bool,
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
        })
    }

    /// Check for changes and return the type of change (if any)
    pub(crate) fn check_for_changes(&self) -> Option<ChangeKind> {
        // Collect all pending events
        let mut baml_changed = false;
        let mut compiler_changed = false;

        while let Ok(result) = self.receiver.try_recv() {
            if let Ok(event) = result {
                // Check if any affected path is a compiler source
                for path in &event.paths {
                    if self.is_compiler_path(path) {
                        compiler_changed = true;
                    } else if path.extension().is_some_and(|ext| ext == "baml") {
                        baml_changed = true;
                    }
                }
            }
        }

        // Compiler changes take priority (trigger rebuild)
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
}
