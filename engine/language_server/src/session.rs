//! Data model, state management, and configuration resolution.

use anyhow::Context;
use index::DocumentController;
use itertools::any;
use std::collections::{BTreeMap, HashMap};
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
pub use self::index::DocumentQuery;
pub(crate) use self::settings::AllSettings;
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

    /// Maps workspace folders to their respective project databases.
    pub projects_by_workspace_folder: Arc<Mutex<BTreeMap<PathBuf, Arc<Mutex<Project>>>>>,

    /// The global position encoding, negotiated during LSP initialization.
    pub position_encoding: PositionEncoding,
    /// Tracks what LSP features the client supports and doesn't support.
    pub resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        Self {
            index: self.index.clone(),
            projects_by_workspace_folder: self.projects_by_workspace_folder.clone(),
            position_encoding: self.position_encoding.clone(),
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
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
        let mut workspaces = BTreeMap::new();
        let index = index::Index::new(global_settings);

        for (url, _) in workspace_folders {
            let workspace_path = url
                .to_file_path()
                .map_err(|()| anyhow!("Workspace URL is not a file or directory: {:?}", url))?;

            workspaces.insert(
                workspace_path.clone(),
                Arc::new(Mutex::new(Project::new(BamlProject {
                    root_dir_name: workspace_path,
                    files: HashMap::new(),
                    unsaved_files: HashMap::new(),
                    cached_runtime: None,
                }))),
            );
        }

        Ok(Self {
            position_encoding,
            projects_by_workspace_folder: Arc::new(Mutex::new(workspaces)),
            index: Arc::new(Mutex::new(index)),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
        })
    }

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path, if
    /// any.
    pub(crate) fn project_db_for_path(
        &self,
        path: impl AsRef<Path> + std::fmt::Debug,
    ) -> Option<Arc<Mutex<Project>>> {
        let guard = self.projects_by_workspace_folder.lock().unwrap();
        guard
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db.clone())
    }

    /// Returns a mutable reference to the project [`ProjectDatabase`] corresponding to the given
    /// path, if any.
    pub(crate) fn project_db_for_path_mut(
        &mut self,
        path: impl AsRef<Path> + std::fmt::Debug,
    ) -> Option<Arc<Mutex<Project>>> {
        let guard = self.projects_by_workspace_folder.lock().unwrap();
        guard
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db.clone())
    }

    /// Ensures that a project database exists for the given BAML file,
    /// creating one if it doesn't exist.
    pub fn ensure_project_db_for_baml_file(&mut self, url: &Url) -> anyhow::Result<()> {
        let baml_src = find_top_level_parent(&PathBuf::from(
            url.to_file_path()
                .map_err(|_| anyhow::anyhow!("Failed to convert URL to path"))?,
        ))
        .context("Failed to find top level parent 2")?;
        match self.project_db_for_path(&baml_src) {
            Some(_) => Ok(()),
            None => {
                self.projects_by_workspace_folder.lock().unwrap().insert(
                    baml_src.clone(),
                    Arc::new(Mutex::new(Project::new(BamlProject {
                        root_dir_name: baml_src,
                        files: HashMap::new(),
                        unsaved_files: HashMap::new(),
                        cached_runtime: None,
                    }))),
                );
                Ok(())
            }
        }
    }

    pub fn reload(&mut self, notifier: Option<Notifier>) -> anyhow::Result<()> {
        tracing::info!("---- Reloading session");
        let project_updates: Vec<HashMap<_, _>> = self
            .projects_by_workspace_folder
            .lock()
            .unwrap()
            .iter_mut()
            .map(|(_projet_root, project)| {
                let files_map = project.lock().unwrap().baml_project.load_files()?;
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
        let files: Vec<(DocumentKey, String)> = project_updates
            .into_iter()
            .map(|project_files| {
                project_files
                    .into_iter()
                    .map(|(key, text_document)| (key, text_document.contents))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect();

        // Index all the files, except for the ones with unsaved changes.
        files.iter().for_each(|(file_url, file_contents)| {
            let text_document = TextDocument::new(file_contents.clone(), 0);
            let document_is_unsaved = any(
                self.projects_by_workspace_folder.lock().unwrap().iter(),
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
        log::info!("--- Reloaded {} files", files.len());

        Ok(())
    }
    pub fn clear_unsaved_files(&mut self) {
        tracing::info!("Clearing unsaved files");
        for (_folder, project) in self.projects_by_workspace_folder.lock().unwrap().iter_mut() {
            project.lock().unwrap().baml_project.unsaved_files.clear();
        }
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        // let key = self.key_from_url(url);
        let project = self.project_db_for_path(url.to_file_path().ok()?)?;
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
        for (_folder, project) in self.projects_by_workspace_folder.lock().unwrap().iter_mut() {
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
            index.update_text_document(key, content_changes, new_version, position_encoding)?;

            let doc_controller = index
                .documents
                .get(doc_key)
                .expect("We just inserted this, so it should be there");

            let text_document = match doc_controller {
                DocumentController::Text(text_document) => text_document,
            };
            text_document.contents().to_string()
        };
        let elapsed = start_time.elapsed();

        let start_time = Instant::now();
        self.projects_by_workspace_folder
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
                    let elapsed = start_time.elapsed();

                    project
                        .lock()
                        .unwrap()
                        .update_runtime(notifier.clone())
                        .map_err(|e| anyhow::anyhow!("Could not update runtime: {e}"))?;
                    let elapsed = start_time.elapsed();
                }
                Ok::<(), anyhow::Error>(())
            })?;
        Ok(())
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&self, key: &DocumentKey) -> anyhow::Result<()> {
        let mut index = self.index.lock().unwrap();
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
        self.session
            .project_db_for_path(self.document_ref.file_url().to_file_path().unwrap())
    }
}
