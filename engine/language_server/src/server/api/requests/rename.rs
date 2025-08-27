use std::{collections::HashMap, path::PathBuf};

use lsp_types::{request, RenameParams, TextEdit, WorkspaceEdit};
use url::Url;

use crate::{
    baml_project::{position_utils::get_word_at_position, BamlRuntimeExt},
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

pub struct Rename;

impl RequestHandler for Rename {
    type RequestType = request::Rename;
}

impl SyncRequestHandler for Rename {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: RenameParams,
    ) -> Result<Option<lsp_types::WorkspaceEdit>> {
        let url = params.text_document_position.text_document.uri;
        if !url.to_string().contains("baml_src") {
            return Ok(None);
        }

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;
        let project = session
            .get_or_create_project(&path)
            .expect("Ensured that a project db exists");

        let res = {
            let mut guard = project.lock();
            let document_key =
                DocumentKey::from_url(&PathBuf::from(guard.root_path()), &url).internal_error()?;

            // Get the symbol under point.
            let doc = guard
                .baml_project
                .files
                .get(&document_key)
                .ok_or(anyhow::anyhow!(
                    "File {} was not present in the project",
                    document_key
                ))
                .internal_error()?;
            let symbol =
                get_word_at_position(&doc.contents, &params.text_document_position.position);
            let new_symbol = params.new_name;

            // If the symbol is a class, find all occurrences and replace them.
            let default_flags = vec!["beta".to_string()];
            let runtime = guard.baml_project.runtime(
                HashMap::new(),
                session
                    .baml_settings
                    .feature_flags
                    .as_ref()
                    .unwrap_or(&default_flags),
            );
            let rt = runtime
                .as_ref()
                .map_err(|_| anyhow::anyhow!("Failed to get runtime"))
                .internal_error()?;
            log::info!("------------ RUNTIME 2----------");

            if rt.is_valid_function(&symbol) {
                // TODO: Implement function renaming
                return Err(anyhow::anyhow!(
                    "Function renaming is not yet supported: '{}'",
                    symbol
                ))
                .internal_error();
            }

            let is_valid_class = rt.is_valid_class(&symbol);
            let is_valid_enum = rt.is_valid_enum(&symbol);
            let is_valid_type_alias = rt.is_valid_type_alias(&symbol);

            // Only classes, enums, and type aliases can be renamed for now.
            if !is_valid_class && !is_valid_enum && !is_valid_type_alias {
                return Err(anyhow::anyhow!("Cannot rename symbol '{}'", symbol)).internal_error();
            }

            let symbol_locations = if is_valid_class {
                rt.search_for_class_locations(&symbol)
            } else if is_valid_enum {
                rt.search_for_enum_locations(&symbol)
            } else {
                // Must be a type alias based on the check above
                rt.search_for_type_alias_locations(&symbol)
            };

            let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

            symbol_locations.iter().try_for_each(|loc| {
                let loc_url = PathBuf::from(&loc.uri);
                let range = lsp_types::Range::new(
                    lsp_types::Position::new(loc.start_line as u32, loc.start_character as u32),
                    lsp_types::Position::new(loc.end_line as u32, loc.end_character as u32),
                );
                let symbol_doc_key =
                    DocumentKey::from_path(&PathBuf::from(guard.root_path()), &loc_url)
                        .internal_error()?;
                let text_edit = TextEdit {
                    range,
                    new_text: new_symbol.clone(),
                };

                let entry = changes.entry(symbol_doc_key.url()).or_default();
                entry.push(text_edit);
                Ok(())
            })?;

            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }))
        };

        // Project lock is released so session can reload
        session.reload(Some(notifier)).internal_error()?;

        res
    }
}
