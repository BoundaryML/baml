//! Project discovery and disk reads, off the owner thread.
//!
//! The owner never touches the filesystem: it posts a job to the executor
//! that calls the host's [`ProjectFs`] and reports back through
//! [`OwnerEvent::RootsLoaded`] / [`OwnerEvent::FilesReloaded`]. What a
//! project *is* lives in `baml_db::project_resolution` (a `baml.toml` owner
//! or a `baml_src/` owner; sources under `baml_src/` when present) so the
//! server and `baml check` load the same file set for the same directory.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::{Name, SourceRootKind};

use crate::{
    mutation::RootSpec,
    state::{GlobalState, OwnerEvent},
};

/// A project root found under (or enclosing) a folder, with the source files
/// it owns. Paths are the host's spelling; the owner canonicalizes nothing
/// here because the folder it passed in was already canonical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRoot {
    pub spec: RootSpec,
    pub files: Vec<PathBuf>,
}

/// A discovered root with its file contents read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRoot {
    pub spec: RootSpec,
    /// On-disk text of every file that was read.
    pub files: Vec<(PathBuf, String)>,
    /// Files deliberately not read because an editor buffer was open for
    /// them when the job started: the buffer is authoritative and disk is
    /// never consulted for an open document. The owner substitutes the
    /// overlay (or, if the buffer closed meanwhile, the database's current
    /// text) so the file stays in the root.
    pub unread: Vec<PathBuf>,
}

/// The host's filesystem, called only from executor jobs.
pub trait ProjectFs: Send + Sync {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// Every project root relevant to `folder`: the project enclosing the
    /// folder (if the folder sits inside one) plus every marked directory
    /// beneath it, with a manifest-less `baml_src/` root dropped when a
    /// `baml.toml` project above it already owns it.
    fn discover_roots(&self, folder: &Path) -> Vec<DiscoveredRoot>;
}

/// The `Workspace` root spec every discovered project gets: one package,
/// [`baml_type::RESERVED_USER_PACKAGE`], until packages carry real names.
pub fn workspace_root_spec(path: PathBuf) -> RootSpec {
    RootSpec {
        path,
        package: Name::new(baml_type::RESERVED_USER_PACKAGE),
        kind: SourceRootKind::Workspace,
    }
}

/// Keep every manifest project and every marker-only project that no
/// manifest project strictly contains. `roots` must be sorted and
/// deduplicated by the caller.
pub fn retain_outermost_manifest_projects(
    roots: &mut Vec<PathBuf>,
    has_manifest: impl Fn(&Path) -> bool,
) {
    let manifest_roots: Vec<PathBuf> = roots
        .iter()
        .filter(|root| has_manifest(root))
        .cloned()
        .collect();
    roots.retain(|candidate| {
        has_manifest(candidate)
            || !manifest_roots
                .iter()
                .any(|manifest| candidate != manifest && candidate.starts_with(manifest))
    });
}

/// A filesystem-less host: reads fail with `Unsupported` and discovery finds
/// nothing. Documents are still served from their editor buffers.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFs;

impl ProjectFs for NoFs {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!(
                "no filesystem is attached to this server ({})",
                path.display()
            ),
        ))
    }

    fn discover_roots(&self, _folder: &Path) -> Vec<DiscoveredRoot> {
        Vec::new()
    }
}

/// The real filesystem, for native hosts.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeFs;

#[cfg(not(target_arch = "wasm32"))]
impl NativeFs {
    /// Directories never descended into while looking for project markers,
    /// even when the workspace has no `.gitignore` to prune them.
    fn is_skipped_dir_name(name: &str) -> bool {
        name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist")
    }

    /// Marked directories (`baml.toml` file or `baml_src/` child) under
    /// `folder`, pruned by the standard ignore files. Links are not followed:
    /// a project reachable only through a symlink would otherwise appear
    /// under two spellings.
    fn scan_marked_project_roots(folder: &Path) -> Vec<PathBuf> {
        use baml_db::project_resolution::{BAML_SRC_DIR, BAML_TOML};
        let walker = ignore::WalkBuilder::new(folder)
            .standard_filters(true)
            .follow_links(false)
            .filter_entry(|entry| {
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                !(is_dir
                    && entry.depth() > 0
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(Self::is_skipped_dir_name))
            })
            .build();
        walker
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_some_and(|t| t.is_dir()))
            .map(ignore::DirEntry::into_path)
            .filter(|dir| dir.join(BAML_TOML).is_file() || dir.join(BAML_SRC_DIR).is_dir())
            .collect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ProjectFs for NativeFs {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn discover_roots(&self, folder: &Path) -> Vec<DiscoveredRoot> {
        use baml_db::project_resolution::{BAML_TOML, find_baml_project_root, project_source_root};
        let mut roots = Vec::new();
        // The folder itself may live inside a project (the user opened
        // `baml_src/` or a subdirectory).
        roots.extend(find_baml_project_root(folder));
        roots.extend(Self::scan_marked_project_roots(folder));
        roots.sort();
        roots.dedup();
        retain_outermost_manifest_projects(&mut roots, |root| root.join(BAML_TOML).is_file());
        roots
            .into_iter()
            .map(|root| {
                let files = baml_db::discover_baml_files(&project_source_root(&root));
                DiscoveredRoot {
                    spec: workspace_root_spec(root),
                    files,
                }
            })
            .collect()
    }
}

/// Read a discovered root's files, skipping documents that are open (their
/// buffers are authoritative). Unreadable files are dropped with a warning:
/// a file the server cannot read is not part of the project.
fn load_root(fs: &dyn ProjectFs, root: DiscoveredRoot, open: &HashSet<PathBuf>) -> LoadedRoot {
    let mut files = Vec::with_capacity(root.files.len());
    let mut unread = Vec::new();
    for path in root.files {
        if open.contains(&path) {
            unread.push(path);
            continue;
        }
        match fs.read_to_string(&path) {
            Ok(text) => files.push((path, text)),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "skipping unreadable source file");
            }
        }
    }
    LoadedRoot {
        spec: root.spec,
        files,
        unread,
    }
}

impl GlobalState {
    /// Discover the projects relevant to `folder` on the executor and post
    /// [`OwnerEvent::RootsLoaded`] with their contents. Files that are open
    /// right now are not read (see [`LoadedRoot::unread`]).
    pub fn spawn_discovery(&self, folder: PathBuf) {
        let fs = Arc::clone(self.fs());
        let handle = self.handle();
        let open: HashSet<PathBuf> = self.open_documents().keys().cloned().collect();
        self.executor().spawn_job(Box::new(move || {
            let roots = fs
                .discover_roots(&folder)
                .into_iter()
                .map(|root| load_root(fs.as_ref(), root, &open))
                .collect();
            handle.post(OwnerEvent::RootsLoaded {
                folder: Some(folder),
                roots,
            });
        }));
    }

    /// Re-read `paths` from disk on the executor and post
    /// [`OwnerEvent::FilesReloaded`]: `Some(text)` for a readable file,
    /// `None` for one that no longer exists. Files that exist but cannot be
    /// read are reported neither way (their database text stands).
    pub fn spawn_reload(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let fs = Arc::clone(self.fs());
        let handle = self.handle();
        self.executor().spawn_job(Box::new(move || {
            let files = paths
                .into_iter()
                .filter_map(|path| match fs.read_to_string(&path) {
                    Ok(text) => Some((path, Some(text))),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Some((path, None))
                    }
                    Err(error) => {
                        tracing::warn!(path = %path.display(), %error, "reload failed; keeping the current text");
                        None
                    }
                })
                .collect();
            handle.post(OwnerEvent::FilesReloaded { files });
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_marker_only_projects_fold_into_their_manifest_project() {
        let mut roots = vec![
            PathBuf::from("/ws/app"),
            PathBuf::from("/ws/app/baml_src_owner"),
            PathBuf::from("/ws/other"),
            PathBuf::from("/ws/other/inner"),
        ];
        let manifests: HashSet<PathBuf> =
            [PathBuf::from("/ws/app"), PathBuf::from("/ws/other/inner")]
                .into_iter()
                .collect();
        retain_outermost_manifest_projects(&mut roots, |root| manifests.contains(root));
        // `/ws/app/baml_src_owner` is inside the `/ws/app` manifest project;
        // `/ws/other` is marker-only but no manifest project contains it;
        // `/ws/other/inner` is a manifest project and stays.
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/ws/app"),
                PathBuf::from("/ws/other"),
                PathBuf::from("/ws/other/inner"),
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_discovery_finds_enclosing_and_nested_projects() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().canonicalize().unwrap();
        // A manifest project at the workspace root, with sources in baml_src.
        std::fs::write(ws.join("baml.toml"), "[package]\nname = \"p\"\n").unwrap();
        std::fs::create_dir_all(ws.join("baml_src/nested")).unwrap();
        std::fs::write(ws.join("baml_src/a.baml"), "").unwrap();
        std::fs::write(ws.join("baml_src/nested/b.baml"), "").unwrap();
        // A file outside baml_src is not part of the project's sources.
        std::fs::write(ws.join("stray.baml"), "").unwrap();
        // A marker-only directory below a manifest project folds into it.
        std::fs::create_dir_all(ws.join("tools/baml_src")).unwrap();
        // Skipped directories are never scanned.
        std::fs::create_dir_all(ws.join("node_modules/pkg/baml_src")).unwrap();

        let roots = NativeFs.discover_roots(&ws.join("baml_src/nested"));
        assert_eq!(roots.len(), 1, "{roots:?}");
        assert_eq!(roots[0].spec, workspace_root_spec(ws.clone()));
        assert_eq!(
            roots[0].files,
            vec![
                ws.join("baml_src/a.baml"),
                ws.join("baml_src/nested/b.baml")
            ]
        );

        // Discovery from the workspace root finds the same single project.
        let from_root = NativeFs.discover_roots(&ws);
        assert_eq!(from_root, roots);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_discovery_of_a_detached_folder_finds_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().canonicalize().unwrap();
        std::fs::write(dir.join("scratch.baml"), "").unwrap();
        assert!(NativeFs.discover_roots(&dir).is_empty());
    }
}
