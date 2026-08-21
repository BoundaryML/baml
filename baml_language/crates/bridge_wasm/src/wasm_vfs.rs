//! The browser's filesystem, and the [`ProjectFs`] the language server reads
//! it through.
//!
//! [`WasmVfs`] is the JS object the host hands to
//! [`crate::BamlWasmRuntime::create`] — an in-memory or origin-private file
//! system living on the JS side. Calls into it are synchronous by contract:
//! the runtime is single-threaded and the server's discovery jobs run inline
//! on the executor, so there is nothing to await into.
//!
//! [`WasmProjectFs`] is the thin adapter the LSP needs: read a file, and find
//! the projects under a folder. Project *identity* is not re-decided here —
//! [`baml_db::project_resolution`] owns the `baml.toml`-owner /
//! `baml_src/`-owner rule, and its predicate-based entry point takes this
//! filesystem's answers, so the browser and `baml check` agree on what a
//! project is.

#![expect(
    deprecated,
    reason = "tsify's into_wasm_abi/from_wasm_abi is the browser's established \
              binding for these shapes; moving to `tsify::Ts` changes what the \
              host worker receives, so it is a protocol change (the deprecation \
              is about a wasm-bindgen leak, tracked upstream)"
)]

use std::path::{Path, PathBuf};

use baml_lsp::{
    DiscoveredRoot, ProjectFs,
    discovery::{retain_outermost_manifest_projects, workspace_root_spec},
};
use js_sys::{Array, Uint8Array};
use serde::{Deserialize, Serialize};
use sys_wasm::SendWrapper;
use wasm_bindgen::{JsValue, prelude::*};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = r#"{
        readDir: (path: string) => string[];
        readDirEntries: (path: string) => WasmVfsDirEntry[];
        createDir: (path: string) => void;
        exists: (path: string) => boolean;
        readFile: (path: string) => Uint8Array;
        writeFile: (path: string, data: Uint8Array) => void;
        metadata: (path: string) => WasmVfsMetadata;
        removeFile: (path: string) => void;
        removeDir: (path: string) => void;
        setTime: (type_: "creation" | "modification" | "access", path: string, time: number) => void;
        copyFile: (src: string, dest: string) => void;
        moveFile: (src: string, dest: string) => void;
        moveDir: (src: string, dest: string) => void;
        readMany: (glob: string) => [string, Uint8Array][];
    }"#)]
    pub type WasmVfs;

    #[wasm_bindgen(method, catch, structural, js_name = readDir)]
    fn read_dir(this: &WasmVfs, path: &str) -> Result<Array, JsValue>;

    /// Directory entries with their type attached, so a listing costs one
    /// round-trip instead of N+1. Hosts that predate this method make the
    /// `Result` an error and the caller falls back to `readDir` + `metadata`.
    #[wasm_bindgen(method, catch, structural, js_name = readDirEntries)]
    fn read_dir_entries(this: &WasmVfs, path: &str) -> Result<Array, JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = createDir)]
    fn create_dir(this: &WasmVfs, path: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = exists)]
    fn exists(this: &WasmVfs, path: &str) -> Result<bool, JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = readFile)]
    fn read_file(this: &WasmVfs, path: &str) -> Result<Uint8Array, JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = writeFile)]
    fn write_file(this: &WasmVfs, path: &str, data: &Uint8Array) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = removeFile)]
    fn remove_file(this: &WasmVfs, path: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = removeDir)]
    fn remove_dir(this: &WasmVfs, path: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch, structural, js_name = metadata)]
    fn metadata(this: &WasmVfs, path: &str) -> Result<WasmVfsMetadata, JsValue>;

}

#[derive(tsify::Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WasmVfsMetadata {
    pub file_type: String,
    pub len: u64,
    pub created: Option<u64>,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
}

/// One entry of a `readDirEntries` listing.
#[derive(tsify::Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WasmVfsDirEntry {
    pub name: String,
    /// `"file"`, `"directory"`, or any host-specific string; only those two
    /// cases are interpreted.
    pub file_type: String,
    pub is_symlink: bool,
}

/// The `extern` bindings above are private to this module; these expose the
/// operations the rest of the crate needs (`baml.fs` and `baml.glob` reach
/// the host filesystem through them).
impl WasmVfs {
    pub(crate) fn vfs_read_dir(&self, path: &str) -> Result<Array, JsValue> {
        self.read_dir(path)
    }

    pub(crate) fn vfs_read_dir_entries(&self, path: &str) -> Result<Array, JsValue> {
        self.read_dir_entries(path)
    }

    pub(crate) fn vfs_create_dir(&self, path: &str) -> Result<(), JsValue> {
        self.create_dir(path)
    }

    pub(crate) fn vfs_exists(&self, path: &str) -> Result<bool, JsValue> {
        self.exists(path)
    }

    pub(crate) fn vfs_read_file(&self, path: &str) -> Result<Uint8Array, JsValue> {
        self.read_file(path)
    }

    pub(crate) fn vfs_write_file(&self, path: &str, data: &Uint8Array) -> Result<(), JsValue> {
        self.write_file(path, data)
    }

    pub(crate) fn vfs_remove_file(&self, path: &str) -> Result<(), JsValue> {
        self.remove_file(path)
    }

    pub(crate) fn vfs_remove_dir(&self, path: &str) -> Result<(), JsValue> {
        self.remove_dir(path)
    }

    pub(crate) fn vfs_metadata(&self, path: &str) -> Result<WasmVfsMetadata, JsValue> {
        self.metadata(path)
    }
}

/// How deep discovery walks below a folder looking for project markers.
///
/// The browser filesystem has no ignore files to prune with (`.gitignore`,
/// which `NativeFs` honours through `ignore::WalkBuilder`), so an unbounded
/// walk of a large mounted tree would block the single JS thread. Editable
/// browser projects are shallow; a marker further down than this is not
/// discovered, and opening a file inside it still mints a root for it.
const DISCOVERY_DEPTH: usize = 8;

/// The LSP's view of [`WasmVfs`].
///
/// `Send + Sync` is a trait requirement this target cannot honour natively —
/// `js_sys` values are `!Send` — and does not need to: wasm is
/// single-threaded and the server runs its jobs inline on the same thread.
/// [`SendWrapper`] is the crate-wide way that is spelled.
pub(crate) struct WasmProjectFs {
    vfs: SendWrapper<std::sync::Arc<WasmVfs>>,
}

impl WasmProjectFs {
    pub(crate) fn new(vfs: std::sync::Arc<WasmVfs>) -> Self {
        Self {
            vfs: SendWrapper::new(vfs),
        }
    }

    fn vfs(&self) -> &WasmVfs {
        self.vfs.inner()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.vfs()
            .vfs_metadata(&path.to_string_lossy())
            .is_ok_and(|metadata| metadata.file_type == "directory")
    }

    fn is_file(&self, path: &Path) -> bool {
        self.vfs()
            .vfs_metadata(&path.to_string_lossy())
            .is_ok_and(|metadata| metadata.file_type == "file")
    }

    /// Names of `dir`'s entries paired with whether each is a directory.
    fn entries(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
        let path = dir.to_string_lossy();
        if let Ok(entries) = self.vfs().vfs_read_dir_entries(&path) {
            return entries
                .iter()
                .filter_map(|entry| {
                    let entry: WasmVfsDirEntry = serde_wasm_bindgen::from_value(entry).ok()?;
                    (!entry.is_symlink)
                        .then(|| (dir.join(&entry.name), entry.file_type == "directory"))
                })
                .collect();
        }
        // Older hosts: one listing plus a metadata call per entry.
        let Ok(names) = self.vfs().vfs_read_dir(&path) else {
            return Vec::new();
        };
        names
            .iter()
            .filter_map(|name| name.as_string())
            .map(|name| {
                let child = dir.join(name);
                let is_dir = self.is_dir(&child);
                (child, is_dir)
            })
            .collect()
    }

    /// Every directory at or below `folder` that carries a project marker.
    fn marked_roots(&self, folder: &Path) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let mut frontier = vec![(folder.to_path_buf(), 0usize)];
        while let Some((dir, depth)) = frontier.pop() {
            if self.is_file(&dir.join(baml_db::project_resolution::BAML_TOML))
                || self.is_dir(&dir.join(baml_db::project_resolution::BAML_SRC_DIR))
            {
                roots.push(dir.clone());
            }
            if depth == DISCOVERY_DEPTH {
                continue;
            }
            for (child, is_dir) in self.entries(&dir) {
                if is_dir {
                    frontier.push((child, depth + 1));
                }
            }
        }
        roots
    }

    /// `.baml` files under a project's source root.
    fn baml_files(&self, source_root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut frontier = vec![source_root.to_path_buf()];
        while let Some(dir) = frontier.pop() {
            for (child, is_dir) in self.entries(&dir) {
                if is_dir {
                    frontier.push(child);
                } else if child.extension().is_some_and(|ext| ext == "baml") {
                    files.push(child);
                }
            }
        }
        files.sort();
        files
    }
}

impl ProjectFs for WasmProjectFs {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        let bytes = self
            .vfs()
            .vfs_read_file(&path.to_string_lossy())
            .map_err(|error| std::io::Error::other(format!("{}: {error:?}", path.display())))?;
        String::from_utf8(bytes.to_vec()).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {error}", path.display()),
            )
        })
    }

    fn discover_roots(&self, folder: &Path) -> Vec<DiscoveredRoot> {
        let mut roots = Vec::new();
        // The folder itself may sit inside a project (the host opened
        // `baml_src/`, or a subdirectory of it).
        roots.extend(
            baml_db::project_resolution::find_baml_project_root_from_ancestors(
                folder.ancestors().map(Path::to_path_buf),
                |dir| self.is_file(&dir.join(baml_db::project_resolution::BAML_TOML)),
                |dir| self.is_dir(&dir.join(baml_db::project_resolution::BAML_SRC_DIR)),
            ),
        );
        roots.extend(self.marked_roots(folder));
        roots.sort();
        roots.dedup();
        retain_outermost_manifest_projects(&mut roots, |root| {
            self.is_file(&root.join(baml_db::project_resolution::BAML_TOML))
        });
        roots
            .into_iter()
            .map(|root| {
                let source_root = root.join(baml_db::project_resolution::BAML_SRC_DIR);
                let source_root = if self.is_dir(&source_root) {
                    source_root
                } else {
                    root.clone()
                };
                DiscoveredRoot {
                    files: self.baml_files(&source_root),
                    spec: workspace_root_spec(root),
                }
            })
            .collect()
    }
}
