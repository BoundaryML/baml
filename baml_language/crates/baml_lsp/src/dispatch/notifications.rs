//! Notification handlers: owner-inline, each a [`SourceMutation`] batch and
//! at most a job posted to the executor.
//!
//! Document lifecycle: `didOpen` tracks the buffer and applies it as an
//! overlay (minting a provisional root when no root contains the file, and
//! asking the executor to discover the enclosing project); `didChange`
//! replaces the overlay; `didClose` drops it and reconciles with disk. The
//! opened document is never read from disk. Watched-file events touch only
//! the paths they name.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::SourceRootKind;
use baml_db::project_resolution::{BAML_SRC_DIR, BAML_TOML};
use lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, FileChangeType, InitializedParams, WillSaveTextDocumentParams,
};

use crate::{
    discovery::workspace_root_spec,
    error::LspError,
    mutation::SourceMutation,
    paths,
    state::{Applied, GlobalState, OpenDocument, SessionKey},
};

/// Whether documents under this root may carry editor overlays and be
/// reconciled with disk. Exhaustive so a new kind must decide.
fn is_editable(kind: SourceRootKind) -> bool {
    match kind {
        SourceRootKind::Workspace => true,
        SourceRootKind::Stdlib | SourceRootKind::Dependency | SourceRootKind::Dynamic => false,
    }
}

/// The first rejection of a batch as the handler's error, so the host can
/// log why a notification had no effect.
fn first_rejection(applied: Applied) -> Result<(), LspError> {
    match applied.rejected.into_iter().next() {
        Some((mutation, error)) => {
            tracing::warn!(?mutation, %error, "mutation rejected");
            Err(error)
        }
        None => Ok(()),
    }
}

/// A `baml.toml` file or a `baml_src` directory: its parent's project set
/// may have changed.
fn is_project_marker(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == BAML_TOML || name == BAML_SRC_DIR)
}

fn is_baml_source(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "baml")
}

// ── Lifecycle ────────────────────────────────────────────────────────────

pub(super) fn initialized(
    state: &mut GlobalState,
    session: SessionKey,
    _params: InitializedParams,
) -> Result<(), LspError> {
    let folders = state.session(session)?.workspace_folders.clone();
    if folders.is_empty() {
        tracing::info!("no workspace folders; projects are discovered from opened documents");
    }
    for folder in folders {
        state.spawn_discovery(folder);
    }
    Ok(())
}

/// The host terminates the process; nothing to tear down here.
#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn exit(_state: &mut GlobalState, _session: SessionKey, (): ()) -> Result<(), LspError> {
    Ok(())
}

/// The transport claims the response on its own thread (the client gets
/// `RequestCanceled` promptly; no double-respond is possible). Cutting the
/// *running* query short via its snapshot's Salsa token is not wired yet:
/// the pool job runs to completion and its result is discarded at the
/// claimed response. Wire this together with a request-id → snapshot-token
/// map when the remaining request handlers land.
#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn cancel_request(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: lsp_types::CancelParams,
) -> Result<(), LspError> {
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn set_trace(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: lsp_types::SetTraceParams,
) -> Result<(), LspError> {
    Ok(())
}

// ── Documents ────────────────────────────────────────────────────────────

pub(super) fn did_open(
    state: &mut GlobalState,
    session: SessionKey,
    params: DidOpenTextDocumentParams,
) -> Result<(), LspError> {
    let lsp_types::TextDocumentItem {
        uri, version, text, ..
    } = params.text_document;
    let path = paths::canonical_document_path(state.roots(), &uri)?;

    let provisional_root = match state.roots().root_for_path(&path) {
        Some(entry) if is_editable(entry.kind) => None,
        Some(entry) => {
            tracing::debug!(
                path = %path.display(),
                root = %entry.path.display(),
                "document is in a read-only root; buffer not tracked"
            );
            return Ok(());
        }
        // No root contains the file: serve it now from a provisional root at
        // its directory, and let discovery find the enclosing project.
        None => {
            Some(
                path.parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| LspError::InvalidPath {
                        path: path.clone(),
                        message: "document has no parent directory".to_owned(),
                    })?,
            )
        }
    };

    state.track_open_document(
        path.clone(),
        OpenDocument {
            uri,
            version: Some(version),
            session,
            text: Arc::from(text.as_str()),
        },
    );
    let mut batch = Vec::with_capacity(2);
    if let Some(root) = &provisional_root {
        // The overlay just tracked is merged into the root's file set.
        batch.push(SourceMutation::UpsertRoot {
            spec: workspace_root_spec(root.clone()),
            files: Vec::new(),
        });
    }
    batch.push(SourceMutation::SetOverlay {
        path,
        text,
        version: Some(version),
    });
    let applied = state.apply(batch);
    if let Some(root) = provisional_root
        && !applied
            .rejected
            .iter()
            .any(|(mutation, _)| matches!(mutation, SourceMutation::UpsertRoot { .. }))
    {
        state.mark_provisional_root(root.clone());
        state.spawn_discovery(root);
    }
    first_rejection(applied)
}

/// FULL sync only: exactly one change event with no range.
#[expect(
    clippy::needless_pass_by_value,
    reason = "uniform dispatch-table signature"
)]
pub(super) fn did_change(
    state: &mut GlobalState,
    _session: SessionKey,
    params: DidChangeTextDocumentParams,
) -> Result<(), LspError> {
    let text = match params.content_changes.as_slice() {
        [event] if event.range.is_none() => event.text.clone(),
        _ => {
            return Err(LspError::InvalidParams(
                "expected a single full-document change event (TextDocumentSyncKind::FULL)"
                    .to_owned(),
            ));
        }
    };
    let path = paths::canonical_document_path(state.roots(), &params.text_document.uri)?;
    if state.open_document(&path).is_none() {
        // Read-only roots are never tracked; anything else is a protocol
        // violation (didChange before didOpen).
        return match state.roots().root_for_path(&path) {
            Some(entry) if !is_editable(entry.kind) => Ok(()),
            Some(_) | None => Err(LspError::InvalidParams(format!(
                "didChange for a document that is not open: {}",
                path.display()
            ))),
        };
    }
    let applied = state.apply(vec![SourceMutation::SetOverlay {
        path,
        text,
        version: Some(params.text_document.version),
    }]);
    first_rejection(applied)
}

#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn will_save(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: WillSaveTextDocumentParams,
) -> Result<(), LspError> {
    Ok(())
}

/// The buffer already is the database text; disk catching up changes
/// nothing.
#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn did_save(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: DidSaveTextDocumentParams,
) -> Result<(), LspError> {
    Ok(())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "uniform dispatch-table signature"
)]
pub(super) fn did_close(
    state: &mut GlobalState,
    _session: SessionKey,
    params: DidCloseTextDocumentParams,
) -> Result<(), LspError> {
    let path = paths::canonical_document_path(state.roots(), &params.text_document.uri)?;
    state.close_document(&path);
    Ok(())
}

impl GlobalState {
    /// Drop a document's overlay. The text stays until the disk reload
    /// posted here reconciles it (`SetDisk`/`RemoveFile`); a provisional
    /// root losing its last document is removed instead — it existed only
    /// for that document. Closing an untracked path is a no-op.
    pub fn close_document(&mut self, path: &Path) {
        if self.open_document(path).is_none() {
            return;
        }
        let provisional_root = self
            .roots()
            .root_for_path(path)
            .filter(|entry| self.is_provisional_root(&entry.path))
            .map(|entry| entry.path.clone())
            .filter(|root| {
                self.open_documents_under(root)
                    .all(|(open, _)| open == path)
            });
        let mut batch = vec![SourceMutation::CloseDocument {
            path: path.to_path_buf(),
        }];
        match provisional_root {
            Some(root) => {
                batch.push(SourceMutation::RemoveRoot { path: root });
                let applied = self.apply(batch);
                log_rejections(&applied, "didClose");
            }
            None => {
                let applied = self.apply(batch);
                log_rejections(&applied, "didClose");
                self.spawn_reload(vec![path.to_path_buf()]);
            }
        }
    }
}

pub(super) fn log_rejections(applied: &Applied, context: &str) {
    for (mutation, error) in &applied.rejected {
        tracing::warn!(context, ?mutation, %error, "mutation rejected");
    }
}

// ── Workspace ────────────────────────────────────────────────────────────

#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn did_change_configuration(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: DidChangeConfigurationParams,
) -> Result<(), LspError> {
    Ok(())
}

/// Only the named URIs are touched: a deleted source leaves the database
/// inline; created/changed sources are re-read on the executor; a project
/// marker (`baml.toml`, `baml_src`) re-runs discovery for its directory.
#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn did_change_watched_files(
    state: &mut GlobalState,
    _session: SessionKey,
    params: DidChangeWatchedFilesParams,
) -> Result<(), LspError> {
    let mut rediscover: BTreeSet<PathBuf> = BTreeSet::new();
    let mut reload: Vec<PathBuf> = Vec::new();
    let mut batch: Vec<SourceMutation> = Vec::new();
    for change in params.changes {
        let path = match paths::canonical_document_path(state.roots(), &change.uri) {
            Ok(path) => path,
            Err(error) => {
                tracing::debug!(uri = %change.uri, %error, "ignoring watched-file event");
                continue;
            }
        };
        if is_project_marker(&path) {
            rediscover.extend(path.parent().map(Path::to_path_buf));
            continue;
        }
        if !is_baml_source(&path) {
            continue;
        }
        if state
            .roots()
            .root_for_path(&path)
            .is_some_and(|entry| !is_editable(entry.kind))
        {
            continue;
        }
        match change.typ {
            FileChangeType::DELETED => batch.push(SourceMutation::RemoveFile { path }),
            FileChangeType::CREATED | FileChangeType::CHANGED => reload.push(path),
            other => tracing::debug!(?other, "unknown file change type"),
        }
    }
    let applied = state.apply(batch);
    log_rejections(&applied, "didChangeWatchedFiles");
    state.spawn_reload(reload);
    for folder in rediscover {
        state.spawn_discovery(folder);
    }
    Ok(())
}

/// Added folders are discovered; removed folders drop the workspace roots
/// under them that no other session's folder still covers and that have no
/// open documents.
#[expect(
    clippy::needless_pass_by_value,
    reason = "uniform dispatch-table signature"
)]
pub(super) fn did_change_workspace_folders(
    state: &mut GlobalState,
    session: SessionKey,
    params: DidChangeWorkspaceFoldersParams,
) -> Result<(), LspError> {
    let added: Vec<PathBuf> = params
        .event
        .added
        .iter()
        .filter_map(|folder| paths::canonical_document_path(state.roots(), &folder.uri).ok())
        .collect();
    let removed: Vec<PathBuf> = params
        .event
        .removed
        .iter()
        .filter_map(|folder| paths::canonical_document_path(state.roots(), &folder.uri).ok())
        .collect();

    {
        let folders = &mut state.session_mut(session)?.workspace_folders;
        folders.retain(|folder| !removed.contains(folder));
        for folder in &added {
            if !folders.contains(folder) {
                folders.push(folder.clone());
            }
        }
    }

    let orphaned: Vec<PathBuf> = state
        .roots()
        .workspace_roots()
        .filter(|entry| removed.iter().any(|folder| entry.path.starts_with(folder)))
        .filter(|entry| {
            !state.sessions().any(|(_, s)| {
                s.workspace_folders
                    .iter()
                    .any(|folder| entry.path.starts_with(folder))
            })
        })
        .filter(|entry| state.open_documents_under(&entry.path).next().is_none())
        .map(|entry| entry.path.clone())
        .collect();
    let applied = state.apply(
        orphaned
            .into_iter()
            .map(|path| SourceMutation::RemoveRoot { path })
            .collect(),
    );
    log_rejections(&applied, "didChangeWorkspaceFolders");

    for folder in added {
        state.spawn_discovery(folder);
    }
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn did_create_files(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: lsp_types::CreateFilesParams,
) -> Result<(), LspError> {
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn did_rename_files(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: lsp_types::RenameFilesParams,
) -> Result<(), LspError> {
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "uniform dispatch-table signature")]
pub(super) fn did_delete_files(
    _state: &mut GlobalState,
    _session: SessionKey,
    _params: lsp_types::DeleteFilesParams,
) -> Result<(), LspError> {
    Ok(())
}
