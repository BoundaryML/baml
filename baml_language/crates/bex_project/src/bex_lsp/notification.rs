use super::LspError;

macro_rules! lsp_notification {
    ($name:tt) => {
        <lsp_types::lsp_notification!($name) as lsp_types::notification::Notification>::METHOD
    };
}

macro_rules! lsp_notification_extract {
    ($notif:ident, $name:tt) => {
        $notif
            .extract(lsp_notification!($name))
            .map_err(LspError::NotificationExtractError)
    };
}

macro_rules! lsp_notification_params {
    ($name:tt) => {
        <lsp_types::lsp_notification!($name) as lsp_types::notification::Notification>::Params
    };
}

/// This is defined for the [`lsp_types::notification::Notification`] trait
macro_rules! define_lsp_notification_trait {
    ( $( $lsp_method:tt => $fn_name:ident ),* $(,)? ) => {
        paste::paste! {
            pub trait BexLspNotification {
                /// Dispatch an incoming notification to the appropriate handler method.
                fn handle_notification(&self, notif: lsp_server::Notification) {
                    let is_log_message = notif.method.as_str() == "window/logMessage";
                    let result = match notif.method.as_str() {
                        $(
                            lsp_notification!($lsp_method) => {
                                match lsp_notification_extract!(notif, $lsp_method) {
                                    Ok(args) => self.[<on_notification_ $fn_name>](args),
                                    Err(err) => Err(err),
                                }
                            }
                        ),*,
                        other => Err(LspError::NotificationNotSupported(other.to_string())),
                    };
                    match result {
                        Ok(()) => (),
                        Err(err) => {
                            if !is_log_message {
                                let _ = self.send_notification_log_message(lsp_types::LogMessageParams {
                                        typ: lsp_types::MessageType::ERROR,
                                        message: err.to_string(),
                                    });
                            }
                        }
                    }
                }

                /// Return a sender closure used by all `send_notification_*` methods.
                fn notification_sender(&self) -> Box<dyn Fn(lsp_server::Notification) -> Result<(), LspError> + '_>;

                $(
                    /// Handler for an incoming `$lsp_method` notification.
                    fn [<on_notification_ $fn_name>](
                        &self,
                        _params: lsp_notification_params!($lsp_method),
                    ) -> Result<(), LspError> {
                        Err(LspError::NotificationNotSupported(
                            format!("Notification not supported: {}", lsp_notification!($lsp_method))
                        ))
                    }

                    /// Build and send a `$lsp_method` notification via `get_sender`.
                    fn [<send_notification_ $fn_name>](
                        &self,
                        params: lsp_notification_params!($lsp_method),
                    ) -> Result<(), LspError> {
                        let notif = lsp_server::Notification::new(
                            lsp_notification!($lsp_method).to_string(),
                            params,
                        );
                        (self.notification_sender())(notif)
                    }
                )*
            }
        }
    };
}

define_lsp_notification_trait! {
    "$/cancelRequest"                     => cancel_request,
    "$/setTrace"                          => set_trace,
    "initialized"                         => initialized,
    "exit"                                => exit,
    "window/showMessage"                  => show_message,
    "window/logMessage"                   => log_message,
    "window/workDoneProgress/cancel"      => work_done_progress_cancel,
    "telemetry/event"                     => telemetry_event,
    "textDocument/didOpen"                => did_open,
    "textDocument/didChange"              => did_change,
    "textDocument/willSave"               => will_save,
    "textDocument/didSave"                => did_save,
    "textDocument/didClose"               => did_close,
    "textDocument/publishDiagnostics"     => publish_diagnostics,
    "workspace/didChangeConfiguration"    => did_change_configuration,
    "workspace/didChangeWatchedFiles"     => did_change_watched_files,
    "workspace/didChangeWorkspaceFolders" => did_change_workspace_folders,
    "workspace/didCreateFiles"            => did_create_files,
    "workspace/didRenameFiles"            => did_rename_files,
    "workspace/didDeleteFiles"            => did_delete_files,
}
