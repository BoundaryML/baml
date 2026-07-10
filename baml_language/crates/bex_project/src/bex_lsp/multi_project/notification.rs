use super::{BexMulitProject, LspError, ProjectRefreshMode, canonical_fs_path_identity};
use crate::bex_lsp::notification::BexLspNotification;

impl BexLspNotification for BexMulitProject {
    fn notification_sender(
        &self,
    ) -> Box<dyn Fn(lsp_server::Notification) -> Result<(), LspError> + '_> {
        let sender = self.sender.clone();
        Box::new(move |notif| sender.send_notification(notif))
    }

    fn on_notification_exit(
        &self,
        _params: lsp_notification_params!("exit"),
    ) -> Result<(), LspError> {
        tracing::info!("LSP exit received");
        // Project and playground state outlive an individual LSP transport.
        // Session revocation is owned by the ingress runtime.
        Ok(())
    }

    fn on_notification_initialized(
        &self,
        _params: lsp_notification_params!("initialized"),
    ) -> Result<(), LspError> {
        let _ = self.session_config()?;
        let workspace_roots = self.workspace_roots.lock().unwrap().clone();
        self.discover_workspace_projects(&workspace_roots);
        // Initialization is a subscriber boundary. A preload or an earlier
        // endpoint may have warmed catalog dedupe, but this session still
        // requires its own complete catalog after the handshake.
        self.send_list_projects(true);

        Ok(())
    }

    fn on_notification_did_open(
        &self,
        params: lsp_notification_params!("textDocument/didOpen"),
    ) -> Result<(), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(project_root.clone())?;
        let sources = self.load_project_sources(&project_root)?;
        let document_path = canonical_fs_path_identity(&crate::fs::FsPath::from_vfs(&path));
        let document = crate::project::OpenDocument {
            client_uri: params.text_document.uri.clone(),
            version: params.text_document.version,
            text: params.text_document.text.clone(),
        };
        project_handle.project.apply_all_sources_and_open_document(
            &sources,
            document_path.clone(),
            params.text_document.uri,
            params.text_document.version,
            params.text_document.text,
        );
        self.open_documents
            .lock()
            .unwrap()
            .insert(document_path, document);
        self.refresh_project(
            &project_root,
            ProjectRefreshMode::Applied {
                full_diagnostic_refresh: true,
            },
        );
        Ok(())
    }

    fn on_notification_did_change_watched_files(
        &self,
        params: lsp_notification_params!("workspace/didChangeWatchedFiles"),
    ) -> Result<(), LspError> {
        let mut projects_to_update = Vec::new();
        let mut rediscover = false;
        for change in params.changes {
            let Ok(path) = self.get_path_from_uri(&change.uri) else {
                continue;
            };
            match change.typ {
                lsp_types::FileChangeType::CREATED | lsp_types::FileChangeType::DELETED => {
                    // A marker/root may have appeared or disappeared, and a
                    // deleted path no longer has ancestors that the ordinary
                    // resolver can inspect. Reconcile the complete scoped
                    // catalog rather than keeping a zombie LiveProject.
                    rediscover = true;
                    if let Ok(project_root) = Self::get_baml_project_root(&path) {
                        projects_to_update.push(project_root);
                    }
                }
                lsp_types::FileChangeType::CHANGED => {
                    if let Ok(project_root) = Self::get_baml_project_root(&path) {
                        projects_to_update.push(project_root);
                    }
                }
                _ => {}
            }
        }

        if rediscover {
            let workspace_roots = self.workspace_roots.lock().unwrap().clone();
            self.discover_workspace_projects(&workspace_roots);
            return Ok(());
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
                return Err(LspError::RequestNotSupported(
                    "Expected a single full-document change event (TextDocumentSyncKind::FULL)"
                        .to_string(),
                ));
            }
        };

        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project = self.get_or_create_project(project_root.clone())?;
        let document_path = canonical_fs_path_identity(&crate::fs::FsPath::from_vfs(&path));
        let document = crate::project::OpenDocument {
            client_uri: params.text_document.uri.clone(),
            version: params.text_document.version,
            text: new_text.clone(),
        };
        project.project.apply_open_document(
            document_path.clone(),
            params.text_document.uri,
            params.text_document.version,
            new_text,
        );
        self.open_documents
            .lock()
            .unwrap()
            .insert(document_path, document);
        self.refresh_project(
            &project_root,
            ProjectRefreshMode::Applied {
                full_diagnostic_refresh: false,
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
        let document_path = canonical_fs_path_identity(&crate::fs::FsPath::from_vfs(&path));
        let disk_text = self
            .load_project_sources(&project_root)?
            .into_iter()
            .find_map(|(path, text)| {
                (canonical_fs_path_identity(&path) == document_path).then_some(text)
            });
        project
            .project
            .close_open_document(&document_path, disk_text);
        self.open_documents.lock().unwrap().remove(&document_path);
        self.refresh_project(
            &project_root,
            ProjectRefreshMode::Applied {
                full_diagnostic_refresh: true,
            },
        );
        Ok(())
    }

    fn on_notification_did_save(
        &self,
        params: lsp_notification_params!("textDocument/didSave"),
    ) -> Result<(), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        // Saving does not close the editor overlay. The open document's URI,
        // version and text remain authoritative until didClose.
        let _ = Self::get_baml_project_root(&path)?;
        Ok(())
    }

    fn on_notification_cancel_request(
        &self,
        _params: lsp_notification_params!("$/cancelRequest"),
    ) -> Result<(), LspError> {
        // B2's ingress registry claims the exactly-once response. Accepting
        // the notification here keeps compatibility transports from logging
        // the previous spurious "not supported" error.
        Ok(())
    }
}
