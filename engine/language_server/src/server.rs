//! Scheduling, I/O, and API endpoints.

use log::info;
use std::num::NonZeroUsize;
// The new PanicInfoHook name requires MSRV >= 1.82
#[allow(deprecated)]
use std::panic::PanicInfo;
use std::path::PathBuf;

use lsp_server::Message;
use lsp_types::{
    ClientCapabilities, CodeLensOptions, CompletionOptions, DiagnosticOptions,
    DiagnosticServerCapabilities, FileSystemWatcher, HoverProviderCapability, InitializeParams,
    MessageType, SaveOptions, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Url,
};
use schedule::Task;

use self::connection::{Connection, ConnectionInitializer};
use self::schedule::event_loop_thread;
use crate::baml_project::file_utils::{find_baml_src, find_top_level_parent};
use crate::session::{AllSettings, ClientSettings, Session};
use crate::PositionEncoding;

pub mod api;
pub mod client;
pub mod connection;
mod schedule;

use crate::message::try_show_message;
pub(crate) use connection::ClientSender;

pub type Result<T> = std::result::Result<T, api::Error>;

pub(crate) struct Server {
    pub connection: Connection,
    pub client_capabilities: ClientCapabilities,
    pub worker_threads: NonZeroUsize,
    pub session: Session,
}

impl Server {
    pub fn new(worker_threads: NonZeroUsize) -> anyhow::Result<Self> {
        tracing::info!("Starting server with {} worker threads", worker_threads);
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
        Self::new_with_connection(worker_threads, connection, init_params)
    }

    pub fn new_with_connection(
        worker_threads: NonZeroUsize,
        connection: Connection,
        init_params: InitializeParams,
    ) -> anyhow::Result<Self> {
        crate::message::init_messenger(connection.make_sender());

        let client_capabilities = init_params.capabilities.clone();
        let position_encoding = Self::find_best_position_encoding(&client_capabilities);

        let AllSettings {
            global_settings,
            mut workspace_settings,
        } = AllSettings::from_value(
            init_params
                .initialization_options
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default())),
        );

        crate::logging::init_logging(
            global_settings.tracing.log_level.unwrap_or_default(),
            global_settings.tracing.log_file.as_deref(),
        );

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

        let workspaces = init_params
            .workspace_folders
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

        if workspaces.len() > 1 {
            // TODO(dhruvmanila): Support multi-root workspaces
            anyhow::bail!("Multi-root workspaces are not supported yet");
        }

        let mut session = Session::new(
            &client_capabilities,
            position_encoding,
            global_settings,
            &workspaces,
        )?;

        // Create a client and notifier to pass to reload
        let client = client::Client::new(connection.make_sender());
        let notifier = client.notifier();

        // Reload the session with the notifier
        session.reload(Some(notifier))?;

        Ok(Self {
            connection,
            worker_threads,
            session,
            client_capabilities,
        })
    }

    pub fn run(self) -> anyhow::Result<()> {
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

        event_loop_thread(move || {
            Self::event_loop(
                &self.connection,
                &self.client_capabilities,
                self.session,
                self.worker_threads,
            )?;
            self.connection.close()?;
            Ok(())
        })?
        .join()
    }

    #[allow(clippy::needless_pass_by_value)] // this is because we aren't using `next_request_id` yet.
    fn event_loop(
        connection: &Connection,
        _client_capabilities: &ClientCapabilities,
        mut session: Session,
        worker_threads: NonZeroUsize,
    ) -> anyhow::Result<()> {
        // Ensure we have a notifier for reload operations
        let client = client::Client::new(connection.make_sender());
        let notifier = client.notifier();

        // Make sure the session is properly loaded after initialization
        session.reload(Some(notifier.clone()))?;

        let mut scheduler =
            schedule::Scheduler::new(&mut session, worker_threads, connection.make_sender());

        Self::try_register_capabilities(&_client_capabilities, &mut scheduler);

        for msg in connection.incoming() {
            if connection.handle_shutdown(&msg)? {
                break;
            }
            let task = match msg {
                Message::Request(req) => api::request(req),
                Message::Notification(notification) => api::notification(notification),
                Message::Response(response) => scheduler.response(response),
            };
            scheduler.dispatch(task);
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
        if dynamic_registration {
            // Register all dynamic capabilities here

            // `workspace/didChangeWatchedFiles`
            // (this registers the configuration file watcher)
            let params = lsp_types::RegistrationParams {
                registrations: vec![lsp_types::Registration {
                    id: "baml-server-file-operations".into(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: Some(
                        serde_json::to_value(lsp_types::DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.{baml}".into()),
                                kind: None,
                            }],
                        })
                        .unwrap(),
                    ),
                }],
            };

            let response_handler = |()| {
                tracing::info!("Configuration file watcher successfully registered");
                Task::nothing()
            };

            if let Err(err) = scheduler
                .request::<lsp_types::request::RegisterCapability>(params, response_handler)
            {
                tracing::error!("An error occurred when trying to register the configuration file watcher: {err}");
            }
        } else {
            tracing::warn!("LSP client does not support dynamic capability registration - automatic configuration reloading will not be available.");
        }
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
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            rename_provider: Some(lsp_types::OneOf::Left(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(true),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                    ..Default::default()
                },
            )),
            ..Default::default()
        }
    }
}

// /// Starting from a root directory, return all baml files in the directory,
// /// searching recursively.
// /// The returned tuples are pairs of filepaths (relative to the project root),
// /// and the full file contents.
// pub fn gather_baml_files(root_dir: &Path) -> Result<Vec<(String, String)>> {
//     let mut files = Vec::new();
//     let empty_path = PathBuf::new();
//     _gather_baml_files(root_dir, empty_path, &mut HashSet::new(), &mut files)?;
//     Ok(files)
// }
//
// /// The recursive body for `gather_baml_files`.
// /// It will be called at each level of the directory structure under the
// /// project root.
// ///
// /// Params:
// ///   root_path: Project directory.
// ///   subdir: Path relative to root_path that we're currently searching.
// ///   visited_dirs: Track every visited subdir to prevent symlink loops.
// ///   files: Accumulated subdirs and their respective file contents.
// fn _gather_baml_files<'a>(
//     root_path: &'a Path,
//     subdir: PathBuf,
//     visited_dirs: &mut HashSet<PathBuf>,
//     files: &'a mut Vec<(String, String)>,
// ) -> Result<()> {
//
//     // Stop recursion if we have seen this directory before.
//     if visited_dirs.contains(&subdir) {
//         return Ok(());
//     }
//
//     visited_dirs.insert(subdir.clone());
//     let absolute = root_path.join(&subdir);
//
//     let dir_entries = fs::read_dir(absolute).map_err(|e| internal_error(e.to_string()))?;
//     for dir_entry in dir_entries.filter_map(|d| d.ok()) {
//
//         if dir_entry.path().is_dir() {
//             let subdir = subdir.join(dir_entry.file_name());
//             _gather_baml_files(root_path, subdir, visited_dirs, files)?;
//
//         } else {
//             if let Some(file_name) = dir_entry.path().file_name().and_then(|os_str| os_str.to_str()) {
//                 if file_name.ends_with(".baml") {
//                     let file_path = subdir.join(file_name);
//                     let contents = fs::read_to_string(dir_entry.path()).map_err(|e| internal_error(e.to_string()))?;
//                     if let Some(relative_path) = file_path.to_str() {
//                         files.push((relative_path.to_string(), contents));
//                     }
//                 }
//             }
//
//         }
//     }
//     Ok(())
// }
//
// #[track_caller]
// fn internal_error(msg: String) -> api::Error {
//     api::Error::new(
//         anyhow::anyhow!(msg),
//         lsp_server::ErrorCode::InternalError,
//     )
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::path::PathBuf;
//
//     #[test]
//     fn test_gather() {
//         let res = gather_baml_files(&PathBuf::from("/Users/greghale/code/baml/integ-tests/baml_src")).unwrap();
//         dbg!(&res);
//         panic!("Here");
//     }
// }
