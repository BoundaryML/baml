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

        // Use baml_ide completion
        let completions = baml_ide::complete(lsp_db.db(), source_file, project, offset);

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

/// Convert a baml_ide CompletionItem to an LSP CompletionItem.
fn to_lsp_completion_item(item: baml_ide::CompletionItem) -> CompletionItem {
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

/// Convert a baml_ide CompletionKind to an LSP CompletionItemKind.
fn to_lsp_completion_kind(kind: baml_ide::CompletionKind) -> CompletionItemKind {
    match kind {
        baml_ide::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        baml_ide::CompletionKind::Function => CompletionItemKind::FUNCTION,
        baml_ide::CompletionKind::Class => CompletionItemKind::CLASS,
        baml_ide::CompletionKind::Enum => CompletionItemKind::ENUM,
        baml_ide::CompletionKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
        baml_ide::CompletionKind::Field => CompletionItemKind::FIELD,
        baml_ide::CompletionKind::Client => CompletionItemKind::MODULE,
        baml_ide::CompletionKind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
        baml_ide::CompletionKind::Property => CompletionItemKind::PROPERTY,
        baml_ide::CompletionKind::Snippet => CompletionItemKind::SNIPPET,
        baml_ide::CompletionKind::Generator => CompletionItemKind::MODULE,
        baml_ide::CompletionKind::Test => CompletionItemKind::METHOD,
        baml_ide::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
    }
}
