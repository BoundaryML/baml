use std::path::PathBuf;

use internal_baml_core::internal_baml_ast::{format_schema, FormatOptions};
use lsp_types::{request, DocumentFormattingParams, TextEdit};

use crate::{
    baml_project::position_utils::full_document_range,
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
        let url = params.text_document.uri;
        if !url.to_string().contains("baml_src") {
            return Ok(None);
        }

        // let url = &params.text_document.uri;
        // let path = url
        //     .to_file_path()
        //     .internal_error_msg("Could not convert URL to path")?;
        // session
        //     .ensure_project_db_for_baml_file(url)
        //     .internal_error()?;
        // let project = session
        //     .project_db_for_path_mut(path)
        //     .expect("Ensured that a project db exists");
        // let document_key = DocumentKey::from_url(
        //     &PathBuf::from(project.lock().baml_project.root_dir_name.clone()),
        //     &url,
        // )
        // .internal_error()?;
        // let doc_contents = match project
        //     .lock()
        //     .unwrap()
        //     .baml_project
        //     .files
        //     .get(&document_key)
        // {
        //     None => {
        //         tracing::warn!("Failed to find doc {:?}", url);
        //         Err(anyhow::anyhow!(
        //             "File {} was not present in the project",
        //             url
        //         ))
        //     }
        //     Some(text_document) => Ok(text_document.contents.clone()),
        // }
        // .internal_error()?;
        // format_schema(
        //     &doc_contents,
        //     FormatOptions {
        //         indent_width: 2,
        //         fail_on_unhandled_rule: false,
        //     },
        // )
        // .map(|new_contents| {
        //     Ok(Some(vec![TextEdit {
        //         range: full_document_range(&doc_contents),
        //         new_text: new_contents,
        //     }]))
        // })
        // .unwrap_or_else(|e| {
        //     notifier
        //         .notify_baml_error(e.to_string().as_str())
        //         .internal_error()?;
        //     Ok(None)
        // })
        Ok(None)
    }
}
