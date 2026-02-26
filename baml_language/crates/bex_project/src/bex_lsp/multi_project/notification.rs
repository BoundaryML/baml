use super::{BexMulitProject, LspError, ProjectRefreshMode};
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
        let mut projects = self.projects.lock().unwrap();
        projects.clear();
        Ok(())
    }

    fn on_notification_initialized(
        &self,
        _params: lsp_notification_params!("initialized"),
    ) -> Result<(), LspError> {
        let workspace_roots = self.workspace_roots.lock().unwrap().clone();

        if workspace_roots.is_empty() {
            tracing::warn!(
                "No workspace roots provided during initialize — skipping project discovery"
            );
            return Ok(());
        }

        let mut project_roots = Vec::new();
        for root in &workspace_roots {
            let Ok(dirs) = root.walk_dir() else {
                tracing::warn!("Failed to walk workspace root: {}", root.as_str());
                continue;
            };
            for entry in dirs.filter_map(Result::ok) {
                if let Ok(pr) = Self::get_baml_project_root(&entry) {
                    project_roots.push(pr);
                }
            }
        }

        project_roots.sort_by_key(|path| path.as_str().to_string());
        project_roots.dedup_by(|a, b| a.as_str() == b.as_str());

        tracing::info!("Discovered {} BAML project(s)", project_roots.len());

        for project_root in project_roots {
            let Ok(_) = self.get_or_create_project(project_root.clone()) else {
                continue;
            };
            self.refresh_project(&project_root, ProjectRefreshMode::Full);
        }

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
            params.text_document.text,
        );
        drop(in_memory_changes);

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
                return Err(LspError::RequestNotSupported(
                    "Expected a single full-document change event (TextDocumentSyncKind::FULL)"
                        .to_string(),
                ));
            }
        };

        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project = self.get_or_create_project(project_root.clone())?;

        let mut in_memory_changes = project.in_memory_changes.lock().unwrap();
        in_memory_changes.insert(crate::fs::FsPath::from_vfs(&path), new_text);
        drop(in_memory_changes);

        self.refresh_project(&project_root, ProjectRefreshMode::InMemoryChangesOnly);
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

        self.refresh_project(&project_root, ProjectRefreshMode::Only(vec![path]));
        Ok(())
    }

    fn on_notification_did_save(
        &self,
        params: lsp_notification_params!("textDocument/didSave"),
    ) -> Result<(), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let project_root = Self::get_baml_project_root(&path)?;
        let project = self.get_or_create_project(project_root)?;

        let mut in_memory_changes = project.in_memory_changes.lock().unwrap();
        in_memory_changes.remove(&crate::fs::FsPath::from_vfs(&path));
        drop(in_memory_changes);

        // We don't need to refresh the project here, because the in-memory
        // and disk versions of the file are already in sync
        Ok(())
    }
}
