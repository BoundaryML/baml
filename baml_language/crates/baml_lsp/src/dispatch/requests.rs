//! Request handlers, in the two shapes the tables in [`super`] expect.

use std::path::PathBuf;

use lsp_types::{
    InitializeParams, InitializeResult, SaveOptions, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};

use crate::{
    error::LspError,
    paths,
    position_codec::{PositionCodec, PositionEncoding},
    snapshot::Snapshot,
    state::{GlobalState, SessionKey, SessionLifecycle},
};

/// What this server can do, for the `initialize` handshake. `encoding` is
/// the session's negotiated position encoding; advertising it is mandatory
/// whenever the client offered `positionEncodings`.
///
/// Diagnostics are push-only (`publishDiagnostics`); the pull provider is
/// deliberately absent so editors never show each diagnostic twice.
pub fn server_capabilities(encoding: PositionEncoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(encoding.to_lsp_kind()),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
            },
        )),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(lsp_types::OneOf::Left(true)),
            }),
            file_operations: None,
        }),
        diagnostic_provider: None,
        code_lens_provider: None,
        ..ServerCapabilities::default()
    }
}

pub fn initialize_result(encoding: PositionEncoding) -> InitializeResult {
    InitializeResult {
        capabilities: server_capabilities(encoding),
        server_info: Some(ServerInfo {
            name: "baml-lsp".to_owned(),
            version: Some(baml_version::CANONICAL_VERSION.to_owned()),
        }),
    }
}

/// The `initializationOptions` this server reads.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializationOptions {
    #[serde(default)]
    baml_client: BamlClientOptions,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BamlClientOptions {
    /// A materialized copy of the stdlib stubs; wins over the host's
    /// environment/toolchain default when present.
    #[serde(default)]
    stdlib_dir: Option<PathBuf>,
}

// ── Owner-inline ─────────────────────────────────────────────────────────

pub(super) fn initialize(
    state: &mut GlobalState,
    session: SessionKey,
    params: InitializeParams,
) -> Result<InitializeResult, LspError> {
    let encoding = PositionEncoding::negotiate(
        params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_deref()),
    );
    let snippet_support = params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.completion.as_ref())
        .and_then(|completion| completion.completion_item.as_ref())
        .and_then(|item| item.snippet_support)
        .unwrap_or(false);

    let mut workspace_folders: Vec<PathBuf> = params
        .workspace_folders
        .iter()
        .flatten()
        .filter_map(|folder| paths::canonical_document_path(state.roots(), &folder.uri).ok())
        .collect();
    #[expect(
        deprecated,
        reason = "rootUri is the fallback for clients without workspace folders"
    )]
    if workspace_folders.is_empty()
        && let Some(root_uri) = &params.root_uri
        && let Ok(path) = paths::canonical_document_path(state.roots(), root_uri)
    {
        workspace_folders.push(path);
    }

    if let Some(options) = params.initialization_options {
        match serde_json::from_value::<InitializationOptions>(options) {
            Ok(options) => {
                if let Some(stdlib_dir) = options.baml_client.stdlib_dir {
                    state.set_stdlib_dir(Some(stdlib_dir));
                }
            }
            Err(error) => tracing::warn!(%error, "ignoring malformed initializationOptions"),
        }
    }

    tracing::info!(
        ?workspace_folders,
        ?encoding,
        snippet_support,
        "session initialized"
    );
    let session_state = state.session_mut(session)?;
    session_state.encoding = Some(encoding);
    session_state.snippet_support = snippet_support;
    session_state.workspace_folders = workspace_folders;
    session_state.lifecycle = SessionLifecycle::Initialized;
    Ok(initialize_result(encoding))
}

/// The database is kept: a browser reload re-initializes against the same
/// state, and the host tears the process down on `exit`.
pub(super) fn shutdown(
    state: &mut GlobalState,
    session: SessionKey,
    (): (),
) -> Result<(), LspError> {
    state.session_mut(session)?.lifecycle = SessionLifecycle::ShuttingDown;
    Ok(())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "dispatch-table signature: handlers own their params"
)]
pub(super) fn execute_command(
    _state: &mut GlobalState,
    _session: SessionKey,
    params: lsp_types::ExecuteCommandParams,
) -> Result<Option<serde_json::Value>, LspError> {
    Err(LspError::RequestNotSupported(format!(
        "workspace/executeCommand ({})",
        params.command
    )))
}

/// Code lenses are produced fully resolved.
#[expect(
    clippy::unnecessary_wraps,
    reason = "dispatch-table signature: every handler is fallible"
)]
pub(super) fn code_lens_resolve(
    _state: &mut GlobalState,
    _session: SessionKey,
    lens: lsp_types::CodeLens,
) -> Result<lsp_types::CodeLens, LspError> {
    Ok(lens)
}

// ── Snapshot ─────────────────────────────────────────────────────────────

/// Whole-document formatting. A file that does not parse yields no edits
/// (the diagnostics say why); an unchanged file yields no edits.
#[expect(
    clippy::needless_pass_by_value,
    reason = "dispatch-table signature: handlers own their params"
)]
pub(super) fn formatting(
    snap: &Snapshot,
    params: lsp_types::DocumentFormattingParams,
) -> Result<Option<Vec<lsp_types::TextEdit>>, LspError> {
    let path = paths::canonical_document_path(snap.roots(), &params.text_document.uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    let text = file.text(db);
    let formatted = match baml_fmt::format_salsa(db, file, baml_fmt::FormatOptions::default()) {
        Ok(formatted) => formatted,
        Err(baml_fmt::FormatterError::ParseErrors(_)) => return Ok(None),
        Err(baml_fmt::FormatterError::StrongAstError(error)) => {
            return Err(LspError::RequestFailed(format!(
                "cannot format: {}",
                error.print_with_file_context(&path, text)
            )));
        }
    };
    if formatted == *text {
        return Ok(None);
    }
    // Replace the whole document: [0:0, document end) in the session's
    // encoding, so CRLF and non-ASCII last lines are measured correctly.
    let codec = PositionCodec::new(text, snap.cx().encoding);
    Ok(Some(vec![lsp_types::TextEdit {
        range: lsp_types::Range {
            start: lsp_types::Position::default(),
            end: codec.document_end(),
        },
        new_text: formatted,
    }]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_advertise_the_negotiated_encoding() {
        assert_eq!(
            server_capabilities(PositionEncoding::UTF8).position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF8)
        );
        assert_eq!(
            server_capabilities(PositionEncoding::UTF16).position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF16)
        );
    }

    #[test]
    fn capabilities_are_full_sync_push_diagnostics_only() {
        let capabilities = server_capabilities(PositionEncoding::UTF16);
        let Some(TextDocumentSyncCapability::Options(sync)) = capabilities.text_document_sync
        else {
            panic!("text sync options expected");
        };
        assert_eq!(sync.change, Some(TextDocumentSyncKind::FULL));
        assert_eq!(sync.will_save, Some(false));
        assert!(capabilities.diagnostic_provider.is_none());
        assert!(capabilities.code_lens_provider.is_none());
        assert!(capabilities.execute_command_provider.is_none());
    }

    #[test]
    fn initialization_options_are_lenient() {
        let options: InitializationOptions =
            serde_json::from_value(serde_json::json!({ "unrelated": 1 })).unwrap();
        assert!(options.baml_client.stdlib_dir.is_none());
        let options: InitializationOptions = serde_json::from_value(
            serde_json::json!({ "bamlClient": { "stdlibDir": "/toolchain/stdlib" } }),
        )
        .unwrap();
        assert_eq!(
            options.baml_client.stdlib_dir.as_deref(),
            Some(std::path::Path::new("/toolchain/stdlib"))
        );
    }
}
