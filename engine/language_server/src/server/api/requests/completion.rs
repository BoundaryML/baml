use std::path::PathBuf;

use lsp_types::{request, CompletionItem, CompletionList, CompletionParams, CompletionResponse};

use crate::{
    baml_project::{position_utils::get_word_at_position, trim_line},
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
    ) -> Result<Option<lsp_types::CompletionResponse>> {
        let url = params.text_document_position.text_document.uri;
        if !url.to_string().contains("baml_src") {
            return Ok(None);
        }

        // TODO: Enable this only if you
        // 1. test on windows, with chinese characters
        // 2. Modify position_utils.rs to use byte offsets to account for chinese/multibyte characters
        // 3. Don't crash if you index into a string with a byte offset that is out of bounds
        // let url = params.text_document_position.text_document.uri;
        // let path = url
        //     .to_file_path()
        //     .internal_error_msg("Could not convert URL to path")?;

        // // Use the unified method to get or create the project
        // let project = session
        //     .get_or_create_project(&path)
        //     .expect("Failed to get or create project");

        // let guard = project.lock();
        // let document_key =
        //     DocumentKey::from_url(&PathBuf::from(guard.root_path()), &url).internal_error()?;
        // let doc = guard
        //     .baml_project
        //     .files
        //     .get(&document_key)
        //     .ok_or(anyhow::anyhow!(
        //         "File {} was not present in the project",
        //         document_key
        //     ))
        //     .internal_error()?;
        // let word = get_word_at_position(&doc.contents, &params.text_document_position.position);
        // let cleaned_word = trim_line(&word);
        // // let cleaned_word = word;
        // let completions = match cleaned_word.as_str() {
        //     "_." => Some(vec![
        //         r#"role("system")"#,
        //         r#"role("assistant")"#,
        //         r#"role("user")"#,
        //     ]),
        //     "ctx." => Some(vec![r#"output_format"#, r#"client"#]),
        //     "ctx.client." => Some(vec![r#"name"#, r#"provider"#]),
        //     _ => None,
        // };
        // Ok(completions.map(|completions| {
        //     let completion_list = CompletionList {
        //         is_incomplete: false,
        //         items: completions
        //             .into_iter()
        //             .map(|completion| CompletionItem {
        //                 label: completion.to_string(),
        //                 ..CompletionItem::default()
        //             })
        //             .collect(),
        //         ..CompletionList::default()
        //     };
        //     CompletionResponse::List(completion_list)
        // }))
        Ok(None)
    }
}
