use lsp_types::{self as types, HoverParams, request as req};

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
        let Ok(project) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        let guard = project.lock();
        let lsp_db = guard.lsp_db();

        // Get the SourceFile for this path
        let Some(source_file) = lsp_db.get_file(&path) else {
            tracing::debug!("Hover: file not found in LspDatabase: {:?}", path);
            return Ok(None);
        };

        // Find symbol at position
        let Some(symbol) = lsp_db.symbol_at_position(source_file, position) else {
            return Ok(None);
        };

        // Generate hover text
        let hover_text = lsp_db.get_hover_text(&symbol);

        Ok(Some(types::Hover {
            contents: types::HoverContents::Markup(types::MarkupContent {
                kind: types::MarkupKind::Markdown,
                value: format!("```baml\n{}\n```", hover_text),
            }),
            range: None,
        }))
    }
}
