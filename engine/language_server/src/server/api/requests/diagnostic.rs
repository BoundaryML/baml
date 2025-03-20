use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport, Url,
};

use crate::baml_project::Project;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler, SyncRequestHandler};
use crate::server::api::ResultExt;
use crate::server::Result;
use crate::server::client::{Notifier, Requester};
use crate::DocumentKey;
use crate::session::{DocumentSnapshot, Session};
use crate::server::api::diagnostics::{project_diagnostics, session_lsp_diagnostics};

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: Project,
        _notifier: Notifier,
        _params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        tracing::info!("****** RUN_WITH_SNAPSHOTDocumentDiagnosticRequestHandler");
        todo!()
        // let diagnostics = project_diagnostics(&snapshot, &db);

        // Ok(DocumentDiagnosticReportResult::Report(
        //     DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
        //         related_documents: None,
        //         full_document_diagnostic_report: FullDocumentDiagnosticReport {
        //             result_id: None,
        //             items: diagnostics,
        //         },
        //     }),
        // ))
    }
}

impl SyncRequestHandler for DocumentDiagnosticRequestHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        requester: &mut Requester,
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





//     // let range = if let (Some(file), Some(range)) = (diagnostic.file(), diagnostic.range()) {
//     //     let index = line_index(db.upcast(), file);
//     //     let source = source_text(db.upcast(), file);

//     //     range.to_range(&source, &index, encoding)
//     // } else {
//     //     Range::default()
//     // };

//     let severity = match diagnostic.severity() {
//         Severity::Info => DiagnosticSeverity::INFORMATION,
//         Severity::Warning => DiagnosticSeverity::WARNING,
//         Severity::Error | Severity::Fatal => DiagnosticSeverity::ERROR,
//     };

//     Diagnostic {
//         range: Range::default(),
//         severity: Some(severity),
//         tags: None,
//         code: Some(NumberOrString::String(diagnostic.id().to_string())),
//         code_description: None,
//         source: Some("red-knot".into()),
//         message: diagnostic.message().into_owned(),
//         related_information: None,
//         data: None,
//     }
// }
