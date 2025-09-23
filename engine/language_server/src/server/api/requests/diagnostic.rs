use std::{borrow::Cow, sync::Arc};

use lsp_types::{
    request::DocumentDiagnosticRequest, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
    RelatedFullDocumentDiagnosticReport, RelatedUnchangedDocumentDiagnosticReport,
    UnchangedDocumentDiagnosticReport, Url,
};
use parking_lot::Mutex;

use crate::{
    baml_project::Project,
    server::{
        api::{
            diagnostics::{file_diagnostics, project_diagnostics},
            traits::{BackgroundDocumentRequestHandler, RequestHandler, SyncRequestHandler},
            ResultExt,
        },
        client::{Notifier, Requester},
        Result,
    },
    session::Session,
    DocumentSnapshot,
};

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

// // Consider fixing snapshots and running this on a background thread.
impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> std::borrow::Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        _snapshot: DocumentSnapshot,
        db: Arc<Mutex<Project>>,
        _notifier: Notifier,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        diagnostics_report(db, &params.text_document.uri)
    }
}

impl SyncRequestHandler for DocumentDiagnosticRequestHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let url = params.text_document.uri.clone();
        if !url.to_string().contains("baml_src") {
            return Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                    related_documents: None,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id: "".to_string(),
                    },
                }),
            ));
        }

        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        let project = session
            .get_or_create_project(&path)
            .expect("Project should exist");

        let default_flags = vec!["beta".to_string()];
        let effective_flags = session
            .baml_settings
            .feature_flags
            .as_ref()
            .unwrap_or(&default_flags);
        tracing::info!(
            "diagnostic_request: session feature_flags: {:?}, effective_flags: {:?}",
            session
                .baml_settings
                .feature_flags
                .as_ref()
                .unwrap_or(&default_flags),
            &effective_flags
        );
        let diagnostics = file_diagnostics(project, &url, effective_flags);
        // diagnostics

        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: diagnostics,
                },
            }),
        ))
    }
}

fn diagnostics_report(
    project: Arc<Mutex<Project>>,
    url: &Url,
) -> Result<DocumentDiagnosticReportResult> {
    let diagnostics = file_diagnostics(project, url, &[]);
    Ok(DocumentDiagnosticReportResult::Report(
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items: diagnostics,
            },
        }),
    ))
}
