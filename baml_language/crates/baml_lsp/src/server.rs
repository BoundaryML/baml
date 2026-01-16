//! Scheduling, I/O, and API endpoints.

// The new PanicInfoHook name requires MSRV >= 1.82
#[allow(deprecated)]
use std::panic::PanicInfo;
use std::{
    num::NonZeroUsize,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use baml_lsp_types::BamlNotification;
use log::info;
use lsp_server::Message;
use lsp_types::{
    ClientCapabilities, CodeLensOptions, CompletionOptions, DiagnosticOptions,
    DiagnosticServerCapabilities, FileSystemWatcher, HoverProviderCapability, InitializeParams,
    MessageType, SaveOptions, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Url, WorkspaceClientCapabilities,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
    notification::DidChangeTextDocument,
};
// TODO: playground_server is disabled for now
// use playground_server::{WebviewCommand, WebviewRouterMessage};
use schedule::Task;
use serde::{Deserialize, Serialize};
use serde_json::json;
use similar::algorithms::NoFinishHook;
use tokio::sync::{RwLock, broadcast};

use self::{
    connection::{Connection, ConnectionInitializer},
    schedule::event_loop_thread,
};
use crate::{
    PositionEncoding,
    baml_project::file_utils::{find_baml_src, find_top_level_parent},
    session::{AllSettings, ClientSettings, Session},
};

pub mod api;
pub mod client;
mod commands;
pub mod connection;
mod schedule;

pub(crate) use connection::ClientSender;

use crate::message::try_show_message;

pub type Result<T> = std::result::Result<T, api::Error>;

pub(crate) struct ServerArgs {
    pub tokio_runtime: tokio::runtime::Runtime,
    // TODO: playground_server is disabled for now, using dummy () types
    pub webview_router_to_websocket_tx: broadcast::Sender<()>,
    pub to_webview_router_rx: broadcast::Receiver<()>,
    pub to_webview_router_tx: broadcast::Sender<()>,
    pub playground_port: u16,
    pub proxy_port: u16,
}

pub(crate) struct Server {
    pub connection: Connection,
    pub client_capabilities: ClientCapabilities,
    pub session: Session,
    pub worker_threads: NonZeroUsize,
    pub args: ServerArgs,
}

#[derive(Serialize, Deserialize)]
struct PortNotificationParams {
    port: u16,
}

impl Server {
    pub fn new(worker_threads: NonZeroUsize, args: ServerArgs) -> anyhow::Result<Self> {
        let connection = ConnectionInitializer::stdio();
        let (id, init_params) = connection.initialize_start()?;

        let client_capabilities = init_params.capabilities.clone();
        let position_encoding = Self::find_best_position_encoding(&client_capabilities);
        let server_capabilities = Self::server_capabilities(position_encoding);

        let connection = connection.initialize_finish(
            id,
            &server_capabilities,
            crate::SERVER_NAME,
            crate::version(),
        )?;
        Self::new_with_connection(worker_threads, connection, init_params, args)
    }

    pub fn new_with_connection(
        worker_threads: NonZeroUsize,
        connection: Connection,
        init_params: InitializeParams,
        args: ServerArgs,
    ) -> anyhow::Result<Self> {
        crate::message::init_messenger(connection.make_sender());

        let client_capabilities = init_params.capabilities.clone();
        let position_encoding = Self::find_best_position_encoding(&client_capabilities);
        // crate::logging::init_logging(crate::logging::LogLevel::Debug, None);

        let init_options = init_params
            .clone()
            .initialization_options
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default()));

        tracing::debug!("--- Received initialization options: {:?}", init_options);

        let AllSettings {
            global_settings,
            mut workspace_settings,
        } = AllSettings::from_value(init_options);

        crate::logging::init_logging(
            global_settings.tracing.log_level.unwrap_or_default(),
            global_settings.tracing.log_file.as_deref(),
        );
        if let Err(e) = tracing_log::LogTracer::init() {
            tracing::warn!("Failed to initialize log tracer: {e}");
            // Decide how to handle this error - maybe log it via tracing if possible,
            // or exit if logging is critical.
        }

        let mut workspace_for_url = |url: Url| {
            let Some(workspace_settings) = workspace_settings.as_mut() else {
                return (url, ClientSettings::default());
            };
            let settings = workspace_settings.remove(&url).unwrap_or_else(|| {
                tracing::warn!("No workspace settings found for {}", url);
                ClientSettings::default()
            });
            (url, settings)
        };
        tracing::info!(
            "--- workspace folders: {:?}",
            init_params.workspace_folders.clone()
        );

        let workspaces = init_params
            .workspace_folders.clone()
            .filter(|folders| !folders.is_empty())
            .map(|folders| folders.into_iter().filter_map(|folder| {
                let baml_src_dir = find_baml_src(&PathBuf::from(folder.uri.path()))?;
                let baml_src_uri = Url::from_file_path(baml_src_dir.to_str()?).ok()?;
                Some(workspace_for_url(baml_src_uri))
            }).collect())
            .or_else(|| {
                tracing::warn!("No workspace(s) were provided during initialization. Using the current working directory as a default workspace...");
                let pwd = std::env::current_dir().ok()?;
                if pwd.ends_with("baml_src") {
                    let url = Url::from_file_path(pwd).expect("PWD should be valid");
                    Some(vec![workspace_for_url(url)])
                } else {
                    let baml_src_dir = find_top_level_parent(&std::env::current_dir().ok()?)?;
                    let uri = Url::from_file_path(baml_src_dir).ok()?;
                    Some(vec![workspace_for_url(uri)])
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Failed to get the current working directory while creating a default workspace.")
            })?;

        tracing::info!("Starting server with {} worker threads", worker_threads);
        tracing::info!("-------- Version: {}", env!("CARGO_PKG_VERSION"));

        let rt = tokio::runtime::Runtime::new()?;

        // Extract client version from initialization parameters
        let client_version = init_params
            .client_info
            .as_ref()
            .and_then(|info| info.version.clone());

        let mut session = Session::new(
            &client_capabilities,
            position_encoding,
            global_settings,
            &workspaces,
            args.playground_port,
            args.to_webview_router_tx.clone(),
            client_version,
        )?;

        let lsp_methods_to_forward_to_webview = session
            .baml_settings
            .lsp_methods_to_forward_to_webview
            .clone();

        let client = client::Client::new(
            connection.make_sender(),
            args.to_webview_router_tx.clone(),
            lsp_methods_to_forward_to_webview.unwrap_or_default(),
        );
        let notifier = client.notifier();

        session.reload(Some(notifier))?;

        let server = Self {
            connection,
            worker_threads,
            session,
            client_capabilities,
            args,
        };

        // TODO: playground_server is disabled for now
        // {
        //     let lsp_sender = server.connection.make_sender();
        //     server.args.tokio_runtime.spawn(async move {
        //         let _ = lsp_sender
        //             .send(Message::Notification(
        //                 BamlNotification::PlaygroundPort {
        //                     port: server.args.playground_port,
        //                 }
        //                 .to_lsp_notification(),
        //             ))
        //             .inspect_err(|e| {
        //                 tracing::error!(
        //                     "Failed to send baml/playground_port notification to IDE: {e}"
        //                 );
        //             });
        //     });
        // }
        // {
        //     // Start the webview router loop
        //     //
        //     // This is the communication bridge between the webview and IDE in non-VSCode environments
        //     // and allows the webview to send messages to Jetbrains and allows Jetbrains to send messages
        //     // to the webview.
        //     //
        //     // webview->IDE is generally backed by the webview POSTing to /webview/SEND_LSP_NOTIFICATION_TO_IDE,
        //     //   and the language server will then forward that to the IDE
        //     // IDE->webview is generally backed by the IDE calling POST /webview/SEND_LSP_NOTIFICATION_TO_WEBVIEW,
        //     //   and the language server will then forward that to the webview
        //     //
        //     // (Note that although the language-server pretends to offer a request-response API, it does not
        //     // block on either the IDE or webview responding before responding to its caller.)
        //     //
        //     // Incoming messages are received via to_webview_router_tx, which the router will then decide to
        //     // dispatch to either the webview (via its websocket) or the IDE (via its LSP connection).
        //     let notifier = client.notifier();
        //     let lsp_sender = server.connection.make_sender();
        //     let mut to_webview_router_rx = server.args.to_webview_router_rx.resubscribe();
        //     let webview_router_to_websocket_tx = server.args.webview_router_to_websocket_tx.clone();
        //     let mut session = server.session.clone();
        //     server.args.tokio_runtime.spawn(async move {
        //         tracing::info!("Starting the webview router loop: will dispatch messages to the webview and IDE");
        //         while let Ok(msg) = to_webview_router_rx.recv().await {
        //             match msg {
        //                 WebviewRouterMessage::WasmIsInitialized => {
        //                     // Reloading the session publishes a runtime_updated notification to the webview
        //                     let _ = session.reload(Some(notifier.clone())).inspect_err(|e| {
        //                         tracing::error!("Failed to reload session: {e}");
        //                     });
        //                 }
        //                 WebviewRouterMessage::GetLanguageServerSettings(sender) => {
        //                     tracing::info!("Received playground GET_LANGUAGE_SERVER_SETTINGS request");
        //                     let _ = sender.send(json!(&session.baml_settings)).inspect_err(|e| {
        //                         tracing::error!("Failed to send GET_LANGUAGE_SERVER_SETTINGS response to WebviewRouter: {e}");
        //                     });
        //                 }
        //                 WebviewRouterMessage::UpdateLanguageServerSettings(unparsed_settings) => {
        //                     tracing::info!("Received playground UPDATE_LANGUAGE_SERVER_SETTINGS request: {:?}", unparsed_settings);
        //                     let _ = session.update_baml_settings(unparsed_settings.clone());
        //                     let _ = notifier
        //                         .notify_raw("baml_settings_updated".to_string(), json!(&session.baml_settings))
        //                         .inspect_err(|e| {
        //                             tracing::error!("Failed to send baml_settings_updated notification to IDE: {e}");
        //                         });
        //                 }
        //                 WebviewRouterMessage::SendLspNotificationToIde (notification) => {
        //                     tracing::info!("Received playground SEND_LSP_NOTIFICATION_TO_IDE request: {:?}", notification);
        //                     let _ = lsp_sender
        //                         .send(Message::Notification(notification))
        //                         .inspect_err(|e| {
        //                             tracing::error!("Failed to forward SEND_LSP_NOTIFICATION_TO_IDE message to IDE: {e}");
        //                         });
        //                 }
        //                 WebviewRouterMessage::SendMessageToWebview(command) => {
        //                     tracing::info!("Received playground SEND_MESSAGE_TO_WEBVIEW request: {:?}", command);
        //                     // Simply forward the WebviewCommand to the websocket - no processing needed
        //                     let _ = webview_router_to_websocket_tx
        //                         .send(command)
        //                         .inspect_err(|e| {
        //                             tracing::error!("Failed to send WebviewCommand to websocket: {e}");
        //                         });
        //                 }
        //             }
        //         }
        //         tracing::info!("Playground rx channel closed");
        //     });
        // }

        Ok(server)
    }

    pub fn run(self) -> anyhow::Result<()> {
        tracing::info!("BAML language server started inside hot reload lorem ipsum");
        // The new PanicInfoHook name requires MSRV >= 1.82
        #[allow(deprecated)]
        type PanicHook = Box<dyn Fn(&PanicInfo<'_>) + 'static + Sync + Send>;
        struct RestorePanicHook {
            hook: Option<PanicHook>,
        }

        impl Drop for RestorePanicHook {
            fn drop(&mut self) {
                if let Some(hook) = self.hook.take() {
                    std::panic::set_hook(hook);
                }
            }
        }

        // unregister any previously registered panic hook
        // The hook will be restored when this function exits.
        let _ = RestorePanicHook {
            hook: Some(std::panic::take_hook()),
        };

        // When we panic, try to notify the client.
        std::panic::set_hook(Box::new(move |panic_info| {
            use std::io::Write;

            let backtrace = std::backtrace::Backtrace::force_capture();
            info!("{panic_info}\n{backtrace}");
            tracing::error!("{panic_info}\n{backtrace}");

            // we also need to print to stderr directly for when using `$logTrace` because
            // the message won't be sent to the client.
            // But don't use `eprintln` because `eprintln` itself may panic if the pipe is broken.
            let mut stderr = std::io::stderr().lock();
            writeln!(stderr, "{panic_info}\n{backtrace}").ok();

            try_show_message(
                "The BAML language server exited with a panic. See the logs for more details."
                    .to_string(),
                MessageType::ERROR,
            )
            .ok();
        }));

        std::thread::spawn(|| {
            const DEADLOCK_WATCHDOG_INTERVAL: Duration = Duration::from_secs(10);
            tracing::info!(
                "Starting deadlock watchdog (will poll every {:?})",
                DEADLOCK_WATCHDOG_INTERVAL
            );
            loop {
                std::thread::sleep(DEADLOCK_WATCHDOG_INTERVAL);
                // NB: this shows deadlocks detected since the _last_ check, not all current deadlocks.
                let cycles = parking_lot::deadlock::check_deadlock();
                if cycles.is_empty() {
                    continue;
                }
                tracing::error!("Detected {} deadlocks since the last check:", cycles.len());
                for (i, threads) in cycles.iter().enumerate() {
                    tracing::error!("Deadlock {} of {}:", i + 1, cycles.len());
                    for t in threads {
                        tracing::error!("  Thread {:?}", t.thread_id());
                        tracing::error!("  Backtrace:\n{:?}", t.backtrace());
                    }
                }
            }
        });

        event_loop_thread(move || {
            Self::event_loop(
                &self.connection,
                &self.client_capabilities,
                self.session,
                self.worker_threads,
                self.args.webview_router_to_websocket_tx,
            )?;
            self.connection.close()?;
            Ok(())
        })?
        .join()
    }

    // Note, we can undo all these changes in here and in scheduler.rs and use what red_knot_server (from ruff) does,
    // which has no debouncer.
    #[allow(clippy::needless_pass_by_value)] // this is because we aren't using `next_request_id` yet.
    fn event_loop(
        connection: &Connection,
        client_capabilities: &ClientCapabilities,
        mut session: Session,
        worker_threads: NonZeroUsize,
        // TODO: playground_server is disabled for now
        _webview_router_to_websocket_tx: broadcast::Sender<()>,
    ) -> anyhow::Result<()> {
        let to_webview_router_tx = session.to_webview_router_tx.clone();
        let lsp_methods_to_forward_to_webview = session
            .baml_settings
            .lsp_methods_to_forward_to_webview
            .clone();

        // Ensure we have a notifier for reload operations
        let client = client::Client::new(
            connection.make_sender(),
            to_webview_router_tx.clone(),
            lsp_methods_to_forward_to_webview
                .clone()
                .unwrap_or_default(),
        );
        let notifier = client.notifier();
        // Make sure the session is properly loaded after initialization
        session.reload(Some(notifier.clone()))?;
        let mut scheduler =
            schedule::Scheduler::new(&mut session, worker_threads, connection.make_sender());
        Self::try_register_capabilities(client_capabilities, &mut scheduler);

        for msg in connection.incoming() {
            // tracing::info!("Received message: {:?}", msg);
            if connection.handle_shutdown(&msg)? {
                break;
            }
            // TODO: playground_server forwarding is disabled for now
            // webview_router_to_websocket_tx.send(LangServerToWasmMessage::LspMessage(msg.clone()))?;
            let tasks = match msg {
                Message::Request(req) => {
                    // TODO: playground_server forwarding is disabled for now
                    // if lsp_methods_to_forward_to_webview
                    //     .clone()
                    //     .unwrap_or_default()
                    //     .contains(&req.method)
                    // {
                    //     let _ = to_webview_router_tx
                    //         .send(WebviewRouterMessage::SendMessageToWebview(
                    //             playground_server::WebviewCommand::LspMessage(
                    //                 lsp_server::Notification::new(
                    //                     req.method.clone(),
                    //                     req.params.clone(),
                    //                 ),
                    //             ),
                    //         ))
                    //         .inspect_err(|e| {
                    //             tracing::error!("Failed to forward LSP request to webview: {e}");
                    //         });
                    // }
                    vec![api::request(req)]
                }
                Message::Notification(notification) => api::notification(notification),
                Message::Response(response) => {
                    tracing::info!("Preparing to send response: {:?}", response);
                    vec![scheduler.response(response)]
                }
            };

            // Dispatch each task in the vector
            for task in tasks {
                scheduler.dispatch(task);
            }
        }

        Ok(())
    }

    fn try_register_capabilities(
        client_capabilities: &ClientCapabilities,
        scheduler: &mut schedule::Scheduler,
    ) {
        let dynamic_registration = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files)
            .and_then(|watched_files| watched_files.dynamic_registration)
            .unwrap_or_default();
        tracing::info!(
            "dynamic_registration ATTEMPT START HELLO AGAIN: {}",
            dynamic_registration
        );
        if dynamic_registration {
            // Register all dynamic capabilities here

            // `workspace/didChangeWatchedFiles`
            // (this registers the configuration file watcher)
            let params = lsp_types::RegistrationParams {
                registrations: vec![
                    lsp_types::Registration {
                        id: "baml-server-file-operations".into(),
                        method: "workspace/didChangeWatchedFiles".into(),
                        register_options: Some(
                            serde_json::to_value(
                                lsp_types::DidChangeWatchedFilesRegistrationOptions {
                                    watchers: vec![FileSystemWatcher {
                                        glob_pattern: lsp_types::GlobPattern::String(
                                            "**/*.{baml}".into(),
                                        ),
                                        kind: None,
                                    }],
                                },
                            )
                            .unwrap(),
                        ),
                    },
                    lsp_types::Registration {
                        id: "baml-server-configuration".into(),
                        method: "workspace/didChangeConfiguration".into(),
                        register_options: None,
                    },
                ],
            };

            let response_handler = |()| {
                tracing::info!("Configuration file watcher successfully registered");
                Task::nothing()
            };

            if let Err(err) = scheduler
                .request::<lsp_types::request::RegisterCapability>(params, response_handler)
            {
                tracing::error!(
                    "An error occurred when trying to register the configuration file watcher: {err}"
                );
            }
        } else {
            tracing::warn!(
                "LSP client does not support dynamic capability registration - automatic configuration reloading will not be available."
            );
        }
        tracing::info!("dynamic_registration ATTEMPT END: {}", dynamic_registration);
    }

    pub fn find_best_position_encoding(
        client_capabilities: &ClientCapabilities,
    ) -> PositionEncoding {
        client_capabilities
            .general
            .as_ref()
            .and_then(|general_capabilities| general_capabilities.position_encodings.as_ref())
            .and_then(|encodings| {
                encodings
                    .iter()
                    .filter_map(|encoding| PositionEncoding::try_from(encoding).ok())
                    .max() // this selects the highest priority position encoding
            })
            .unwrap_or_default()
    }

    pub fn server_capabilities(position_encoding: PositionEncoding) -> ServerCapabilities {
        ServerCapabilities {
            position_encoding: Some(position_encoding.into()),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some(crate::DIAGNOSTIC_NAME.into()),
                ..Default::default()
            })),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec!['@'.to_string(), '"'.to_string(), '.'.to_string()]),
                ..Default::default()
            }),
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: Some(true),
            }),
            code_action_provider: None,
            execute_command_provider: Some(lsp_types::ExecuteCommandOptions {
                commands: vec![api::OPEN_IN_BROWSER_COMMAND.to_string()],
                work_done_progress_options: Default::default(),
            }),
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            references_provider: Some(lsp_types::OneOf::Left(true)),
            document_formatting_provider: None,
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            rename_provider: Some(lsp_types::OneOf::Left(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    will_save: Some(true),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                    ..Default::default()
                },
            )),
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                    supported: Some(true),
                    change_notifications: Some(lsp_types::OneOf::Left(true)),
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
