use crate::{server::schedule::Task, session::Session};
use diagnostics::project_diagnostics;
use log::info;
use lsp_server;
use lsp_types::{
    DocumentDiagnosticReport, DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
    RelatedFullDocumentDiagnosticReport,
};
use serde::Deserialize;
use std::path::PathBuf;
use url::Url;

mod diagnostics;
mod notifications;
mod requests;
mod traits;

use notifications as notification;
use requests as request;

use self::traits::{NotificationHandler, RequestHandler};

use super::{client::Responder, schedule::BackgroundSchedule, Result};

pub(super) fn request<'a>(req: lsp_server::Request) -> Task<'a> {
    let id = req.id.clone();

    match req.method.as_str() {
        // request::CodeActions::METHOD => background_request_task::<request::CodeActions>(
        //     req,
        //     BackgroundSchedule::LatencySensitive,
        // ),
        // request::CodeActionResolve::METHOD => {
        //     background_request_task::<request::CodeActionResolve>(req, BackgroundSchedule::Worker)
        // }
        "bamlCliVersion" => {
            let version = env!("CARGO_PKG_VERSION");
            return Task::local(move |_, _, _, responder| {
                responder
                    .respond(id, Ok(version))
                    .map_err(|err| {
                        tracing::error!("Failed to send response: {err}");
                    })
                    .unwrap_or(())
            });
        }
        request::Completion::METHOD => local_request_task::<request::Completion>(req),
        request::CodeLens::METHOD => local_request_task::<request::CodeLens>(req),
        request::GotoDefinition::METHOD => local_request_task::<request::GotoDefinition>(req),
        request::Rename::METHOD => local_request_task::<request::Rename>(req),
        request::DocumentDiagnosticRequestHandler::METHOD => {
            local_request_task::<request::DocumentDiagnosticRequestHandler>(req)
        }
        "requestDiagnostics" => {
            eprintln!("req: {:?}", req);
            let params = serde_json::from_value::<DiagnosticRequestParams>(req.params)
                .expect("Failed to parse JSON");
            let url = Url::parse(&params.project_id)
                .map_err(|e| {
                    tracing::error!("Failed to parse URL: {e}");
                    e
                })
                .expect("Failed to parse URL");
            return Task::local(move |session, _, _, responder| {
                // let diagnostics_report =
                let project_file = params.project_id;
                session
                    .ensure_project_db_for_baml_file(&url)
                    .expect("Failed to ensure project");
                let project = session
                    .project_db_for_path(url.to_file_path().unwrap())
                    .unwrap();
                let diagnostics = project_diagnostics(&project, Some(&url));

                let report = Ok(DocumentDiagnosticReportResult::Report(
                    DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: diagnostics,
                        },
                    }),
                ));
                responder
                    .respond(id, report)
                    .map_err(|e| {
                        tracing::error!("Failed to send response: {e}");
                    })
                    .unwrap_or(());
            });
        }
        // request::DocumentDiagnosticRequestHandler::METHOD => {
        //     background_request_task::<request::DocumentDiagnosticRequestHandler>(
        //         req,
        //         BackgroundSchedule::LatencySensitive,
        //     )
        // }

        // request::ExecuteCommand::METHOD => local_request_task::<request::ExecuteCommand>(req),
        // request::Format::METHOD => {
        //     background_request_task::<request::Format>(req, BackgroundSchedule::Fmt)
        // }
        // request::FormatRange::METHOD => {
        //     background_request_task::<request::FormatRange>(req, BackgroundSchedule::Fmt)
        // }
        request::DocumentFormatting::METHOD => {
            local_request_task::<request::DocumentFormatting>(req)
        }
        request::Hover::METHOD => local_request_task::<request::Hover>(req),
        method => {
            tracing::warn!("Received request {method} which does not have a handler");
            return Task::nothing();
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing request with ID {id}: {err}");
        show_err_msg!(
            "BAML failed to handle a request from the editor. Check the logs for more details."
        );
        let result: Result<()> = Err(err);
        Task::immediate(id, result)
    })
}

pub(super) fn notification<'a>(notif: lsp_server::Notification) -> Task<'a> {
    match notif.method.as_str() {
        notification::DidChangeTextDocumentHandler::METHOD => {
            local_notification_task::<notification::DidChangeTextDocumentHandler>(notif)
        }
        notification::DidChangeConfiguration::METHOD => {
            local_notification_task::<notification::DidChangeConfiguration>(notif)
        }
        notification::DidChangeWatchedFiles::METHOD => {
            local_notification_task::<notification::DidChangeWatchedFiles>(notif)
        }
        // notification::DidChangeWorkspace::METHOD => {
        //     local_notification_task::<notification::DidChangeWorkspace>(notif)
        // }
        notification::DidCloseTextDocumentHandler::METHOD => local_notification_task::<notification::DidCloseTextDocumentHandler>(notif),
        notification::DidOpenTextDocumentHandler::METHOD => local_notification_task::<notification::DidOpenTextDocumentHandler>(notif),
        notification::DidSaveTextDocument::METHOD => {
            local_notification_task::<notification::DidSaveTextDocument>(notif)
        }
        method => {
            tracing::warn!("Received notification {method} which does not have a handler.");
            return Task::nothing();
        }
    }
    .unwrap_or_else(|err| {
        tracing::error!("Encountered error when routing notification: {err}");
        show_err_msg!("Ruff failed to handle a notification from the editor. Check the logs for more details.");
        Task::nothing()
    })
}

fn local_request_task<'a, R: traits::SyncRequestHandler>(
    req: lsp_server::Request,
) -> super::Result<Task<'a>> {
    let (id, params) = cast_request::<R>(req)?;
    Ok(Task::local(|session, notifier, requester, responder| {
        let result = R::run(session, notifier, requester, params);
        respond::<R>(id, result, &responder);
    }))
}

fn background_request_task<'a, R: traits::BackgroundDocumentRequestHandler>(
    req: lsp_server::Request,
    schedule: BackgroundSchedule,
) -> super::Result<Task<'a>> {
    let (_id, params) = cast_request::<R>(req)?;
    let url = R::document_url(&params).into_owned();
    let path = url
        .clone()
        .to_file_path()
        .internal_error_msg("Could not convert URL to path")?;
    Ok(Task::background(schedule, move |session: &Session| {
        // let Ok(path) = url_to_any_system_path(&url) else {
        //     return Box::new(|_, _| {});
        // };
        // let db = match path {
        //     AnySystemPath::System(path) => match session.project_db_for_path(path.as_std_path()) {
        //         Some(db) => db.clone(),
        //         None => session.default_project_db().clone(),
        //     },
        //     AnySystemPath::SystemVirtual(_) => session.default_project_db().clone(),
        // };

        let Some(_snapshot) = session.take_snapshot(url) else {
            return Box::new(|_, _| {});
        };
        // TODO get the relevant Project and pass it in.
        info!(
            "session.projects.len(): {:?}",
            session.projects_by_workspace_folder.len()
        );
        let _db = session.project_db_for_path(path).clone();

        Box::new(move |_notifier, _responder| {
            // let result = R::run_with_snapshot(snapshot, db, notifier, params);
            // respond::<R>(id, result, &responder);
        })
    }))
}

fn local_notification_task<'a, N: traits::SyncNotificationHandler>(
    notif: lsp_server::Notification,
) -> super::Result<Task<'a>> {
    let (id, params) = cast_notification::<N>(notif)?;
    Ok(Task::local(move |session, notifier, requester, _| {
        if let Err(err) = N::run(session, notifier, requester, params) {
            tracing::error!("An error occurred while running {id}: {err}");
            show_err_msg!("Ruff encountered a problem. Check the logs for more details.");
        }
    }))
}

// #[allow(dead_code)]
// fn background_notification_thread<'a, N: traits::BackgroundDocumentNotificationHandler>(
//     req: lsp_server::Notification,
//     schedule: BackgroundSchedule,
// ) -> super::Result<Task<'a>> {
//     let (_id, params) = cast_notification::<N>(req)?;
//     Ok(Task::background(schedule, move |session: &Session| {
//         let project = session.default_project_db()?;
//         let document_key = DocumentKey::from_path(project.root_path(), params)?;
//         // TODO(jane): we should log an error if we can't take a snapshot.
//         let Some(_snapshot) = session.take_snapshot(document_key) else {
//             return Box::new(|_, _| {});
//         };
//         Box::new(move |_notifier, _| {
//             // if let Err(err) = N::run_with_snapshot(snapshot, notifier, params) {
//             //     tracing::error!("An error occurred while running {id}: {err}");
//             //     show_err_msg!("Ruff encountered a problem. Check the logs for more details.");
//             // }
//         })
//     }))
// }

#[derive(Deserialize)]
struct DiagnosticRequestParams {
    #[serde(rename = "projectId")]
    project_id: String,
}

/// Tries to cast a serialized request from the server into
/// a parameter type for a specific request handler.
/// It is *highly* recommended to not override this function in your
/// implementation.
fn cast_request<Req>(
    request: lsp_server::Request,
) -> super::Result<(
    lsp_server::RequestId,
    <<Req as RequestHandler>::RequestType as lsp_types::request::Request>::Params,
)>
where
    Req: traits::RequestHandler,
{
    request.clone()
        .extract(Req::METHOD)
        .map_err(|ref err| match &err {
            json_err @ lsp_server::ExtractError::JsonError { .. } => {
                anyhow::anyhow!("JSON parsing failure:\n{json_err}")
            }
            lsp_server::ExtractError::MethodMismatch(e) => {
                eprintln!("req: {:?}", request.clone());
                eprintln!("Req::METHOD: {:?}", Req::METHOD.clone());
                eprintln!("ExtractError: {:?}", e.clone());
                unreachable!("A method mismatch should not be possible here unless you've used a different handler (`Req`) \
                    than the one whose method name was matched against earlier.")
            }
        })
        .with_failure_code(lsp_server::ErrorCode::InternalError)
}

/// Sends back a response to the server using a [`Responder`].
fn respond<Req>(
    id: lsp_server::RequestId,
    result: crate::server::Result<
        <<Req as traits::RequestHandler>::RequestType as lsp_types::request::Request>::Result,
    >,
    responder: &Responder,
) where
    Req: traits::RequestHandler,
{
    if let Err(err) = &result {
        tracing::error!("An error occurred with result ID {id}: {err}");
        show_err_msg!("Ruff encountered a problem. Check the logs for more details.");
    }
    if let Err(err) = responder.respond(id, result) {
        tracing::error!("Failed to send response: {err}");
    }
}

/// Tries to cast a serialized request from the server into
/// a parameter type for a specific request handler.
fn cast_notification<N>(
    notification: lsp_server::Notification,
) -> super::Result<
    (
        &'static str,
        <<N as traits::NotificationHandler>::NotificationType as lsp_types::notification::Notification>::Params,
)> where N: traits::NotificationHandler{
    Ok((
        N::METHOD,
        notification
            .extract(N::METHOD)
            .map_err(|err| match err {
                json_err @ lsp_server::ExtractError::JsonError { .. } => {
                    anyhow::anyhow!("JSON parsing failure:\n{json_err}")
                }
                lsp_server::ExtractError::MethodMismatch(_) => {
                    unreachable!("A method mismatch should not be possible here unless you've used a different handler (`N`) \
                        than the one whose method name was matched against earlier.")
                }
            })
            .with_failure_code(lsp_server::ErrorCode::InternalError)?,
    ))
}

pub struct Error {
    pub code: lsp_server::ErrorCode,
    pub error: anyhow::Error,
}

/// A trait to convert result types into the server result type, [`super::Result`].
trait LSPResult<T> {
    fn with_failure_code(self, code: lsp_server::ErrorCode) -> super::Result<T>;
}

impl<T, E: Into<anyhow::Error>> LSPResult<T> for core::result::Result<T, E> {
    fn with_failure_code(self, code: lsp_server::ErrorCode) -> super::Result<T> {
        self.map_err(|err| Error::new(err.into(), code))
    }
}

impl Error {
    pub(crate) fn new(err: anyhow::Error, code: lsp_server::ErrorCode) -> Self {
        Self { code, error: err }
    }
}

// Right now, we treat the error code as invisible data that won't
// be printed.
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

// TODO: Probably redundant with LSPResult.
trait ResultExt<T> {
    fn internal_error(self) -> Result<T>;
    fn internal_error_msg(self, msg: &str) -> Result<T>;
}

impl<T> ResultExt<T> for anyhow::Result<T> {
    fn internal_error(self) -> Result<T> {
        self.map_err(|e| Error {
            error: e,
            code: lsp_server::ErrorCode::InternalError,
        })
    }

    fn internal_error_msg(self, msg: &str) -> Result<T> {
        self.map_err(|e| Error {
            error: anyhow::anyhow!("{msg}: {e}"),
            code: lsp_server::ErrorCode::InternalError,
        })
    }
}

impl<T> ResultExt<T> for std::result::Result<T, ()> {
    fn internal_error(self) -> Result<T> {
        self.map_err(|()| Error {
            error: anyhow::anyhow!("Unknown error"),
            code: lsp_server::ErrorCode::InternalError,
        })
    }

    fn internal_error_msg(self, msg: &str) -> Result<T> {
        self.map_err(|()| Error {
            error: anyhow::anyhow!(anyhow::anyhow!("{}", msg)),
            code: lsp_server::ErrorCode::InternalError,
        })
    }
}
