use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport, Url,
};

use crate::baml_project::Project;
use crate::server::api::diagnostics::{file_diagnostics, project_diagnostics};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, SyncRequestHandler,
};
use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::DocumentSnapshot;

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

// // Consider fixing snapshots and running this on a background thread.
impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> std::borrow::Cow<Url> {
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
        let path = url
            .to_file_path()
            .internal_error_msg("Could not convert URL to path")?;

        let project = session
            .get_or_create_project(&path)
            .expect("Project should exist");

        let diagnostics = file_diagnostics(project, &url);
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
    let diagnostics = file_diagnostics(project, url);
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
