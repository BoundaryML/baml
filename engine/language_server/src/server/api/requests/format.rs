use crate::baml_project::position_utils::full_document_range;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::{DocumentKey, Session};
use internal_baml_core::internal_baml_schema_ast::{format_schema, FormatOptions};
use lsp_types::{request, DocumentFormattingParams, TextEdit};
use std::path::PathBuf;
pub(crate) struct DocumentFormatting;

impl RequestHandler for DocumentFormatting {
    type RequestType = request::Formatting;
}

impl SyncRequestHandler for DocumentFormatting {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<lsp_types::TextEdit>>> {
        let url = &params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session
            .ensure_project_db_for_baml_file(url)
            .internal_error()?;
        let project = session
            .project_db_for_path_mut(path)
            .expect("Ensured that a project db exists");
        let document_key =
            DocumentKey::from_url(&PathBuf::from(project.root_path()), &url).internal_error()?;
        let doc_contents = match project.baml_project.files.get(&document_key) {
            None => {
                tracing::warn!("Failed to find doc {:?}", url);
                Err(anyhow::anyhow!(
                    "File {} was not present in the project",
                    url
                ))
            }
            Some(text_document) => Ok(text_document.contents.clone()),
        }
        .internal_error()?;
        format_schema(
            &doc_contents,
            FormatOptions {
                indent_width: 2,
                fail_on_unhandled_rule: false,
            },
        )
        .map(|new_contents| {
            Ok(Some(vec![TextEdit {
                range: full_document_range(&doc_contents),
                new_text: new_contents,
            }]))
        })
        .unwrap_or_else(|e| {
            notifier
                .notify_baml_error(e.to_string().as_str())
                .internal_error()?;
            Ok(None)
        })
    }
}

// use anyhow::Context;
// use lsp_types::{self as types, request as req};
// use types::TextEdit;

// use ruff_source_file::LineIndex;

// use crate::edit::{Replacement, ToRangeExt};
// use crate::fix::Fixes;
// use crate::resolve::is_document_excluded_for_formatting;
// use crate::server::api::LSPResult;
// use crate::server::{client::Notifier, Result};
// use crate::session::{DocumentQuery, DocumentSnapshot};
// use crate::{PositionEncoding, TextDocument};

// pub(crate) struct Format;

// impl super::RequestHandler for Format {
//     type RequestType = req::Formatting;
// }

// impl super::BackgroundDocumentRequestHandler for Format {
//     super::define_document_url!(params: &types::DocumentFormattingParams);
//     fn run_with_snapshot(
//         snapshot: DocumentSnapshot,
//         _notifier: Notifier,
//         _params: types::DocumentFormattingParams,
//     ) -> Result<super::FormatResponse> {
//         format_document(&snapshot)
//     }
// }

// /// Formats either a full text document or each individual cell in a single notebook document.
// pub(super) fn format_full_document(snapshot: &DocumentSnapshot) -> Result<Fixes> {
//     let mut fixes = Fixes::default();
//     let query = snapshot.query();

//     match snapshot.query() {
//         DocumentQuery::Notebook { notebook, .. } => {
//             for (url, text_document) in notebook
//                 .urls()
//                 .map(|url| (url.clone(), notebook.cell_document_by_uri(url).unwrap()))
//             {
//                 if let Some(changes) =
//                     format_text_document(text_document, query, snapshot.encoding(), true)?
//                 {
//                     fixes.insert(url, changes);
//                 }
//             }
//         }
//         DocumentQuery::Text { document, .. } => {
//             if let Some(changes) =
//                 format_text_document(document, query, snapshot.encoding(), false)?
//             {
//                 fixes.insert(snapshot.query().make_key().into_url(), changes);
//             }
//         }
//     }

//     Ok(fixes)
// }

// /// Formats either a full text document or an specific notebook cell. If the query within the snapshot is a notebook document
// /// with no selected cell, this will throw an error.
// pub(super) fn format_document(snapshot: &DocumentSnapshot) -> Result<super::FormatResponse> {
//     let text_document = snapshot
//         .query()
//         .as_single_document()
//         .context("Failed to get text document for the format request")
//         .unwrap();
//     let query = snapshot.query();
//     format_text_document(
//         text_document,
//         query,
//         snapshot.encoding(),
//         query.as_notebook().is_some(),
//     )
// }

// fn format_text_document(
//     text_document: &TextDocument,
//     query: &DocumentQuery,
//     encoding: PositionEncoding,
//     is_notebook: bool,
// ) -> Result<super::FormatResponse> {
//     let file_resolver_settings = query.settings().file_resolver();
//     let formatter_settings = query.settings().formatter();

//     // If the document is excluded, return early.
//     if let Some(file_path) = query.file_path() {
//         if is_document_excluded_for_formatting(
//             &file_path,
//             file_resolver_settings,
//             formatter_settings,
//             text_document.language_id(),
//         ) {
//             return Ok(None);
//         }
//     }

//     let source = text_document.contents();
//     let formatted = crate::format::format(text_document, query.source_type(), formatter_settings)
//         .with_failure_code(lsp_server::ErrorCode::InternalError)?;
//     let Some(mut formatted) = formatted else {
//         return Ok(None);
//     };

//     // special case - avoid adding a newline to a notebook cell if it didn't already exist
//     if is_notebook {
//         let mut trimmed = formatted.as_str();
//         if !source.ends_with("\r\n") {
//             trimmed = trimmed.trim_end_matches("\r\n");
//         }
//         if !source.ends_with('\n') {
//             trimmed = trimmed.trim_end_matches('\n');
//         }
//         if !source.ends_with('\r') {
//             trimmed = trimmed.trim_end_matches('\r');
//         }
//         formatted = trimmed.to_string();
//     }

//     let formatted_index: LineIndex = LineIndex::from_source_text(&formatted);

//     let unformatted_index = text_document.index();

//     let Replacement {
//         source_range,
//         modified_range: formatted_range,
//     } = Replacement::between(
//         source,
//         unformatted_index.line_starts(),
//         &formatted,
//         formatted_index.line_starts(),
//     );

//     Ok(Some(vec![TextEdit {
//         range: source_range.to_range(source, unformatted_index, encoding),
//         new_text: formatted[formatted_range].to_owned(),
//     }]))
// }
