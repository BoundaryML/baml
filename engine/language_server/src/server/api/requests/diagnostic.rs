use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport, Url,
};

use crate::baml_project::Project;
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use crate::server::api::diagnostics::session_lsp_diagnostics;

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
        tracing::info!("DocumentDiagnosticRequestHandler");
        let diagnostics = compute_diagnostics(&snapshot, &db);

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

fn compute_diagnostics(snapshot: &DocumentSnapshot, db: &Project) -> Vec<Diagnostic> {
    let Some(_file) = snapshot.file(db) else {
        tracing::info!(
            "No file found for snapshot for `{}`",
            snapshot.query().file_url()
        );

        // let diagnostics = session_lsp_diagnostics(session);
        return vec![];
    };

    // let diagnostics = match db.check_file(file) {
    //     Ok(diagnostics) => diagnostics,
    //     Err(cancelled) => {
    //         tracing::info!("Diagnostics computation {cancelled}");
    //         return vec![];
    //     }
    // };

    // diagnostics
    //     .as_slice()
    //     .iter()
    //     .map(|message| to_lsp_diagnostic(db, message, snapshot.encoding()))
    //     .collect()

    todo!()
}

// fn to_lsp_diagnostic(
//     db: &dyn Db,
//     diagnostic: &dyn ruff_db::diagnostic::Diagnostic,
//     encoding: crate::PositionEncoding,
// ) -> Diagnostic {
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
