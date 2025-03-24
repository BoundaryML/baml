//! Data model, state management, and configuration resolution.

use anyhow::Context;
use index::DocumentController;
use itertools::any;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, TextDocumentContentChangeEvent, Url};

// use red_knot_project::{ProjectDatabase, ProjectMetadata};
// use ruff_db::files::{system_path_to_file, File};
// use ruff_db::system::SystemPath;
// use ruff_db::Db;

use crate::baml_db::{File, FileRevision, FileStatus};
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
pub struct Session {
    /// Used to retrieve information about open documents and settings.
    ///
    /// This will be [`None`] when a mutable reference is held to the index via [`index_mut`]
    /// to prevent the index from being accessed while it is being modified. It will be restored
    /// when the mutable reference ([`MutIndexGuard`]) is dropped.
    ///
    /// [`index_mut`]: Session::index_mut
    pub index: Option<Arc<index::Index>>,

    /// Maps workspace folders to their respective project databases.
    pub projects_by_workspace_folder: BTreeMap<PathBuf, Project>,

    /// The global position encoding, negotiated during LSP initialization.
    pub position_encoding: PositionEncoding,
    /// Tracks what LSP features the client supports and doesn't support.
    pub resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
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
                workspace_path,
                Project::new(BamlProject {
                    root_dir_name: url.to_file_path().expect("TODO"),
                    files: HashMap::new(),
                    unsaved_files: HashMap::new(),
                }),
            );
        }

        Ok(Self {
            position_encoding,
            projects_by_workspace_folder: workspaces,
            index: Some(Arc::new(index)),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
        })
    }

    // TODO(dhruvmanila): Ideally, we should have a single method for `workspace_db_for_path_mut`
    // and `default_workspace_db_mut` but the borrow checker doesn't allow that.
    // https://github.com/astral-sh/ruff/pull/13041#discussion_r1726725437

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path, if
    /// any.
    pub(crate) fn project_db_for_path(
        &self,
        path: impl AsRef<Path> + std::fmt::Debug,
    ) -> Option<&Project> {
        let res = self
            .projects_by_workspace_folder
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db);

        // if let Some(p) = res.as_ref() {
        //     eprintln!("project_db_for_path {:?}: {:?}", &path, p.root_path());
        // }
        res
    }

    /// Returns a mutable reference to the project [`ProjectDatabase`] corresponding to the given
    /// path, if any.
    pub(crate) fn project_db_for_path_mut(
        &mut self,
        path: impl AsRef<Path> + std::fmt::Debug,
    ) -> Option<&mut Project> {
        let res = self
            .projects_by_workspace_folder
            .range_mut(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db);

        // if let Some(p) = &res.as_ref() {
        //     eprintln!("project_db_for_path {:?}: {:?}", &path, p.root_path());
        // }
        res
    }

    /// Ensures that a project database exists for the given BAML file,
    /// creating one if it doesn't exist.
    pub fn ensure_project_db_for_baml_file(&mut self, url: &Url) -> anyhow::Result<()> {
        let baml_src = find_top_level_parent(&PathBuf::from(url.to_file_path().map_err(|_| anyhow::anyhow!("Failed to convert URL to path"))?))
            .context("Failed to find top level parent 2")?;
        match self.project_db_for_path(&baml_src) {
            Some(_) => Ok(()),
            None => {
                self.projects_by_workspace_folder.insert(
                    baml_src.clone(),
                    Project::new(BamlProject {
                        root_dir_name: baml_src,
                        files: HashMap::new(),
                        unsaved_files: HashMap::new(),
                    }),
                );
                Ok(())
            }
        }
    }

    pub fn reload(&mut self, notifier: Option<Notifier>) -> anyhow::Result<()> {
        let project_updates: Vec<HashMap<_, _>> = self
            .projects_by_workspace_folder
            .iter_mut()
            .map(|(_projet_root, project)| {
                let files_map = project.baml_project.load_files()?;
                // project.baml_project.unsaved_files.clear();
                project.update_runtime(notifier.clone()).map_err(|e| {
                    anyhow::anyhow!("Failed to update runtime after reloading files: {e}")
                })?;
                // let files_vec = files_map.into_iter().collect::<Vec<_>>();
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
            let document_is_unsaved = any(&self.projects_by_workspace_folder, |(_, project)| {
                project.baml_project.unsaved_files.contains_key(&file_url)
            });
            if !document_is_unsaved {
                self.open_text_document(file_url.clone(), text_document);
            }
        });

        Ok(())
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        // let key = self.key_from_url(url);
        let project = self.project_db_for_path(url.to_file_path().ok()?)?;
        let document_key = DocumentKey::from_url(&PathBuf::from(project.root_path()), &url).ok()?;
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            document_ref: self.index().make_document_ref(document_key)?,
            position_encoding: self.position_encoding,
        })
    }

    /// Registers a text document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, document_key: DocumentKey, document: TextDocument) {
        self.index_mut()
            .open_text_document(document_key.clone(), document.clone());
        // self.projects_by_workspace_folder
        //     .iter_mut()
        //     .for_each(|(folder, project)| {
        //         dbg!(&folder);
        //         if url
        //             .path()
        //             .starts_with(folder.as_os_str().to_str().expect("TODO: handle error"))
        //         {
        //             eprintln!("MATCH");
        //         }
        //         // project.baml_project.files.insert(url.as_str().to_string(), document.contents().to_string());
        //         // project.baml_project.load_files();
        //         project.reload().expect("TODO: Handle reload errer");
        //     })?;
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
        for (_folder, project) in self.projects_by_workspace_folder.iter_mut() {
            let text_document = TextDocument::new(new_contents.clone(), 0);
            project
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
        &mut self,
        key: &DocumentKey,
        content_changes: Vec<TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        notifier: Option<Notifier>,
    ) -> anyhow::Result<()> {
        let position_encoding = self.position_encoding;

        // let doc_key = match key {
        //     DocumentKey::Text(url) => url,
        // };
        let doc_key = key;
        let doc_contents = {
            let mut index = self.index_mut();
            index.update_text_document(key, content_changes, new_version, position_encoding)?;

            let doc_controller = {
                index
                    .documents
                    .get(doc_key)
                    .expect("We just inserted this, so it should be there")
            };
            let text_document = match doc_controller {
                DocumentController::Text(text_document) => text_document,
            };
            text_document.contents().to_string()
        };

        self.projects_by_workspace_folder
            .iter_mut()
            .try_for_each(|(_folder, project)| {
                let text_document = TextDocument::new(doc_contents.clone(), 0);
                if project.baml_project.files.get(&doc_key).is_some() {
                    project
                        .baml_project
                        .unsaved_files
                        .insert(doc_key.clone(), text_document);

                    project
                        .update_runtime(notifier.clone())
                        .map_err(|e| anyhow::anyhow!("Could not update runtime: {e}"))?;
                }
                Ok::<(), anyhow::Error>(())
            })?;
        Ok(())
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&mut self, key: &DocumentKey) -> anyhow::Result<()> {
        self.index_mut().close_document(key)?;
        Ok(())
    }

    /// Returns a reference to the index.
    ///
    /// # Panics
    ///
    /// Panics if there's a mutable reference to the index via [`index_mut`].
    ///
    /// [`index_mut`]: Session::index_mut
    pub fn index(&self) -> &index::Index {
        self.index.as_ref().unwrap()
    }

    /// Returns a mutable reference to the index.
    ///
    /// This method drops all references to the index and returns a guard that will restore the
    /// references when dropped. This guard holds the only reference to the index and allows
    /// modifying it.
    fn index_mut(&mut self) -> MutIndexGuard {
        let index = self.index.take().unwrap();

        // for db in self.projects_by_workspace_folder.values_mut() {
        //     // Remove the `index` from each database. This drops the count of `Arc<Index>` down to 1
        //     // db.system_mut()
        //     //     .as_any_mut()
        //     //     .downcast_mut::<LSPSystem>()
        //     //     .unwrap()
        //     //     .take_index();
        // }

        // There should now be exactly one reference to index which is self.index.
        let index = Arc::into_inner(index);

        MutIndexGuard {
            session: self,
            index,
        }
    }
}

/// A guard that holds the only reference to the index and allows modifying it.
///
/// When dropped, this guard restores all references to the index.
struct MutIndexGuard<'a> {
    session: &'a mut Session,
    index: Option<index::Index>,
}

impl Deref for MutIndexGuard<'_> {
    type Target = index::Index;

    fn deref(&self) -> &Self::Target {
        self.index.as_ref().unwrap()
    }
}

impl DerefMut for MutIndexGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.index.as_mut().unwrap()
    }
}

// TODO: Fix this?
impl Drop for MutIndexGuard<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            let index = Arc::new(index);
            // for db in self.session.projects_by_workspace_folder.values_mut() {
            //     // db.system_mut()
            //     //     .as_any_mut()
            //     //     .downcast_mut::<LSPSystem>()
            //     //     .unwrap()
            //     //     .set_index(index.clone());
            // }

            self.session.index = Some(index);
        }
    }
}

/// An immutable snapshot of `Session` that references
/// a specific document.
#[derive(Debug)]
pub struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
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

    ///
    pub(crate) fn file(&self, db: &Project) -> Option<File> {
        let url = self.document_ref.file_url();
        let document_key = self.document_ref.file_document_key();
        let path_str = url.as_str().to_string();
        let file_is_in_db = db.baml_project.files.contains_key(&document_key);
        if file_is_in_db {
            Some(File {
                path: path_str,
                permissions: None,
                revision: FileRevision::now(),
                status: FileStatus::Exists,
            })
        } else {
            None
        }
    }
}
