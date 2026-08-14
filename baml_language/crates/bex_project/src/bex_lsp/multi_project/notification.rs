//! LSP notification handlers.
//!
//! Document lifecycle discipline: the overlay map stores text *and*
//! version together, and every refresh applies them as one atomic
//! [`crate::project::SourceBatch`]. While a document is open the overlay is
//! authoritative over disk; `didClose` re-applies disk content and drops the
//! version in the same batch.

use super::{BexMultiProject, LspError, OverlayDocument, ProjectRefreshMode};
use crate::bex_lsp::notification::BexLspNotification;

impl BexLspNotification for BexMultiProject {
    fn notification_sender(
        &self,
    ) -> Box<dyn Fn(lsp_server::Notification) -> Result<(), LspError> + '_> {
        let sender = self.sender.clone();
        Box::new(move |notif| sender.send_notification(notif))
    }

    fn on_notification_cancel_request(
        &self,
        _params: lsp_notification_params!("$/cancelRequest"),
    ) -> Result<(), LspError> {
        // Accepted as a no-op: the dispatch loop is synchronous,
        // so by the time a cancel arrives its request is finished or next in
        // line. Accepting it (instead of erroring "not supported") keeps
        // clients from logging noise; bounded reads keep worst-case latency
        // finite without cooperative cancellation.
        Ok(())
    }

    fn on_notification_set_trace(
        &self,
        _params: lsp_notification_params!("$/setTrace"),
    ) -> Result<(), LspError> {
        // Trace verbosity is not implemented; accept quietly per spec.
        Ok(())
    }

    fn on_notification_will_save(
        &self,
        _params: lsp_notification_params!("textDocument/willSave"),
    ) -> Result<(), LspError> {
        // Accept quietly if a client sends this despite it not being advertised;
        // there is no pre-save work to perform.
        Ok(())
    }

    fn on_notification_exit(
        &self,
        _params: lsp_notification_params!("exit"),
    ) -> Result<(), LspError> {
        tracing::info!("LSP exit received");
        let mut projects = self.projects.lock().unwrap();
        projects.clear();
        Ok(())
    }

    fn on_notification_initialized(
        &self,
        _params: lsp_notification_params!("initialized"),
    ) -> Result<(), LspError> {
        let workspace_roots = self.workspace_roots.lock().unwrap().clone();
        self.discover_workspace_projects(&workspace_roots);

        Ok(())
    }

    fn on_notification_did_open(
        &self,
        params: lsp_notification_params!("textDocument/didOpen"),
    ) -> Result<(), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(project_root.clone())?;

        let mut in_memory_changes = project_handle.in_memory_changes.lock().unwrap();
        in_memory_changes.insert(
            crate::fs::FsPath::from_vfs(&path),
            OverlayDocument {
                text: params.text_document.text,
                version: Some(params.text_document.version),
            },
        );
        drop(in_memory_changes);

        // Full refresh: the first open in a lazily-created project must load
        // the rest of the project from disk too. Overlays win over disk.
        self.refresh_project(&project_root, ProjectRefreshMode::Full);
        Ok(())
    }

    fn on_notification_did_change_watched_files(
        &self,
        params: lsp_notification_params!("workspace/didChangeWatchedFiles"),
    ) -> Result<(), LspError> {
        let mut projects_to_update = Vec::new();
        for change in params.changes {
            let Ok(path) = self.get_path_from_uri(&change.uri) else {
                continue;
            };
            let project_root = Self::get_baml_project_root(&path)?;
            match change.typ {
                lsp_types::FileChangeType::CREATED
                | lsp_types::FileChangeType::DELETED
                | lsp_types::FileChangeType::CHANGED => {
                    projects_to_update.push(project_root);
                }
                _ => {}
            }
        }

        projects_to_update.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        projects_to_update.dedup();

        for project_root in projects_to_update {
            self.refresh_project(&project_root, ProjectRefreshMode::Full);
        }
        Ok(())
    }

    fn on_notification_did_change(
        &self,
        params: lsp_notification_params!("textDocument/didChange"),
    ) -> Result<(), LspError> {
        // Extract full text from change event (we use FULL sync mode)
        let new_text = match params.content_changes.as_slice() {
            [event] if event.range.is_none() => event.text.clone(),
            _ => {
                return Err(LspError::InvalidParams(
                    "Expected a single full-document change event (TextDocumentSyncKind::FULL)"
                        .to_string(),
                ));
            }
        };

        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project = self.get_or_create_project(project_root.clone())?;

        let mut in_memory_changes = project.in_memory_changes.lock().unwrap();
        in_memory_changes.insert(
            crate::fs::FsPath::from_vfs(&path),
            OverlayDocument {
                text: new_text,
                version: Some(params.text_document.version),
            },
        );
        drop(in_memory_changes);

        self.refresh_project(
            &project_root,
            ProjectRefreshMode::InMemoryChangesOnly {
                changed: Some(path),
            },
        );
        Ok(())
    }

    fn on_notification_did_close(
        &self,
        params: lsp_notification_params!("textDocument/didClose"),
    ) -> Result<(), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project = self.get_or_create_project(project_root.clone())?;

        let mut in_memory_changes = project.in_memory_changes.lock().unwrap();
        in_memory_changes.remove(&crate::fs::FsPath::from_vfs(&path));
        drop(in_memory_changes);

        // Re-apply disk content and drop the open-document version in the
        // same batch: future publications for this file become unversioned.
        self.refresh_project(
            &project_root,
            ProjectRefreshMode::ClosedDocuments(vec![path]),
        );
        Ok(())
    }

    fn on_notification_did_save(
        &self,
        params: lsp_notification_params!("textDocument/didSave"),
    ) -> Result<(), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project = self.get_or_create_project(project_root)?;

        // Buffer == disk at save time, so the overlay entry is redundant
        // until the next didChange re-adds it. The document stays open and
        // its version stays valid: the open-document version map is only
        // cleared by didClose.
        let mut in_memory_changes = project.in_memory_changes.lock().unwrap();
        in_memory_changes.remove(&crate::fs::FsPath::from_vfs(&path));
        drop(in_memory_changes);

        // No refresh needed: the database already holds this exact text.
        Ok(())
    }
}
