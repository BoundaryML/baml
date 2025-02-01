use crate::baml_project::position_utils::get_word_at_position;
use crate::baml_project::{trim_line, BamlRuntimeExt};
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::{DocumentKey, Session};
use anyhow::Context;
use lsp_types::{
    self, request as req, GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range,
    Url,
};
use std::path::PathBuf;

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
        let url = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        session
            .ensure_project_db_for_baml_file(
                &params.text_document_position_params.text_document.uri,
            )
            .internal_error()?;
        let project = session
            .project_db_for_path_mut(path)
            .expect("Ensured that a project db exists");
        project.update_runtime(Some(notifier)).internal_error()?;

        let document_key = DocumentKey::from_url(
            &PathBuf::from(project.root_path()),
            &params.text_document_position_params.text_document.uri,
        )
        .internal_error()?;
        let doc = project
            .baml_project
            .files
            .get(&document_key)
            .ok_or(anyhow::anyhow!(
                "File {} was not present in the project",
                document_key
            ))
            .internal_error()?;
        let word = get_word_at_position(
            &doc.contents,
            &params.text_document_position_params.position,
        );
        let cleaned_word = trim_line(&word);
        if cleaned_word.is_empty() {
            return Ok(None);
        }
        let rt = project.runtime().internal_error()?;
        let maybe_symbol = rt.search_for_symbol(&cleaned_word);
        match maybe_symbol {
            None => Ok(None),
            Some(symbol_location) => {
                let range = Range {
                    start: Position {
                        line: symbol_location.start_line as u32,
                        character: symbol_location.start_character as u32,
                    },
                    end: Position {
                        line: symbol_location.end_line as u32,
                        character: symbol_location.end_character as u32,
                    },
                };
                let target_uri = Url::from_file_path(&symbol_location.uri)
                    .map_err(|_| anyhow::anyhow!("Failed to parse target URI"))
                    .internal_error()?;
                let goto_definition_response = GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range,
                });
                Ok(Some(goto_definition_response))
            }
        }

        //     // let target_uri = Url::from_file_path(&symbol_location.uri).unwrap();
        //     // lsp_types::GotoDefinitionResponse::Scalar(Location {
        //     //     uri: target_uri,
        //     //     range,
        //     // })
        //     let maybe_target_uri = Url::parse(&symbol_location.uri);
        //     match maybe_target_uri {
        //         Ok(target_uri) => {
        //             Some(Location { uri: target_uri, range })
        //         }
        //         Err(_) => {
        //             eprintln!("Error parsing target URI: {:?}", symbol_location.uri);
        //             None
        //         },
        //     }

        // }).ok();
        // Ok(goto_definition_response)
    }
}

// handleDefinitionRequest(doc: TextDocument, position: Position): LocationLink[] {
//   const word = getWordAtPosition(doc, position)

//   //clean non-alphanumeric characters besides underscores and periods
//   const cleaned_word = trimLine(word)
//   if (cleaned_word === '') {
//     return []
//   }

//   // Search for the symbol in the runtime
//   const match = this.runtime().search_for_symbol(cleaned_word)

//   // If we found a match, return the location
//   if (match) {
//     return [
//       {
//         targetUri: URI.file(match.uri).toString(),
//         //unused default values for now
//         targetRange: {
//           start: { line: 0, character: 0 },
//           end: { line: 0, character: 0 },
//         },
//         targetSelectionRange: {
//           start: { line: match.start_line, character: match.start_character },
//           end: { line: match.end_line, character: match.end_character },
//         },
//       },
//     ]
//   }

//   return []
// }
