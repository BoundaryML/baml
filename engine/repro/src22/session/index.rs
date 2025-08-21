use std::{path::PathBuf, sync::Arc};

use lsp_types::Url;
use rustc_hash::FxHashMap;

use super::ClientSettings;
use crate::{
    edit::{DocumentKey, DocumentVersion},
    PositionEncoding, TextDocument,
};

/// Stores and tracks all open documents in a session, along with their associated settings.
#[derive(Default, Debug)]
pub struct Index {
    /// Maps all document file URLs to the associated document controller
    pub documents: FxHashMap<DocumentKey, DocumentController>,

    /// Global settings provided by the client.
    pub global_settings: ClientSettings,
}

impl Index {
    pub fn new(global_settings: ClientSettings) -> Self {
        Self {
            documents: FxHashMap::default(),
            global_settings,
        }
    }

    pub fn text_document_urls(&self) -> impl Iterator<Item = &DocumentKey> + '_ {
        self.documents
            .iter()
            .filter_map(|(url, doc)| doc.as_text().and(Some(url)))
    }

    pub fn update_text_document(
        &mut self,
        key: &DocumentKey,
        content_changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> anyhow::Result<()> {
        let controller = self.document_controller_for_key(key)?;
        let Some(document) = controller.as_text_mut() else {
            anyhow::bail!("Text document URI does not point to a text document");
        };

        if content_changes.is_empty() {
            document.update_version(new_version);
            return Ok(());
        }

        document.apply_changes(content_changes, new_version, encoding);

        Ok(())
    }

    // pub fn key_from_url(&self, url: Url) -> DocumentKey {
    //     DocumentKey::Text(url)
    // }

    pub fn num_documents(&self) -> usize {
        self.documents.len()
    }

    pub fn make_document_ref(&self, document_key: DocumentKey) -> Option<DocumentQuery> {
        // let url = self.url_for_key(&key)?.clone();
        let controller = self.documents.get(&document_key)?;
        Some(controller.make_ref(document_key))
    }

    pub fn open_text_document(&mut self, document_key: DocumentKey, document: TextDocument) {
        self.documents
            .insert(document_key, DocumentController::new_text(document));
    }

    pub fn close_document(&mut self, document_key: &DocumentKey) -> anyhow::Result<()> {
        // let Some(url) = self.url_for_key(key).cloned() else {
        //     anyhow::bail!("Tried to close unavailable document `{key}`");
        // };

        let Some(_) = self.documents.remove(document_key) else {
            anyhow::bail!(
                "tried to close document that didn't exist at {}",
                document_key
            )
        };
        Ok(())
    }

    pub fn document_controller_for_key(
        &mut self,
        document_key: &DocumentKey,
    ) -> anyhow::Result<&mut DocumentController> {
        let Some(controller) = self.documents.get_mut(document_key) else {
            anyhow::bail!("Document controller not available at `{}`", document_key);
        };
        Ok(controller)
    }

    // fn url_for_key<'a>(&'a self, key: &'a DocumentKey) -> Option<&'a Url> {
    //     match key {
    //         DocumentKey::Text(path) => Some(path),
    //     }
    // }
}

/// A mutable handler to an underlying document.
/// TODO: Don't use an enum here.
#[derive(Debug)]
pub enum DocumentController {
    Text(Arc<TextDocument>),
}

impl DocumentController {
    fn new_text(document: TextDocument) -> Self {
        Self::Text(Arc::new(document))
    }

    fn make_ref(&self, document_key: DocumentKey) -> DocumentQuery {
        match &self {
            Self::Text(document) => DocumentQuery::Text {
                document_key,
                document: document.clone(),
            },
        }
    }

    #[allow(dead_code)]
    pub fn as_text(&self) -> Option<&TextDocument> {
        match self {
            Self::Text(document) => Some(document),
            // Self::Notebook(_) => None,
        }
    }

    pub fn as_text_mut(&mut self) -> Option<&mut TextDocument> {
        Some(match self {
            Self::Text(document) => Arc::make_mut(document),
            // Self::Notebook(_) => return None,
        })
    }
}

/// A read-only query to an open document.
/// This query can 'select' a text document.
/// It also includes document settings.
#[derive(Debug, Clone)]
pub enum DocumentQuery {
    Text {
        document_key: DocumentKey,
        document: Arc<TextDocument>,
    },
}

impl DocumentQuery {
    // /// Retrieve the original key that describes this document query.
    // pub(crate) fn make_key(&self) -> DocumentKey {
    //     match self {
    //         Self::Text { file_url, .. } => DocumentKey::Text(file_url.clone()),
    //     }
    // }

    /// Get the version of document selected by this query.
    pub(crate) fn version(&self) -> DocumentVersion {
        match self {
            Self::Text { document, .. } => document.version(),
            // Self::Notebook { notebook, .. } => notebook.version(),
        }
    }

    /// Get the URL for the document selected by this query.
    pub(crate) fn file_url(&self) -> Url {
        match self {
            Self::Text { document_key, .. } => document_key.url(),
            // Self::Notebook { file_url, .. } => file_url,
        }
    }

    pub(crate) fn file_document_key(&self) -> &DocumentKey {
        match self {
            Self::Text { document_key, .. } => document_key,
        }
    }

    /// Get the path for the document selected by this query.
    ///
    /// Returns `None` if this is an unsaved (untitled) document.
    ///
    /// The path isn't guaranteed to point to a real path on the filesystem. This is the case
    /// for unsaved (untitled) documents.
    pub(crate) fn file_path(&self) -> Option<PathBuf> {
        self.file_url().to_file_path().ok()
    }

    // /// Get the path for the document selected by this query, ignoring whether the file exists on disk.
    // ///
    // /// Returns the URL's path if this is an unsaved (untitled) document.
    // pub(crate) fn virtual_file_path(&self) -> Cow<Path> {
    //     self.file_path()
    //         .map(Cow::Owned)
    //         .unwrap_or_else(|| Cow::Borrowed(Path::new(self.file_url().path())))
    // }

    /// Attempt to access the single inner text document selected by the query.
    /// If this query is selecting an entire notebook document, this will return `None`.
    pub(crate) fn as_single_document(&self) -> Option<&TextDocument> {
        match self {
            Self::Text { document, .. } => Some(document),
        }
    }
}
