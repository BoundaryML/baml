use super::LspError;

macro_rules! lsp_request {
    ($name:tt) => {
        <lsp_types::lsp_request!($name) as lsp_types::request::Request>::METHOD
    };
}

macro_rules! lsp_request_extract {
    ($notif:ident, $name:tt) => {
        $notif
            .extract(lsp_request!($name))
            .map_err(LspError::RequestExtractError)
    };
}

macro_rules! lsp_request_params {
    ($name:tt) => {
        <lsp_types::lsp_request!($name) as lsp_types::request::Request>::Params
    };
}

macro_rules! lsp_request_result {
    ($name:tt) => {
        <lsp_types::lsp_request!($name) as lsp_types::request::Request>::Result
    };
}

/// This is defined for the [`lsp_types::request::Request`] trait
macro_rules! define_lsp_request_trait {
    ( $( $lsp_method:tt => $fn_name:ident ),* $(,)? ) => {
        paste::paste! {
            pub trait BexLspRequest {
                fn handle_request(&self, notif: lsp_server::Request) {
                    let sender = self.request_sender();
                    let id = notif.id.clone();
                    match notif.method.as_str() {
                        $(
                            lsp_request!($lsp_method) => {
                                let (id, params) = match lsp_request_extract!(notif, $lsp_method) {
                                    Ok(extracted) => extracted,
                                    Err(err) => {
                                        let _ = sender(id, Err(err));
                                        return;
                                    }
                                };
                                let result = match self.[<on_request_ $fn_name>](params) {
                                    Ok(result) => serde_json::to_value(result).map_err(LspError::RequestSerializeError),
                                    Err(err) => Err(err),
                                };
                                let _ = sender(id, result);
                            }
                        ),*,
                        other => {
                            let _ = sender(notif.id, Err(LspError::UnknownErrorCode(format!("request not supported: {}", other))));
                            return;
                        }
                    }
                }

                /// Return a sender closure used by all `send_notification_*` methods.
                fn request_sender(&self) -> Box<dyn Fn(lsp_server::RequestId, Result<serde_json::Value, LspError>) -> Result<(), LspError> + '_>;


                $(
                    fn [<on_request_ $fn_name>](
                        &self,
                        _params: lsp_request_params!($lsp_method),
                    ) -> Result<lsp_request_result!($lsp_method), LspError> {
                        Err(LspError::RequestNotSupported(format!("request not supported: {}", $lsp_method)))
                    }
                )*
            }
        }
    };
}

define_lsp_request_trait! {
    "initialize" => initialize,
    "shutdown" => shutdown,

    "window/showDocument" => window_show_document,
    "window/showMessageRequest" => window_show_message_request,
    "window/workDoneProgress/create" => window_work_done_progress_create,

    "client/registerCapability" => client_register_capability,
    "client/unregisterCapability" => client_unregister_capability,

    "textDocument/willSaveWaitUntil" => text_document_will_save_wait_until,
    "textDocument/completion" => text_document_completion,
    "textDocument/hover" => text_document_hover,
    "textDocument/signatureHelp" => text_document_signature_help,
    "textDocument/declaration" => text_document_declaration,
    "textDocument/definition" => text_document_definition,
    "textDocument/references" => text_document_references,
    "textDocument/documentHighlight" => text_document_document_highlight,
    "textDocument/documentSymbol" => text_document_document_symbol,
    "textDocument/codeAction" => text_document_code_action,
    "textDocument/codeLens" => text_document_code_lens,
    "textDocument/documentLink" => text_document_document_link,
    "textDocument/rangeFormatting" => text_document_range_formatting,
    "textDocument/onTypeFormatting" => text_document_on_type_formatting,
    "textDocument/formatting" => text_document_formatting,
    "textDocument/rename" => text_document_rename,
    "textDocument/documentColor" => text_document_document_color,
    "textDocument/colorPresentation" => text_document_color_presentation,
    "textDocument/foldingRange" => text_document_folding_range,
    "textDocument/prepareRename" => text_document_prepare_rename,
    "textDocument/implementation" => text_document_implementation,
    "textDocument/selectionRange" => text_document_selection_range,
    "textDocument/typeDefinition" => text_document_type_definition,
    "textDocument/moniker" => text_document_moniker,
    "textDocument/linkedEditingRange" => text_document_linked_editing_range,
    "textDocument/prepareCallHierarchy" => text_document_prepare_call_hierarchy,
    "textDocument/prepareTypeHierarchy" => text_document_prepare_type_hierarchy,
    "textDocument/semanticTokens/full" => text_document_semantic_tokens_full,
    "textDocument/semanticTokens/full/delta" => text_document_semantic_tokens_full_delta,
    "textDocument/semanticTokens/range" => text_document_semantic_tokens_range,
    "textDocument/inlayHint" => text_document_inlay_hint,
    "textDocument/inlineValue" => text_document_inline_value,
    "textDocument/diagnostic" => text_document_diagnostic,

    "workspace/applyEdit" => workspace_apply_edit,
    "workspace/symbol" => workspace_symbol,
    "workspace/executeCommand" => workspace_execute_command,
    "workspace/configuration" => workspace_configuration,
    "workspace/diagnostic" => workspace_diagnostic,
    "workspace/diagnostic/refresh" => workspace_diagnostic_refresh,
    "workspace/willCreateFiles" => workspace_will_create_files,
    "workspace/willRenameFiles" => workspace_will_rename_files,
    "workspace/willDeleteFiles" => workspace_will_delete_files,
    "workspace/workspaceFolders" => workspace_workspace_folders,
    "workspace/semanticTokens/refresh" => workspace_semantic_tokens_refresh,
    "workspace/codeLens/refresh" => workspace_code_lens_refresh,
    "workspace/inlayHint/refresh" => workspace_inlay_hint_refresh,
    "workspace/inlineValue/refresh" => workspace_inline_value_refresh,

    "callHierarchy/incomingCalls" => call_hierarchy_incoming_calls,
    "callHierarchy/outgoingCalls" => call_hierarchy_outgoing_calls,
    "codeAction/resolve" => code_action_resolve,
    "codeLens/resolve" => code_lens_resolve,
    "completionItem/resolve" => completion_item_resolve,
    "documentLink/resolve" => document_link_resolve,
    "inlayHint/resolve" => inlay_hint_resolve,
    "typeHierarchy/subtypes" => type_hierarchy_subtypes,
    "typeHierarchy/supertypes" => type_hierarchy_supertypes,
    "workspaceSymbol/resolve" => workspace_symbol_resolve,
}
