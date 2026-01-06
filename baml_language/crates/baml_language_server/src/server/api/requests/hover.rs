use baml_project::position::lsp_position_to_offset;
use lsp_types::{self as types, HoverParams, request as req};
use text_size::TextSize;

use crate::{
    Session,
    server::{
        Result,
        api::{
            ResultExt,
            traits::{RequestHandler, SyncRequestHandler},
        },
        client::{Notifier, Requester},
    },
};

pub(crate) struct Hover;

impl RequestHandler for Hover {
    type RequestType = req::HoverRequest;
}

impl SyncRequestHandler for Hover {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: HoverParams,
    ) -> Result<Option<types::Hover>> {
        let url = &params.text_document_position_params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let position = &params.text_document_position_params.position;

        // Get the project to access the LspDatabase
        let Ok(project_handle) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        let guard = project_handle.lock();
        let lsp_db = guard.lsp_db();

        // Get the SourceFile for this path
        let Some(source_file) = lsp_db.get_file(&path) else {
            tracing::debug!("Hover: file not found in LspDatabase: {:?}", path);
            return Ok(None);
        };

        // Get the project for hover lookup
        let Some(project) = lsp_db.project() else {
            return Ok(None);
        };

        // Convert LSP position to byte offset
        let text = source_file.text(lsp_db.db());
        let offset = TextSize::from(lsp_position_to_offset(text, position) as u32);

        // Use baml_ide hover
        let hover_result = baml_ide::hover::hover(lsp_db.db(), source_file, project, offset);

        match hover_result {
            Some(hover) => {
                let content = hover.display(baml_ide::MarkupKind::Markdown);
                Ok(Some(types::Hover {
                    contents: types::HoverContents::Markup(types::MarkupContent {
                        kind: types::MarkupKind::Markdown,
                        value: content,
                    }),
                    range: None, // Could use hover.range if needed
                }))
            }
            None => Ok(None),
        }
    }
}
