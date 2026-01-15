//! Find all references implementation for BAML LSP.

use lsp_types::{self, Location, Position, Range, ReferenceParams, Url, request as req};

use crate::{
    Session,
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

pub struct FindReferences;

impl RequestHandler for FindReferences {
    type RequestType = req::References;
}

impl SyncRequestHandler for FindReferences {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        // Get the file path from the URL
        let url = params.text_document_position.text_document.uri.clone();
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        // Get or create the project for this file
        let Ok(project) = session.get_or_create_project(&path) else {
            return Ok(None);
        };

        // Get the position in the document
        let position = params.text_document_position.position;

        // Get the database
        let guard = project.lock();
        let db = guard.db();

        // Get the FileId for this path
        let file_id = match db.path_to_file_id(&path) {
            Some(id) => id,
            None => return Ok(Some(Vec::new())),
        };

        // Get the source file to calculate proper text position
        let source_file = match db.get_file_by_id(file_id) {
            Some(f) => f,
            None => return Ok(Some(Vec::new())),
        };

        // Get the source text and create a LineIndex
        let text = source_file.text(db);
        let line_index = crate::baml_source_file::LineIndex::from_source_text(text);

        // Convert LSP position to TextSize using LineIndex
        let text_position = line_index.offset(
            crate::baml_source_file::OneIndexed::from_zero_indexed(position.line as usize),
            crate::baml_source_file::OneIndexed::from_zero_indexed(position.character as usize),
            text,
        );

        // Call the baml_ide find_all_references function (convert TextSize types)
        let text_size_position = text_size::TextSize::from(text_position.as_u32());
        let references = baml_ide::find_all_references(db, file_id, text_size_position);

        // Convert References to LSP Locations
        let locations: Vec<Location> = references
            .iter()
            .filter_map(|reference| {
                // Convert the file path to URI
                let uri = Url::from_file_path(&reference.file_path).ok()?;

                // Get the reference file's text and create a LineIndex
                let ref_file_id = db.path_to_file_id(&reference.file_path)?;
                let ref_source = db.get_file_by_id(ref_file_id)?;
                let ref_text = ref_source.text(db);
                let ref_line_index = crate::baml_source_file::LineIndex::from_source_text(ref_text);

                // Convert the span to LSP range using LineIndex
                let start_u32: u32 = reference.span.range.start().into();
                let end_u32: u32 = reference.span.range.end().into();
                let local_text_range = crate::baml_text_size::TextRange::new(
                    crate::baml_text_size::TextSize::from(start_u32),
                    crate::baml_text_size::TextSize::from(end_u32),
                );
                let range = local_text_range.to_range(
                    ref_text,
                    &ref_line_index,
                    crate::edit::PositionEncoding::UTF8,
                );

                Some(Location { uri, range })
            })
            .collect();

        Ok(Some(locations))
    }
}
