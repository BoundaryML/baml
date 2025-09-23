//! Data model, state management, and configuration resolution.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use anyhow::{anyhow, Context};
use index::DocumentController;
use itertools::any;
use lsp_types::{ClientCapabilities, TextDocumentContentChangeEvent, Url};
use parking_lot::Mutex;
use playground_server::{FrontendMessage, WebviewNotification, WebviewRouterMessage};
use serde_json::Value;

pub(crate) use self::{capabilities::ResolvedClientCapabilities, settings::AllSettings};
pub use self::{
    index::DocumentQuery,
    settings::{BamlSettings, ClientSettings},
};
use crate::{
    baml_project::{file_utils::find_top_level_parent, BamlProject, Project},
    edit::{DocumentKey, DocumentVersion},
    server::client::Notifier,
};
// use crate::system::{url_to_any_system_path, AnySystemPath, LSPSystem};
use crate::{PositionEncoding, TextDocument};

mod capabilities;
pub mod index;
pub mod settings;

use tokio::sync::{broadcast, RwLock};

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

    pub playground_port: u16,
    pub to_webview_router_tx: broadcast::Sender<WebviewRouterMessage>,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            index: self.index.clone(),
            baml_src_projects: self.baml_src_projects.clone(),
            position_encoding: self.position_encoding,
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            baml_settings: self.baml_settings.clone(),
            playground_port: self.playground_port,
            to_webview_router_tx: self.to_webview_router_tx.clone(),
        }
    }
}

impl Session {
    pub fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_settings: ClientSettings,
        workspace_folders: &[(Url, ClientSettings)],
        playground_port: u16,
        to_webview_router_tx: broadcast::Sender<WebviewRouterMessage>,
        client_version: Option<String>,
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

        let baml_settings = BamlSettings::default().with_client_version(client_version);

        Ok(Self {
            position_encoding,
            baml_src_projects: Arc::new(Mutex::new(projects)),
            index: Arc::new(Mutex::new(index)),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
            baml_settings: {
                tracing::info!(
                    "--- Session::new global_settings.baml: {:?}",
                    global_settings.baml
                );
                let baml_settings = global_settings.baml.clone().unwrap_or_default();
                tracing::info!("--- Session::new final baml_settings: {:?}", baml_settings);
                baml_settings
            },
            playground_port,
            to_webview_router_tx,
        })
    }

    pub fn update_baml_settings(&mut self, settings: Value) -> bool {
        tracing::info!("update_baml_settings called with: {:?}", settings);
        match serde_json::from_value::<BamlSettings>(settings) {
            Ok(parsed_settings) => {
                tracing::info!("Successfully parsed BAML settings: {:?}", parsed_settings);
                tracing::info!(
                    "Previous feature_flags: {:?}",
                    self.baml_settings.feature_flags
                );

                // Check if feature flags actually changed
                let feature_flags_changed =
                    self.baml_settings.feature_flags != parsed_settings.feature_flags;

                self.baml_settings = parsed_settings;
                tracing::info!("New feature_flags: {:?}", self.baml_settings.feature_flags);

                if feature_flags_changed {
                    tracing::info!("Feature flags changed, diagnostics should be republished");
                }

                feature_flags_changed
            }
            Err(err) => {
                tracing::error!("Failed to parse BAML settings: {}", err);
                false
            }
        }
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
        let mut projects = self.baml_src_projects.lock();

        // If project exists, return it
        if let Some(project) = projects.get(&baml_src) {
            return Some(project.clone());
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
        projects.insert(baml_src, new_project.clone());
        Some(new_project)
    }

    pub fn print_baml_projects(&self) {
        let projects = self.baml_src_projects.lock();

        let info_string = projects
            .iter()
            .map(|(key, project)| format!("{}: {:?}", key.display(), project.lock().root_path()))
            .collect::<Vec<_>>()
            .join("\n");

        tracing::info!(
            "{} projects_by_workspace_folder: {:?}",
            projects.len(),
            info_string
        );
    }

    pub fn reload(&mut self, notifier: Option<Notifier>) -> anyhow::Result<()> {
        // tracing::info!("skipping session reload");
        // return Ok(());
        tracing::info!("Reloading session");
        let mut baml_src_projects = self.baml_src_projects.lock();

        // Drop moved "baml_src" directories, otherwise the project_updates
        // code below will fail trying to read directories that no longer exist.
        let removed_baml_src_dirs = baml_src_projects
            .keys()
            .filter(|project_root| !project_root.exists())
            .cloned()
            .collect::<Vec<_>>();
        for baml_src_dir in &removed_baml_src_dirs {
            baml_src_projects.remove(baml_src_dir);
        }

        let project_updates: Vec<HashMap<_, _>> = baml_src_projects
            .iter_mut()
            .map(|(_project_root, project)| {
                let files_map = project
                    .lock()
                    .baml_project
                    .load_files()
                    .map_err(|e| anyhow::anyhow!("Failed to load project files: {}", e))?;
                {
                    let default_flags = vec!["beta".to_string()];
                    project.lock().update_runtime(
                        notifier.clone(),
                        self.baml_settings
                            .feature_flags
                            .as_ref()
                            .unwrap_or(&default_flags),
                    )
                }
                .map_err(|e| {
                    tracing::error!("Failed to update runtime after reloading files: {e}");
                    anyhow::anyhow!("Failed to update runtime after reloading files: {e}")
                })?;
                Ok(files_map)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        tracing::info!("Initial reload of {} files", project_updates.len());

        // Guard no longer used. We can drop now instead of waiting for the end
        // of scope.
        drop(baml_src_projects);

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
            let document_is_unsaved = any(self.baml_src_projects.lock().iter(), |(_, project)| {
                project
                    .lock()
                    .baml_project
                    .unsaved_files
                    .contains_key(file_url)
            });
            if !document_is_unsaved {
                self.open_text_document(file_url.clone(), text_document);
            }
        });
        log::info!("Reloaded {} files", files.len());

        Ok(())
    }

    pub fn clear_unsaved_files(&mut self) {
        tracing::info!("Clearing unsaved files");
        for (_folder, project) in self.baml_src_projects.lock().iter_mut() {
            project.lock().baml_project.unsaved_files.clear();
        }
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        let file_path = url.to_file_path().ok()?;
        let project = self.get_or_create_project(&file_path)?;

        let document_key =
            DocumentKey::from_url(&project.lock().baml_project.root_dir_name, &url).ok()?;

        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            document_ref: self.index.lock().make_document_ref(document_key)?,
            position_encoding: self.position_encoding,
            session: Arc::new((*self).clone()),
        })
    }

    /// Registers a text document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&self, document_key: DocumentKey, document: TextDocument) {
        self.index.lock().open_text_document(document_key, document);
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
        for (_folder, project) in self.baml_src_projects.lock().iter_mut() {
            let text_document = TextDocument::new(new_contents.clone(), 0);
            project
                .lock()
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
            let mut index = self.index.lock();
            index.update_text_document(key, content_changes, new_version, position_encoding)?;

            let doc_controller = index
                .documents
                .get(doc_key)
                .expect("We just inserted this, so it should be there");

            let DocumentController::Text(text_document) = doc_controller;

            text_document.contents().to_string()
        };
        let _elapsed = start_time.elapsed();

        let start_time = Instant::now();
        self.baml_src_projects
            .lock()
            .iter_mut()
            .try_for_each(|(_folder, project)| {
                let text_document = TextDocument::new(doc_contents.clone(), 0);
                if project.lock().baml_project.files.contains_key(doc_key) {
                    project
                        .lock()
                        .baml_project
                        .unsaved_files
                        .insert(doc_key.clone(), text_document);
                    let _elapsed = start_time.elapsed();

                    {
                        let default_flags = vec!["beta".to_string()];
                        project.lock().update_runtime(
                            notifier.clone(),
                            self.baml_settings
                                .feature_flags
                                .as_ref()
                                .unwrap_or(&default_flags),
                        )
                    }
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
        let mut index = self.index.lock();
        index.close_document(key)?;
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

    pub(crate) fn session_baml_settings(&self) -> &BamlSettings {
        &self.session.baml_settings
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use lsp_types::ClientCapabilities;

    use super::*; // Import items from outer module (Session, Project, etc.)
    use crate::{
        baml_project::{BamlProject, Project},
        logging::{init_logging, LogLevel},
        session::settings::ClientSettings,
        PositionEncoding,
    };

    // Minimal setup for Session::new
    fn create_test_session() -> Session {
        // Use default/empty capabilities and settings for simplicity
        let client_capabilities = ClientCapabilities::default();
        // Assuming UTF8 is a valid variant or default for PositionEncoding
        let position_encoding = PositionEncoding::UTF8;
        let global_settings = ClientSettings::default();
        let workspace_folders = vec![]; // Start with empty workspace

        let (to_webview_router_tx, _) = broadcast::channel(1);

        Session::new(
            &client_capabilities,
            position_encoding,
            global_settings,
            &workspace_folders,
            0,
            to_webview_router_tx,
            None, // No client_version for this test
        )
        .unwrap()
    }

    #[test]
    fn test_get_or_create_project() {
        init_logging(LogLevel::Info, None);

        let session = create_test_session();

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
            let project_guard = unwrapped_project.lock();
            let found_root = project_guard.root_path();
            assert_eq!(
                found_root, key2,
                "Expected root: {key2:?}, Found root: {found_root:?}"
            );
        }
    }
}
