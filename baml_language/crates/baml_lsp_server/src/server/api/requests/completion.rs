//! LSP completion request handler.
//!
//! Provides autocomplete suggestions for BAML files.

use baml_project::position::lsp_position_to_offset;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    request,
};
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

pub(crate) struct Completion;

impl RequestHandler for Completion {
    type RequestType = request::Completion;
}

impl SyncRequestHandler for Completion {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let url = &params.text_document_position.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let position = &params.text_document_position.position;

        // Get the project to access the LspDatabase
        let Ok(project_handle) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        let guard = project_handle.lock();
        let lsp_db = guard.lsp_db();

        // Get the SourceFile for this path
        let Some(source_file) = lsp_db.get_file(&path) else {
            tracing::debug!("Completion: file not found in LspDatabase: {:?}", path);
            return Ok(None);
        };

        // Get the project for completion lookup
        let Some(project) = lsp_db.project() else {
            return Ok(None);
        };

        // Convert LSP position to byte offset (properly handles multibyte characters)
        let text = source_file.text(lsp_db.db());
        let offset = TextSize::from(lsp_position_to_offset(text, position) as u32);

        // Use baml_lsp_actions completion
        let completions = baml_lsp_actions::complete(lsp_db.db(), source_file, project, offset);

        // Convert to LSP types
        let items: Vec<CompletionItem> = completions
            .into_iter()
            .map(to_lsp_completion_item)
            .collect();

        if items.is_empty() {
            return Ok(None);
        }

        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        })))
    }
}

/// Convert a baml_lsp_actions CompletionItem to an LSP CompletionItem.
fn to_lsp_completion_item(item: baml_lsp_actions::CompletionItem) -> CompletionItem {
    CompletionItem {
        label: item.label,
        kind: Some(to_lsp_completion_kind(item.kind)),
        detail: item.detail,
        insert_text: item.insert_text,
        sort_text: item.sort_text,
        documentation: item.documentation.map(lsp_types::Documentation::String),
        ..Default::default()
    }
}

/// Convert a baml_lsp_actions CompletionKind to an LSP CompletionItemKind.
fn to_lsp_completion_kind(kind: baml_lsp_actions::CompletionKind) -> CompletionItemKind {
    match kind {
        baml_lsp_actions::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        baml_lsp_actions::CompletionKind::Function => CompletionItemKind::FUNCTION,
        baml_lsp_actions::CompletionKind::Class => CompletionItemKind::CLASS,
        baml_lsp_actions::CompletionKind::Enum => CompletionItemKind::ENUM,
        baml_lsp_actions::CompletionKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
        baml_lsp_actions::CompletionKind::Field => CompletionItemKind::FIELD,
        baml_lsp_actions::CompletionKind::Client => CompletionItemKind::MODULE,
        baml_lsp_actions::CompletionKind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
        baml_lsp_actions::CompletionKind::Property => CompletionItemKind::PROPERTY,
        baml_lsp_actions::CompletionKind::Snippet => CompletionItemKind::SNIPPET,
        baml_lsp_actions::CompletionKind::Generator => CompletionItemKind::MODULE,
        baml_lsp_actions::CompletionKind::Test => CompletionItemKind::METHOD,
        baml_lsp_actions::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
        baml_lsp_actions::CompletionKind::TemplateString => CompletionItemKind::FUNCTION,
    }
}
