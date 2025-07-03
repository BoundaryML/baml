use std::path::PathBuf;

use lsp_types::{
    self, request as req, GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range,
    Url,
};

#[cfg(feature = "playground-server")]
use crate::playground::broadcast_function_change;
use crate::{
    baml_project::{position_utils::get_word_at_position, trim_line, BamlRuntimeExt},
    server::{
        api::{
            traits::{RequestHandler, SyncRequestHandler},
            ResultExt,
        },
        client::{Notifier, Requester},
        Result,
    },
    DocumentKey, Session,
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
        let url = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        if !url.to_string().contains("baml_src") {
            return Ok(None);
        }

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");
        project
            .lock()
            .unwrap()
            .update_runtime(Some(notifier))
            .internal_error()?;

        let document_key = DocumentKey::from_url(
            &project.lock().unwrap().baml_project.root_dir_name,
            &params.text_document_position_params.text_document.uri,
        )
        .internal_error()?;
        let guard = project.lock().unwrap();
        let doc = guard
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
        let rt = guard.runtime().internal_error()?;
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

                // Broadcast function change to playground clients
                #[cfg(feature = "playground-server")]
                if let Some(state) = &session.playground_state {
                    // Get the first function from the current file if available
                    if let Some(function) = guard
                        .list_functions()
                        .unwrap_or_default()
                        .into_iter()
                        .find(|f| f.span.file_path == document_key.path().to_string_lossy())
                    {
                        tracing::info!("Broadcasting function change for: {}", function.name);
                        let root_path = guard.root_path().to_string_lossy().to_string();
                        let state = state.clone();
                        let function_name = function.name.clone();
                        if let Some(runtime) = &session.playground_runtime {
                            runtime.spawn(async move {
                                let _ =
                                    broadcast_function_change(&state, &root_path, function_name)
                                        .await;
                            });
                        }
                    }
                }

                Ok(Some(goto_definition_response))
            }
        }
    }
}
