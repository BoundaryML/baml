use crate::baml_project::position_utils::get_word_at_position;
use crate::baml_project::trim_line;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::{DocumentKey, Session};
use lsp_types::{request, CompletionItem, CompletionList, CompletionParams, CompletionResponse};
use std::path::PathBuf;
pub(crate) struct Completion;

impl RequestHandler for Rename {
    type RequestType = request::Rename;
}

impl SyncRequestHandler for Rename {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: RenameParams,
    ) -> Result<Option<lsp_types::WorkspaceEdit>> {
        let url = params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session
            .ensure_project_db_for_baml_file(&url)
            .internal_error()?;
        let project = session
            .project_db_for_path(path)
            .expect("Ensured that a project db exists");
        let document_key =
            DocumentKey::from_url(&PathBuf::from(project.root_path()), &url).internal_error()?;
        
        // Get the symbol under point.
        let doc = project.baml_project.files.get(&document_key).ok_or(anyhow::anyhow!(
            "File {} was not present in the project",
            document_key
        ))
        .internal_error()?;
        let symbol = get_word_at_position(&doc.contents, &params.position);
        let new_symbol = params.new_name;

        // If the symbol is a class, find all 
        let rt = project.baml_project.runtime(HashMap::new()).clone();
        if rt.is_valid_class(symbol) {
            let symbol_locations = rt.search_for_class_locations(symbol);
            let mut changes = HashMap::new();

            symbol_locations.iter().for_each(|loc| {
                let range = lsp_types::Range::new(
                    lsp_types::Position::new(loc.line, loc.column),
                    lsp_types::Position::new(loc.line, loc.column + symbol.len()),
                );
                let uri = loc.uri.clone();
                let text_edit = TextEdit { range, new_text: new_symbol };

                let mut entry = changes.entry(uri).or_insert_with(Vec::new);
                entry.push(text_edit);
            });
            return Ok(Some(WorkspaceEdit {
                changes,
                document_changes: None,
                change_annotations: None,
            }))
        }
        
    }
            
}