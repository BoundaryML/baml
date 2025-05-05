use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::client::Requester;
use crate::server::{client::Notifier, Result};
use crate::{DocumentKey, Session};
use lsp_types::{self as types, request as req, HoverParams, TextDocumentItem};

pub(crate) struct Hover;

impl RequestHandler for Hover {
    type RequestType = req::HoverRequest;
}

impl SyncRequestHandler for Hover {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: HoverParams,
    ) -> Result<Option<types::Hover>> {
        let url = &params.text_document_position_params.text_document.uri;
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");

        let document_key =
            DocumentKey::from_url(project.lock().unwrap().root_path(), &url).internal_error()?;

        let text_document_item = match project
            .lock()
            .unwrap()
            .baml_project
            .files
            .get(&document_key)
        {
            None => {
                tracing::warn!("*** HOVER: Failed to find doc {:?}", url);
                Err(anyhow::anyhow!(
                    "File {} was not present in the project",
                    url
                ))
            }
            Some(text_document) => Ok(TextDocumentItem {
                uri: url.clone(),
                language_id: "BAML".to_string(),
                text: text_document.contents.clone(),
                version: 1,
            }),
        }
        .internal_error()?;
        let position = params.text_document_position_params.position;
        // Just swallow the error here, we dont want hover failures to show error notifs for a user.
        let hover = match project.lock().unwrap().handle_hover_request(
            &text_document_item,
            &position,
            notifier,
        ) {
            Ok(hover) => hover,
            Err(e) => {
                tracing::error!("Error handling hover request: {}", e);
                None
            }
        };
        Ok(hover)
    }
}
