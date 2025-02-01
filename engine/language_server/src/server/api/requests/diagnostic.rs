use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport, Url,
};

use crate::baml_project::Project;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::Result;
use crate::server::client::{Notifier, Requester};
use crate::session::Session;
use crate::server::api::diagnostics::project_diagnostics;

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

// // Consider fixing snapshots and running this on a background thread.
// impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
// }

impl SyncRequestHandler for DocumentDiagnosticRequestHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let url = params.text_document.uri.clone();
        let path = url.to_file_path().internal_error_msg("Could not convert URL to path")?;

        session.ensure_project_db_for_baml_file(&params.text_document.uri).internal_error()?;
        let project = session.project_db_for_path_mut(path).expect("Just ensured it exists");

        let diagnostics = project_diagnostics(project, Some(&url));
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

fn diagnostics_report(project: &Project, url: &Url) -> Result<DocumentDiagnosticReportResult> {
    let diagnostics = project_diagnostics(project, Some(url));
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
