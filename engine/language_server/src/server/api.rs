use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use diagnostics::{file_diagnostics, project_diagnostics};
use log::info;
use lsp_server;
use lsp_types::{
    DidChangeTextDocumentParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{server::schedule::Task, session::Session};

mod diagnostics;
pub(crate) mod notifications;
mod requests;
mod traits;

use notifications as notification;
use requests as request;

use self::traits::{
    BackgroundDocumentNotificationHandler, NotificationHandler, RequestHandler,
    SyncNotificationHandler,
};
use super::{
    client::{Notifier, Requester, Responder},
    schedule::BackgroundSchedule,
    Result,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct BamlFunctionSpan {
    file_path: String,
    start: usize,
    end: usize,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct BamlFunctionResult {
    name: String,
    span: BamlFunctionSpan,
}

struct BamlFunctionArg {}
// --- Add debounce duration constant ---
const DID_CHANGE_DEBOUNCE_DURATION: Duration = Duration::from_millis(250);

pub(super) fn request<'a>(req: lsp_server::Request) -> Task<'a> {
    let id = req.id.clone();

    match req.method.as_str() {
        request::CodeActionHandler::METHOD => local_request_task::<request::CodeActionHandler>(req),
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
        request::CodeLensResolve::METHOD => local_request_task::<request::CodeLensResolve>(req),
        request::GotoDefinition::METHOD => local_request_task::<request::GotoDefinition>(req),
        request::Rename::METHOD => local_request_task::<request::Rename>(req),
        request::DocumentDiagnosticRequestHandler::METHOD => {
            // tracing::info!("diagnostic notif");
            local_request_task::<request::DocumentDiagnosticRequestHandler>(req)
            // note background request task here sometimes results in inconsistent baml project state...
        }
        "getBAMLFunctions" => {
            // tracing::info!("getBAMLFunctions");
            return Task::local(move |session, _notifier, requester, responder| {
                let result: anyhow::Result<(serde_json::Value,)> = {
                    let mut all_functions = Vec::new();
                    let projects = session.baml_src_projects.lock();
                    let default_flags = vec!["beta".to_string()];
                    let effective_flags = session
                        .baml_settings
                        .feature_flags
                        .as_ref()
                        .unwrap_or(&default_flags);

                    for (_, project) in projects.iter() {
                        let functions = project
                            .lock()
                            .baml_project
                            .list_functions(effective_flags)
                            .iter()
                            .map(|f| BamlFunctionResult {
                                name: f.name.clone(),
                                span: BamlFunctionSpan {
                                    file_path: f.span.file_path.clone(),
                                    start: f.span.start,
                                    end: f.span.end,
                                },
                            })
                            .collect::<Vec<BamlFunctionResult>>();

                        all_functions.extend(functions);
                    }

                    let result = serde_json::to_value(all_functions);
                    if let Ok(result) = result {
                        Ok((result,))
                    } else {
                        Err(anyhow::anyhow!(
                            "Failed to serialize functions: {:?}",
                            result
                        ))
                    }
                };
                if let Ok((result,)) = result {
                    responder.respond(id, Ok(result)).unwrap();
                } else {
                    // no action
                    // responder.respond(id, Err(result.unwrap_err())).unwrap();
                }
            });
        }
        "requestDiagnostics" => {
            // tracing::info!("---- requestDiagnostics");
            return Task::local(move |session, notifier, _requester, responder| {
                let result: anyhow::Result<()> = (|| {
                    // tracing::info!("requestDiagnostics: {:?}", req.params);

                    let params = serde_json::from_value::<DiagnosticRequestParams>(req.params)
                        .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {e}"))?;
                    let url = Url::parse(&params.project_id)
                        .map_err(|e| anyhow::anyhow!("Failed to parse URL: {e}"))?;
                    if !url.to_string().contains("baml_src") {
                        return Ok(());
                    }

                    let project = session
                        .get_or_create_project(url.to_file_path().unwrap())
                        .expect("Already checked for project's existence");
                    {
                        let default_flags = vec!["beta".to_string()];
                        project.lock().update_runtime(
                            Some(notifier),
                            session
                                .baml_settings
                                .feature_flags
                                .as_ref()
                                .unwrap_or(&default_flags),
                        )?
                    };

                    // TODO: I think we need to send ALL diagnostics for the project. Not sure how this report is different vs sending a signle diagnostic param message
                    let default_flags = vec!["beta".to_string()];
                    let diagnostics = file_diagnostics(
                        project.clone(),
                        &url,
                        session
                            .baml_settings
                            .feature_flags
                            .as_ref()
                            .unwrap_or(&default_flags),
                    );
                    // tracing::info!("---- diagnostics Returned: ");
                    let report = Ok(DocumentDiagnosticReportResult::Report(
                        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                            related_documents: None,
                            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                                result_id: None,
                                items: diagnostics,
                            },
                        }),
                    ));
                    responder.respond(id, report)?;
                    Ok(())
                })();
                result.unwrap_or_else(|e| {
                    tracing::error!("Failed to send response: {e}");
                })
            });
        }
        request::ExecuteCommand::METHOD => local_request_task::<request::ExecuteCommand>(req),
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
        _method => {
            // tracing::warn!("Received request {method} which does not have a handler");
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

// Helper function to handle errors consistently when Result is involved
fn handle_notification_result_error<N: traits::NotificationHandler>(
    result: super::Result<Vec<Task<'static>>>,
) -> Vec<Task<'static>> {
    match result {
        Ok(tasks) => tasks,
        Err(err) => {
            tracing::error!(
                "Encountered error when creating task for notification {}: {err}",
                N::METHOD
            );
            show_err_msg!("BAML failed to handle a notification from the editor. Check the logs.");
            vec![Task::nothing()]
        }
    }
}

pub(super) fn notification<'a>(notif: lsp_server::Notification) -> Vec<Task<'a>> {
    match notif.method.as_str() {
        notification::DidChangeTextDocumentHandler::METHOD => {
            handle_notification_result_error::<notification::DidChangeTextDocumentHandler>(
                local_notification_task::<notification::DidChangeTextDocumentHandler>(notif),
            )
        }

        // --- Use local_notification_task helper for these ---
        notification::DidChangeWatchedFiles::METHOD => {
            handle_notification_result_error::<notification::DidChangeWatchedFiles>(
                local_notification_task::<notification::DidChangeWatchedFiles>(notif),
            )
        }
        notification::DidChangeConfiguration::METHOD => {
            handle_notification_result_error::<notification::DidChangeConfiguration>(
                local_notification_task::<notification::DidChangeConfiguration>(notif),
            )
        }
        notification::DidCloseTextDocumentHandler::METHOD => {
            handle_notification_result_error::<notification::DidCloseTextDocumentHandler>(
                local_notification_task::<notification::DidCloseTextDocumentHandler>(notif),
            )
        }
        notification::DidOpenTextDocumentHandler::METHOD => {
            handle_notification_result_error::<notification::DidOpenTextDocumentHandler>(
                local_notification_task::<notification::DidOpenTextDocumentHandler>(notif),
            )
        }
        // --- DidSaveTextDocument now uses the simple local task helper ---
        notification::DidSaveTextDocument::METHOD => {
            // tracing::info!("Did save text document---------");
            handle_notification_result_error::<notification::DidSaveTextDocument>(
                // Do not use background notifs yet, as baml_client may not have an updated view of the project files
                // See the did_save_text_document.rs file for more details
                // background_notification_task::<notification::DidSaveTextDocument>(
                //     notif,
                //     BackgroundSchedule::LatencySensitive,
                // ),
                local_notification_task::<notification::DidSaveTextDocument>(notif),
            )
        }

        method => {
            tracing::warn!("Received notification {method} which does not have a handler.");
            vec![Task::nothing()]
        }
    }
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
        let Some(_snapshot) = session.take_snapshot(url) else {
            return Box::new(|_, _| {});
        };
        // info!(
        //     "session.projects.len(): {:?}",
        //     session.baml_src_projects.lock().len()
        // );
        let _db = session.get_or_create_project(&path).clone();
        if _db.is_none() {
            tracing::error!("Could not find project for path");
            return Box::new(|_, _| {});
        }
        let _db = _db.unwrap();

        Box::new(move |_notifier, _responder| {
            let _ = R::run_with_snapshot(_snapshot, _db, _notifier, params);
        })
    }))
}

fn local_notification_task<'a, N: traits::SyncNotificationHandler>(
    notif: lsp_server::Notification,
) -> super::Result<Vec<Task<'a>>> {
    let (id, params) = cast_notification::<N>(notif)?;
    Ok(vec![Task::local(move |session, notifier, requester, _| {
        if let Err(err) = N::run(session, notifier, requester, params) {
            tracing::error!("An error occurred while running sync notification {id}: {err}");
            show_err_msg!("BAML encountered a problem handling a notification. Check the logs.");
        }
    })])
}

fn background_notification_task<'a, N: traits::BackgroundDocumentNotificationHandler>(
    notif: lsp_server::Notification,
    schedule: BackgroundSchedule,
) -> super::Result<Vec<Task<'a>>> {
    let (id, params) = cast_notification::<N>(notif)?;
    let url = N::document_url(&params).into_owned();

    Ok(vec![Task::background(
        schedule,
        move |session: &Session| {
            let Some(snapshot) = session.take_snapshot(url.clone()) else {
                tracing::warn!(
                    "Could not take snapshot for background notification {id}: {}",
                    url
                );
                return Box::new(|_, _| {});
            };

            Box::new(move |notifier, _| {
                if let Err(err) = N::run_with_snapshot(snapshot, notifier, params) {
                    tracing::error!(
                        "An error occurred while running background notification {id}: {err}"
                    );
                    show_err_msg!("BAML encountered a background problem. Check the logs.");
                }
            })
        },
    )])
}

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
            lsp_server::ExtractError::MethodMismatch(_e) => {
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
        show_err_msg!("BAML encountered a problem. Check the logs for more details.");
    }
    if let Err(err) = responder.respond(id, result) {
        tracing::error!("Failed to send response: {err}");
    }
}

/// Tries to cast a serialized request from the server into
/// a parameter type for a specific request handler.
pub fn cast_notification<N>(
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
