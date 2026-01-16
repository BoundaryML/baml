// TODO: This file has been modified to remove baml_runtime/playground_server dependencies.
// For now, go-to-definition returns None.

use std::path::PathBuf;

use lsp_types::{
    self, GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range, Url,
    request as req,
};

// TODO: playground_server is disabled for now
// use playground_server::WebviewRouterMessage;
use crate::{
    DocumentKey,
    Session,
    // TODO: BamlRuntimeExt is disabled for now
    // baml_project::{BamlRuntimeExt, position_utils::get_word_at_position, trim_line},
    baml_project::{position_utils::get_word_at_position, trim_line},
    edit::ToRangeExt,
    server::{
        Result,
        api::{
            ResultExt,
            traits::{RequestHandler, SyncRequestHandler},
        },
        client::{Notifier, Requester},
    },
};

pub struct GotoDefinition;

impl RequestHandler for GotoDefinition {
    type RequestType = req::GotoDefinition;
}

impl SyncRequestHandler for GotoDefinition {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        tracing::debug!("=== GOTO DEFINITION HANDLER CALLED ===");

        // Get the file path from the URL
        let url = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        tracing::debug!("Got URL: {}", url);

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        tracing::debug!("Converted to path: {:?}", path);

        // Get or create the project for this file
        let Ok(project) = session.get_or_create_project(&path) else {
            tracing::debug!("Failed to get or create project for path: {:?}", path);
            return Ok(None);
        };
        tracing::debug!("Got project for path");

        // Get the position in the document
        let position = params.text_document_position_params.position;
        tracing::debug!(
            "Position in document - line: {}, character: {}",
            position.line,
            position.character
        );

        // Get the database and find the file ID
        let guard = project.lock();
        let db = guard.db();
        tracing::debug!("Got database from project");

        // Get the FileId for this path
        let file_id = match db.path_to_file_id(&path) {
            Some(id) => {
                tracing::debug!("Got FileId for path");
                id
            }
            None => {
                tracing::debug!("Could not get FileId for path: {:?}", path);
                return Ok(None);
            }
        };

        // Get the source file to calculate proper text position
        let source_file = match db.get_file_by_id(file_id) {
            Some(f) => {
                tracing::debug!("Got source file");
                f
            }
            None => {
                tracing::debug!("Could not get source file for FileId");
                return Ok(None);
            }
        };

        // Get the source text and create a LineIndex
        let text = source_file.text(db);
        tracing::debug!("Source file text length: {} chars", text.len());
        let line_index = crate::baml_source_file::LineIndex::from_source_text(text);

        // Convert LSP position to TextSize using LineIndex
        let text_position = line_index.offset(
            crate::baml_source_file::OneIndexed::from_zero_indexed(position.line as usize),
            crate::baml_source_file::OneIndexed::from_zero_indexed(position.character as usize),
            text,
        );
        tracing::debug!(
            "Converted position to text offset: {}",
            text_position.as_u32()
        );

        // Call the baml_ide goto_definition function (convert TextSize types)
        let text_size_position = text_size::TextSize::from(text_position.as_u32());
        tracing::debug!(
            "Calling baml_ide::goto_definition with position: {:?}",
            text_size_position
        );
        match baml_ide::goto_definition(db, file_id, text_size_position) {
            Some(nav_target) => {
                tracing::debug!("Got navigation target!");
                tracing::debug!("  Target name: {}", nav_target.name);
                tracing::debug!("  Target file: {:?}", nav_target.file_path);
                tracing::debug!("  Target span: {:?}", nav_target.span);

                // Convert NavigationTarget to LSP Location
                let target_uri = Url::from_file_path(&nav_target.file_path)
                    .map_err(|_| anyhow::anyhow!("Failed to convert path to URI"))
                    .internal_error()?;

                // Get the target file's text and create a LineIndex
                let target_file_id = match db.path_to_file_id(&nav_target.file_path) {
                    Some(id) => id,
                    None => return Ok(None),
                };
                let target_source = match db.get_file_by_id(target_file_id) {
                    Some(f) => f,
                    None => return Ok(None),
                };
                let target_text = target_source.text(db);
                let target_line_index =
                    crate::baml_source_file::LineIndex::from_source_text(target_text);

                // Convert the span to LSP range using LineIndex
                let start_u32: u32 = nav_target.span.range.start().into();
                let end_u32: u32 = nav_target.span.range.end().into();
                let local_text_range = crate::baml_text_size::TextRange::new(
                    crate::baml_text_size::TextSize::from(start_u32),
                    crate::baml_text_size::TextSize::from(end_u32),
                );
                let range = local_text_range.to_range(
                    target_text,
                    &target_line_index,
                    crate::edit::PositionEncoding::UTF8,
                );

                tracing::debug!(
                    "Returning LSP Location - uri: {}, range: {:?}",
                    target_uri,
                    range
                );
                Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range,
                })))
            }
            None => {
                tracing::debug!("goto_definition returned None - no target found");
                Ok(None)
            }
        }

        // TODO: Original implementation commented out below
        // let url = params
        //     .text_document_position_params
        //     .text_document
        //     .uri
        //     .clone();
        // let path = url
        //     .to_file_path()
        //     .internal_error_msg("Could not convert URL to path")?;
        // let Ok(project) = session.get_or_create_project(&path) else {
        //     return Ok(None);
        // };

        // let document_key = DocumentKey::from_url(
        //     &project.lock().baml_project.root_dir_name,
        //     &params.text_document_position_params.text_document.uri,
        // )
        // .internal_error()?;
        // let guard = project.lock();
        // let doc = guard
        //     .baml_project
        //     .files
        //     .get(&document_key)
        //     .ok_or(anyhow::anyhow!(
        //         "File {} was not present in the project",
        //         document_key
        //     ))
        //     .internal_error()?;
        // let word = get_word_at_position(
        //     &doc.contents,
        //     &params.text_document_position_params.position,
        // );
        // let cleaned_word = trim_line(&word);
        // if cleaned_word.is_empty() {
        //     return Ok(None);
        // }
        // let rt = guard.runtime().internal_error()?;
        // let maybe_symbol = rt.search_for_symbol(&cleaned_word);
        // match maybe_symbol {
        //     None => Ok(None),
        //     Some(symbol_location) => {
        //         let range = Range {
        //             start: Position {
        //                 line: symbol_location.start_line as u32,
        //                 character: symbol_location.start_character as u32,
        //             },
        //             end: Position {
        //                 line: symbol_location.end_line as u32,
        //                 character: symbol_location.end_character as u32,
        //             },
        //         };
        //         let target_uri = Url::from_file_path(&symbol_location.uri)
        //             .map_err(|_| anyhow::anyhow!("Failed to parse target URI"))
        //             .internal_error()?;
        //         let goto_definition_response = GotoDefinitionResponse::Scalar(Location {
        //             uri: target_uri,
        //             range,
        //         });

        //         Ok(Some(goto_definition_response))
        //     }
        // }
    }
}
