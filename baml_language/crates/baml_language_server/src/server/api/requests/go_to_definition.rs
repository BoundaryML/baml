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
        notifier: Notifier,
        _requester: &mut Requester,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        // TODO: Go-to-definition is disabled until we have the salsa database integration
        // For now, return None
        Ok(None)

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
