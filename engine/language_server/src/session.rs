//! Data model, state management, and configuration resolution.

use anyhow::Context;
use index::DocumentController;
use itertools::any;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, TextDocumentContentChangeEvent, Url};

use crate::baml_project::file_utils::find_top_level_parent;
use crate::baml_project::{BamlProject, Project};
use crate::edit::{DocumentKey, DocumentVersion};
// use crate::system::{url_to_any_system_path, AnySystemPath, LSPSystem};
use crate::{PositionEncoding, TextDocument};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub use self::index::{DocumentError, DocumentQuery};
pub(crate) use self::settings::AllSettings;
pub use self::settings::BamlSettings;
pub use self::settings::ClientSettings;
use crate::server::client::Notifier;

mod capabilities;
pub mod index;
mod settings;

// TODO(dhruvmanila): In general, the server shouldn't use any salsa queries directly and instead
// should use methods on `ProjectDatabase`.

/// The global state for the LSP
#[derive(Debug)]
pub struct Session {
    /// Used to retrieve information about open documents and settings.
    pub index: Arc<Mutex<index::Index>>,

    /// Maps baml_src directories to their respective project databases.
    pub baml_src_projects: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<Project>>>>>,

    /// The global position encoding, negotiated during LSP initialization.
    pub position_encoding: PositionEncoding,
    /// Tracks what LSP features the client supports and doesn't support.
    pub resolved_client_capabilities: Arc<ResolvedClientCapabilities>,

    pub baml_settings: BamlSettings,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            index: self.index.clone(),
            baml_src_projects: self.baml_src_projects.clone(),
            position_encoding: self.position_encoding.clone(),
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            baml_settings: self.baml_settings.clone(),
        }
    }
}

impl Session {
    pub fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_settings: ClientSettings,
        workspace_folders: &[(Url, ClientSettings)],
    ) -> anyhow::Result<Self> {
        let mut projects = HashMap::new();
        let index = index::Index::new(global_settings.clone());

        for (url, _) in workspace_folders {
            let workspace_path = url
                .to_file_path()
                .map_err(|()| anyhow!("Workspace URL is not a file or directory: {:?}", url))?;

            // Try to find the baml_src directory
            if let Some(baml_src) = find_top_level_parent(&workspace_path) {
                projects.insert(
                    baml_src.clone(),
                    Arc::new(Mutex::new(Project::new(BamlProject {
                        root_dir_name: baml_src.clone(),
                        files: HashMap::new(),
                        unsaved_files: HashMap::new(),
                        cached_runtime: None,
                    }))),
                );
                tracing::info!(
                    "Session::new: Added initial project for baml_src path: {:?}",
                    baml_src
                );
            } else {
                tracing::info!("Session::new: No baml_src found yet {:?}", workspace_path);
            }
        }

        Ok(Self {
            position_encoding,
            baml_src_projects: Arc::new(Mutex::new(projects)),
            index: Arc::new(Mutex::new(index)),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
            baml_settings: BamlSettings::default(),
        })
    }

    pub fn update_baml_settings(&mut self, settings: Value) {
        match serde_json::from_value(settings) {
            Ok(parsed_settings) => {
                self.baml_settings = parsed_settings;
            }
            Err(err) => {
                tracing::error!("Failed to parse BAML settings: {}", err);
            }
        }
    }

    /// Handles the case where a baml_src directory has been moved by updating
    /// the project mapping and remapping document controllers.
    fn handle_directory_move(&self, old_path: &Path, new_path: &Path) -> anyhow::Result<()> {
        tracing::info!(
            "Handling directory move from {:?} to {:?}",
            old_path,
            new_path
        );

        let mut projects = self.baml_src_projects.lock().unwrap();
        let mut index = self.index.lock().unwrap();

        // Find and remove the old project
        if let Some(project) = projects.remove(old_path) {
            // Update the project's root path
            project.lock().unwrap().baml_project.root_dir_name = new_path.to_path_buf();

            // Collect all document keys that need to be remapped
            let old_document_keys: Vec<DocumentKey> = index.documents.keys().cloned().collect();
            let mut documents_to_remap = Vec::new();

            for old_key in old_document_keys {
                // Check if this document key belongs to the moved project
                if old_key.path().starts_with(old_path) {
                    if let Some(controller) = index.documents.remove(&old_key) {
                        // Create new document key with updated root path
                        let relative_path = old_key
                            .path()
                            .strip_prefix(old_path)
                            .unwrap_or(old_key.path());
                        let new_absolute_path = new_path.join(relative_path);
                        let new_key = DocumentKey::from_path(new_path, &new_absolute_path)?;
                        documents_to_remap.push((new_key, controller));
                    }
                }
            }

            // Re-insert the documents with new keys
            for (new_key, controller) in documents_to_remap {
                index.documents.insert(new_key, controller);
            }

            // Insert the project with the new path
            projects.insert(new_path.to_path_buf(), project);

            tracing::info!(
                "Successfully moved project from {:?} to {:?}",
                old_path,
                new_path
            );
        }

        Ok(())
    }

    /// Detects if any projects have been moved and handles the moves.
    /// This is called when we receive file system notifications.
    pub fn handle_potential_directory_moves(&self) -> anyhow::Result<()> {
        let projects = self.baml_src_projects.lock().unwrap();
        let mut moves_to_handle = Vec::new();

        // Check for projects that no longer exist at their recorded paths
        for (recorded_path, project) in projects.iter() {
            if !recorded_path.exists() {
                // Try to find where this project might have moved
                let project_guard = project.lock().unwrap();
                let expected_name = recorded_path.file_name();
                drop(project_guard);

                if let Some(name) = expected_name {
                    // Search for directories with the same name that might be the moved project
                    // This is a simple heuristic - in practice, you might want to be more sophisticated
                    if let Some(parent) = recorded_path.parent() {
                        if let Ok(entries) = std::fs::read_dir(parent) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.is_dir()
                                    && path.file_name() == Some(name)
                                    && path != *recorded_path
                                {
                                    // Found a potential match
                                    moves_to_handle.push((recorded_path.clone(), path));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Drop the lock before handling moves to avoid deadlock
        drop(projects);

        // Handle each detected move
        for (old_path, new_path) in moves_to_handle {
            self.handle_directory_move(&old_path, &new_path)?;
        }

        Ok(())
    }

    /// Attempts to recover from a document controller not found error by checking
    /// if a project move might have occurred for the specific file being accessed.
    pub fn try_recover_from_missing_document(&self, url: &lsp_types::Url) -> anyhow::Result<bool> {
        let file_path = url
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Could not convert URL to file path: {}", url))?;

        // Try to find the baml_src directory for this file
        if let Some(baml_src) = find_top_level_parent(&file_path) {
            let projects = self.baml_src_projects.lock().unwrap();

            // Check if we have a project for this exact path
            if projects.contains_key(&baml_src) {
                return Ok(false); // Project exists, no recovery needed
            }

            // Look for a project that might have been moved here
            let baml_src_name = baml_src.file_name();
            let mut old_path_to_move = None;

            for (existing_path, _project) in projects.iter() {
                if existing_path.file_name() == baml_src_name && !existing_path.exists() {
                    // Found a project that seems to have been moved
                    old_path_to_move = Some(existing_path.clone());
                    break;
                }
            }

            drop(projects); // Release lock before calling handle_directory_move

            if let Some(old_path) = old_path_to_move {
                tracing::info!(
                    "Attempting recovery: moving project from {:?} to {:?}",
                    old_path,
                    baml_src
                );

                self.handle_directory_move(&old_path, &baml_src)?;
                return Ok(true); // Recovery attempted
            }
        }

        Ok(false) // No recovery possible
    }

    /// Gets or creates a project for the given path.
    ///
    /// This is the primary method for working with projects, replacing the multiple
    /// previous methods. It handles both lookup and creation in a single method.
    ///
    /// Returns:
    /// - Some(Arc<Mutex<Project>>) if a project was found or created
    /// - None if no baml_src directory could be found for the path
    pub fn get_or_create_project(
        &self,
        path: impl AsRef<Path> + std::fmt::Debug,
    ) -> Option<Arc<Mutex<Project>>> {
        // Try to find the baml_src directory
        let baml_src = find_top_level_parent(path.as_ref())?;

        // Lock once and perform all operations within this scope
        let projects = self.baml_src_projects.lock().unwrap();

        // If project exists, return it
        if let Some(project) = projects.get(&baml_src) {
            return Some(project.clone());
        }

        // Check if there's a project with a different path but same directory name
        // This can happen when directories are moved
        let baml_src_name = baml_src.file_name()?;
        let mut old_path_to_move = None;

        for (existing_path, _existing_project) in projects.iter() {
            if existing_path.file_name() == Some(baml_src_name)
                && !existing_path.exists()
                && baml_src.exists()
            {
                // Found a project that seems to have been moved
                old_path_to_move = Some(existing_path.clone());
                break;
            }
        }

        if let Some(old_path) = old_path_to_move {
            // Drop the projects lock before calling handle_directory_move to avoid deadlock
            drop(projects);

            if let Err(e) = self.handle_directory_move(&old_path, &baml_src) {
                tracing::error!("Failed to handle directory move: {}", e);
            } else {
                // Re-acquire the lock and return the moved project
                let projects = self.baml_src_projects.lock().unwrap();
                return projects.get(&baml_src).cloned();
            }
        } else {
            // No move needed, release the lock for project creation
            drop(projects);
        }

        // Create a new project if needed
        tracing::info!("Creating new project for baml_src path: {:?}", baml_src);
        let new_project = Arc::new(Mutex::new(Project::new(BamlProject {
            root_dir_name: baml_src.clone(),
            files: HashMap::new(),
            unsaved_files: HashMap::new(),
            cached_runtime: None,
        })));

        // Insert and return the new project
        let mut projects = self.baml_src_projects.lock().unwrap();
        projects.insert(baml_src, new_project.clone());
        Some(new_project)
    }

    pub fn print_baml_projects(&self) {
        let projects = self.baml_src_projects.lock().unwrap();

        let info_string = projects
            .iter()
            .map(|(key, project)| {
                format!(
                    "{}: {:?}",
                    key.display(),
                    project.lock().unwrap().root_path()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        tracing::info!(
            "{} projects_by_workspace_folder: {:?}",
            projects.len(),
            info_string
        );
    }

    pub fn reload(&mut self, notifier: Option<Notifier>) -> anyhow::Result<()> {
        tracing::info!("Reloading session");
        let project_updates: Vec<HashMap<_, _>> = self
            .baml_src_projects
            .lock()
            .unwrap()
            .iter_mut()
            .map(|(_project_root, project)| {
                let files_map = project
                    .lock()
                    .unwrap()
                    .baml_project
                    .load_files()
                    .map_err(|e| anyhow::anyhow!("Failed to load project files: {}", e))?;
                project
                    .lock()
                    .unwrap()
                    .update_runtime(notifier.clone())
                    .map_err(|e| {
                        tracing::error!("Failed to update runtime after reloading files: {e}");
                        anyhow::anyhow!("Failed to update runtime after reloading files: {e}")
                    })?;
                Ok(files_map)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        tracing::info!("Initial reload of {} files", project_updates.len());

        let files: Vec<(DocumentKey, String)> = project_updates
            .into_iter()
            .flat_map(|project_files| {
                project_files
                    .into_iter()
                    .map(|(key, text_document)| (key, text_document.contents))
                    .collect::<Vec<_>>()
            })
            .collect();

        // Index all the files, except for the ones with unsaved changes.
        files.iter().for_each(|(file_url, file_contents)| {
            let text_document = TextDocument::new(file_contents.clone(), 0);
            let document_is_unsaved = any(
                self.baml_src_projects.lock().unwrap().iter(),
                |(_, project)| {
                    project
                        .lock()
                        .unwrap()
                        .baml_project
                        .unsaved_files
                        .contains_key(&file_url)
                },
            );
            if !document_is_unsaved {
                self.open_text_document(file_url.clone(), text_document);
            }
        });
        log::info!("Reloaded {} files", files.len());

        Ok(())
    }

    pub fn clear_unsaved_files(&mut self) {
        tracing::info!("Clearing unsaved files");
        for (_folder, project) in self.baml_src_projects.lock().unwrap().iter_mut() {
            project.lock().unwrap().baml_project.unsaved_files.clear();
        }
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        let file_path = url.to_file_path().ok()?;
        let project = self.get_or_create_project(&file_path)?;

        let document_key = DocumentKey::from_url(
            &PathBuf::from(project.lock().unwrap().baml_project.root_dir_name.clone()),
            &url,
        )
        .ok()?;

        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            document_ref: self.index.lock().unwrap().make_document_ref(document_key)?,
            position_encoding: self.position_encoding,
            session: Arc::new((*self).clone()),
        })
    }

    /// Registers a text document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&self, document_key: DocumentKey, document: TextDocument) {
        let mut index = self.index.lock().unwrap();
        index.open_text_document(document_key, document);
    }

    pub(crate) fn set_unsaved_file(
        &mut self,
        document_key: &DocumentKey,
        content_changes: Vec<TextDocumentContentChangeEvent>,
    ) -> anyhow::Result<()> {
        let new_contents: String = match content_changes.as_slice() {
            [event] if event.range.is_none() => event.text.clone(),
            _ => {
                anyhow::bail!(
                    "Only one change event, with full text, is supported for unsaved files"
                )
            }
        };
        for (_folder, project) in self.baml_src_projects.lock().unwrap().iter_mut() {
            let text_document = TextDocument::new(new_contents.clone(), 0);
            project
                .lock()
                .unwrap()
                .baml_project
                .unsaved_files
                .insert(document_key.clone(), text_document);
        }
        Ok(())
    }

    /// Updates a text document at the associated `key`.
    ///
    /// The document key must point to a text document, or this will throw an error.
    pub(crate) fn update_text_document(
        &self,
        key: &DocumentKey,
        content_changes: Vec<TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        notifier: Option<Notifier>,
    ) -> anyhow::Result<()> {
        let position_encoding = self.position_encoding;
        let doc_key = key;
        let start_time = Instant::now();
        let doc_contents = {
            let mut index = self.index.lock().unwrap();

            // First attempt to update the document
            let update_result = index.update_text_document(
                key,
                content_changes.clone(),
                new_version,
                position_encoding,
            );

            // If the update failed because document controller wasn't found, try to recover
            match update_result {
                Err(ref doc_error) if doc_error.is_controller_not_available() => {
                    // Try to recover from the missing document
                    let url = key.url();
                    drop(index); // Release the lock before attempting recovery

                    tracing::info!(
                        "Attempting to recover from missing document controller for: {}",
                        url
                    );

                    if let Ok(true) = self.try_recover_from_missing_document(&url) {
                        // Recovery was attempted, try the update again
                        let mut index = self.index.lock().unwrap();
                        index
                            .update_text_document(
                                key,
                                content_changes,
                                new_version,
                                position_encoding,
                            )
                            .map_err(|e| anyhow::anyhow!(e))?;
                    } else {
                        // Recovery failed or wasn't possible, return the original error
                        return Err(anyhow::anyhow!(update_result.unwrap_err()));
                    }
                }
                Err(e) => {
                    // Different error, return it as-is
                    return Err(anyhow::anyhow!(e));
                }
                Ok(()) => {
                    // Update succeeded on first try
                }
            }

            // Re-acquire index lock to get document contents
            let index = self.index.lock().unwrap();

            let doc_controller = index
                .documents
                .get(doc_key)
                .expect("We just inserted this, so it should be there");

            let text_document = match doc_controller {
                DocumentController::Text(text_document) => text_document,
            };
            text_document.contents().to_string()
        };
        let _elapsed = start_time.elapsed();

        let start_time = Instant::now();
        self.baml_src_projects
            .lock()
            .unwrap()
            .iter_mut()
            .try_for_each(|(_folder, project)| {
                let text_document = TextDocument::new(doc_contents.clone(), 0);
                if project
                    .lock()
                    .unwrap()
                    .baml_project
                    .files
                    .get(&doc_key)
                    .is_some()
                {
                    project
                        .lock()
                        .unwrap()
                        .baml_project
                        .unsaved_files
                        .insert(doc_key.clone(), text_document);
                    let _elapsed = start_time.elapsed();

                    project
                        .lock()
                        .unwrap()
                        .update_runtime(notifier.clone())
                        .map_err(|e| anyhow::anyhow!("Could not update runtime: {e}"))?;
                    let _elapsed = start_time.elapsed();
                }
                Ok::<(), anyhow::Error>(())
            })?;
        Ok(())
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&self, key: &DocumentKey) -> anyhow::Result<()> {
        let mut index = self.index.lock().unwrap();
        index.close_document(key).map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    /// Returns a reference to the index.
    pub fn index(&self) -> &Arc<Mutex<index::Index>> {
        &self.index
    }
}

/// An immutable snapshot of `Session` that references
/// a specific document.
#[derive(Debug)]
pub struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
    session: Arc<Session>,
}

impl DocumentSnapshot {
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub fn query(&self) -> &index::DocumentQuery {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub(crate) fn project(&self) -> Option<Arc<Mutex<Project>>> {
        let file_path = self.document_ref.file_url().to_file_path().ok()?;
        self.session.get_or_create_project(&file_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Import items from outer module (Session, Project, etc.)
    use crate::baml_project::{BamlProject, Project};
    use crate::logging::{init_logging, LogLevel};
    use crate::session::settings::ClientSettings;
    use crate::PositionEncoding;
    use lsp_types::ClientCapabilities;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    // Minimal setup for Session::new
    fn create_test_session() -> Session {
        // Use default/empty capabilities and settings for simplicity
        let client_capabilities = ClientCapabilities::default();
        // Assuming UTF8 is a valid variant or default for PositionEncoding
        let position_encoding = PositionEncoding::UTF8;
        let global_settings = ClientSettings::default();
        let workspace_folders = vec![]; // Start with empty workspace

        Session::new(
            &client_capabilities,
            position_encoding,
            global_settings,
            &workspace_folders,
        )
        .unwrap()
    }

    #[test]
    fn test_get_or_create_project() {
        init_logging(LogLevel::Info, None);

        let mut session = create_test_session();

        // Using paths similar to the logs
        let path_str1 = "/Users/aaronvillalpando/Projects/baml-examples/ruby-starter/baml_src";
        let path_str2 = "/Users/aaronvillalpando/Projects/next-app/my-app/baml_src";

        let key1 = PathBuf::from(path_str1);
        let key2 = PathBuf::from(path_str2);

        // Create a project for key1
        let project1 = session.get_or_create_project(&key1);
        assert!(project1.is_some(), "Project should be created for key1");

        // Verify that get_or_create_project returns the same project when called again
        let project1_again = session.get_or_create_project(&key1);
        assert!(project1_again.is_some(), "Project should be found for key1");

        // Create a project for key2
        let project2 = session.get_or_create_project(&key2);
        assert!(project2.is_some(), "Project should be created for key2");

        // Test with a file path inside key2
        let file_path_in_key2 = key2.join("chat.baml");
        let found_project = session.get_or_create_project(&file_path_in_key2);
        assert!(
            found_project.is_some(),
            "Project should be found for file path within key2"
        );

        // Verify it's the same project
        {
            let unwrapped_project = found_project.unwrap();
            let project_guard = unwrapped_project.lock().unwrap();
            let found_root = project_guard.root_path();
            assert_eq!(
                found_root, key2,
                "Expected root: {:?}, Found root: {:?}",
                key2, found_root
            );
        }
    }
}
