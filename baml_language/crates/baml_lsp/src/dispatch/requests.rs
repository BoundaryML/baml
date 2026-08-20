//! Request handlers, in the two shapes the tables in [`super`] expect.

use std::{path::PathBuf, sync::Arc};

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
/// `open_panel` is whether the host installed
/// [`GlobalState::set_open_panel_handler`]. Code lenses and
/// [`OPEN_PANEL_COMMAND`] are advertised together and only then: a lens is a
/// button for that command, so a host that cannot run it must not show one.
///
/// Diagnostics are push-only (`publishDiagnostics`); the pull provider is
/// deliberately absent so editors never show each diagnostic twice.
pub fn server_capabilities(encoding: PositionEncoding, open_panel: bool) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(encoding.to_lsp_kind()),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
            },
        )),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        references_provider: Some(lsp_types::OneOf::Left(true)),
        document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        semantic_tokens_provider: Some(
            lsp_types::SemanticTokensOptions {
                work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(),
                legend: lsp_types::SemanticTokensLegend {
                    token_types: baml_ide::TOKEN_TYPES
                        .iter()
                        .map(|t| lsp_types::SemanticTokenType::new(t.as_str()))
                        .collect(),
                    token_modifiers: baml_ide::TOKEN_MODIFIERS
                        .iter()
                        .map(|m| lsp_types::SemanticTokenModifier::new(m))
                        .collect(),
                },
                range: Some(true),
                full: Some(lsp_types::SemanticTokensFullOptions::Delta { delta: Some(true) }),
            }
            .into(),
        ),
        inlay_hint_provider: Some(lsp_types::OneOf::Left(true)),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(lsp_types::OneOf::Left(true)),
            }),
            file_operations: None,
        }),
        diagnostic_provider: None,
        // Lenses ship fully resolved; `codeLens/resolve` is the identity so
        // clients that honour `resolveProvider` get an answer rather than
        // `MethodNotFound`.
        code_lens_provider: open_panel.then_some(lsp_types::CodeLensOptions {
            resolve_provider: Some(true),
        }),
        execute_command_provider: open_panel.then(|| lsp_types::ExecuteCommandOptions {
            commands: vec![OPEN_PANEL_COMMAND.to_owned()],
            work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(),
        }),

        ..ServerCapabilities::default()
    }
}

pub fn initialize_result(encoding: PositionEncoding, open_panel: bool) -> InitializeResult {
    InitializeResult {
        capabilities: server_capabilities(encoding, open_panel),
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
    Ok(initialize_result(
        encoding,
        state.open_panel_handler().is_some(),
    ))
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

/// The one command this server implements: focus the BAML playground on a
/// function, a test, or a testset. The id is a contract with the VS Code
/// extension, which registers it client-side and routes it back here as
/// `workspace/executeCommand`.
pub const OPEN_PANEL_COMMAND: &str = "baml.openBamlPanel";

/// The single argument of [`OPEN_PANEL_COMMAND`], and the payload handed to
/// the host. Field names are the wire spelling; every field is optional, so
/// a bare "open the playground" is `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPanelArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub testset_name: Option<String>,
}

/// A resolved open-panel request: what the host is asked to do. `project` is
/// always a real workspace root — resolution happens here, so hosts never
/// re-derive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPanelRequest {
    pub project: PathBuf,
    pub function_name: Option<String>,
    pub test_name: Option<String>,
    pub testset_name: Option<String>,
}

pub(super) fn execute_command(
    state: &mut GlobalState,
    _session: SessionKey,
    mut params: lsp_types::ExecuteCommandParams,
) -> Result<Option<serde_json::Value>, LspError> {
    if params.command != OPEN_PANEL_COMMAND {
        return Err(LspError::RequestNotSupported(format!(
            "workspace/executeCommand ({})",
            params.command
        )));
    }
    let Some(handler) = state.open_panel_handler().cloned() else {
        return Err(LspError::RequestNotSupported(format!(
            "workspace/executeCommand ({OPEN_PANEL_COMMAND}): this host has no playground"
        )));
    };
    // A missing argument means "just open it"; more than one is a client bug
    // worth reporting rather than guessing at.
    let args: OpenPanelArgs = match params.arguments.len() {
        0 => OpenPanelArgs::default(),
        1 => serde_json::from_value(params.arguments.remove(0)).map_err(|error| {
            LspError::InvalidParams(format!("{OPEN_PANEL_COMMAND} arguments: {error}"))
        })?,
        count => {
            return Err(LspError::InvalidParams(format!(
                "{OPEN_PANEL_COMMAND} takes at most one argument, got {count}"
            )));
        }
    };

    let project = match &args.project_path {
        Some(path) => {
            let path = paths::canonical_physical_path(std::path::Path::new(path));
            state
                .roots()
                .workspace_roots()
                .any(|entry| entry.path == path)
                .then_some(path)
                .ok_or_else(|| {
                    LspError::InvalidParams(format!(
                        "{OPEN_PANEL_COMMAND}: {} is not a workspace root",
                        args.project_path.as_deref().unwrap_or_default()
                    ))
                })?
        }
        // No project named: the sole workspace root is unambiguous, and it is
        // the only shape that exists until the world-viewpoint unit lands.
        None => state
            .roots()
            .workspace_roots()
            .next()
            .map(|entry| entry.path.clone())
            .ok_or_else(|| {
                LspError::RequestFailed(format!("{OPEN_PANEL_COMMAND}: no BAML project is open"))
            })?,
    };

    handler(&OpenPanelRequest {
        project,
        function_name: args.function_name,
        test_name: args.test_name,
        testset_name: args.testset_name,
    });
    Ok(None)
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

// ── Snapshot lane: position-based features ───────────────────────────────────

/// The (file, byte offset) a position request addresses, in the session's
/// encoding.
fn file_offset(
    snap: &crate::snapshot::Snapshot,
    text_document: &lsp_types::TextDocumentIdentifier,
    position: lsp_types::Position,
) -> Result<(baml_db::SourceFile, text_size::TextSize), LspError> {
    let path = crate::paths::canonical_document_path(snap.roots(), &text_document.uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    let codec = PositionCodec::new(file.text(db), snap.cx().encoding);
    let offset = codec.position_to_offset(position)?;
    Ok((file, offset))
}

pub(super) fn hover(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::HoverParams,
) -> Result<Option<lsp_types::Hover>, LspError> {
    let position_params = params.text_document_position_params;
    let (file, offset) = file_offset(
        snap,
        &position_params.text_document,
        position_params.position,
    )?;
    let Some(info) = baml_ide::type_at(snap.db(), file, offset) else {
        return Ok(None);
    };
    Ok(Some(lsp_types::Hover {
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: super::proto::hover_markdown(&info),
        }),
        range: None,
    }))
}

pub(super) fn goto_definition(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::GotoDefinitionParams,
) -> Result<Option<lsp_types::GotoDefinitionResponse>, LspError> {
    let position_params = params.text_document_position_params;
    let (file, offset) = file_offset(
        snap,
        &position_params.text_document,
        position_params.position,
    )?;
    let Some(target) = baml_ide::definition_at(snap.db(), file, offset) else {
        return Ok(None);
    };
    // A stdlib target with no materialized directory has no URI to open —
    // "no definition" is the honest answer, not an error.
    Ok(super::proto::location(snap, target).map(lsp_types::GotoDefinitionResponse::Scalar))
}

pub(super) fn references(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::ReferenceParams,
) -> Result<Option<Vec<lsp_types::Location>>, LspError> {
    let position_params = params.text_document_position;
    let (file, offset) = file_offset(
        snap,
        &position_params.text_document,
        position_params.position,
    )?;
    let db = snap.db();
    let mut targets = baml_ide::usages_at(db, file, offset);
    let include_declaration = params.context.include_declaration;
    if include_declaration
        && let Some(declaration) = baml_ide::definition_at(db, file, offset)
        && !targets.contains(&declaration)
    {
        targets.insert(0, declaration);
    }
    let locations: Vec<lsp_types::Location> = targets
        .into_iter()
        .filter_map(|target| super::proto::location(snap, target))
        .collect();
    Ok((!locations.is_empty()).then_some(locations))
}

pub(super) fn document_symbol(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::DocumentSymbolParams,
) -> Result<Option<lsp_types::DocumentSymbolResponse>, LspError> {
    let text_document = params.text_document;
    let path = crate::paths::canonical_document_path(snap.roots(), &text_document.uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    let codec = PositionCodec::new(file.text(db), snap.cx().encoding);
    let symbols: Vec<lsp_types::DocumentSymbol> = baml_ide::file_outline(db, file)
        .iter()
        .map(|item| super::proto::document_symbol(item, &codec))
        .collect();
    Ok(Some(lsp_types::DocumentSymbolResponse::Nested(symbols)))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "the dispatch table's snapshot-handler contract is fallible"
)]
pub(super) fn workspace_symbol(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::WorkspaceSymbolParams,
) -> Result<Option<lsp_types::WorkspaceSymbolResponse>, LspError> {
    let query = params.query;
    let db = snap.db();
    // The compiler-visible set: workspace symbols plus the stdlib, so
    // `String` or `deep_copy` are findable — goto-def on the result then
    // rides the stdlib URI mapping.
    let files = baml_db::baml_compiler2_hir::compiler2_all_files(db);
    let symbols = baml_ide::search_symbols(db, &files, &query);
    let infos: Vec<lsp_types::SymbolInformation> = symbols
        .into_iter()
        .filter_map(|symbol| {
            let uri = crate::paths::uri_for_db_path(snap.roots(), &symbol.file.path(db))?;
            let codec = PositionCodec::new(symbol.file.text(db), snap.cx().encoding);
            #[expect(
                deprecated,
                reason = "SymbolInformation::deprecated is an LSP wire field; lsp_types keeps it and struct construction must fill it"
            )]
            Some(lsp_types::SymbolInformation {
                name: symbol.name,
                kind: super::proto::symbol_kind(symbol.kind),
                tags: None,
                deprecated: None,
                location: lsp_types::Location {
                    uri,
                    range: codec.byte_range_to_lsp(symbol.name_span),
                },
                container_name: symbol.container_name,
            })
        })
        .collect();
    Ok(Some(lsp_types::WorkspaceSymbolResponse::Flat(infos)))
}

// ── Snapshot lane: semantic tokens + inlay hints ─────────────────────────────

/// Encode ide-layer tokens with LSP line/character deltas in the session's
/// encoding. Multi-line tokens are split into per-line segments (VS Code has
/// no multiline-token capability); document order in, monotonic deltas out.
fn encode_semantic_tokens(
    tokens: &[baml_ide::SemanticToken],
    codec: &PositionCodec<'_>,
) -> Vec<lsp_types::SemanticToken> {
    let mut out = Vec::with_capacity(tokens.len());
    let (mut prev_line, mut prev_start) = (0u32, 0u32);
    for token in tokens {
        for segment in codec.token_segments(token.range) {
            let delta_line = segment.line - prev_line;
            let delta_start = if delta_line == 0 {
                segment.start_character - prev_start
            } else {
                segment.start_character
            };
            out.push(lsp_types::SemanticToken {
                delta_line,
                delta_start,
                length: segment.length,
                token_type: token.token_type.legend_index(),
                token_modifiers_bitset: token.modifiers.bits(),
            });
            prev_line = segment.line;
            prev_start = segment.start_character;
        }
    }
    out
}

/// The full-tokens response plus the baseline commit for the owner. Shared
/// by the full and (fallback path of the) delta handler.
fn full_tokens_with_commit(
    snap: &crate::snapshot::Snapshot,
    uri: &lsp_types::Url,
) -> Result<(lsp_types::SemanticTokens, crate::state::BaselineCommit), LspError> {
    let path = crate::paths::canonical_document_path(snap.roots(), uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    let codec = PositionCodec::new(file.text(db), snap.cx().encoding);
    let encoded = encode_semantic_tokens(baml_ide::semantic_tokens(db, file), &codec);
    let result_id = snap.revision();
    let tokens = Arc::new(encoded);
    Ok((
        lsp_types::SemanticTokens {
            result_id: Some(result_id.0.to_string()),
            data: tokens.as_ref().clone(),
        },
        crate::state::BaselineCommit {
            path,
            baseline: crate::state::TokenBaseline { result_id, tokens },
        },
    ))
}

type CommitOutcome<T> = (Result<T, LspError>, Option<crate::state::BaselineCommit>);

pub(super) fn semantic_tokens_full(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::SemanticTokensParams,
) -> CommitOutcome<Option<lsp_types::SemanticTokensResult>> {
    let text_document = params.text_document;
    match full_tokens_with_commit(snap, &text_document.uri) {
        Ok((tokens, commit)) => (
            Ok(Some(lsp_types::SemanticTokensResult::Tokens(tokens))),
            Some(commit),
        ),
        Err(error) => (Err(error), None),
    }
}

pub(super) fn semantic_tokens_delta(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::SemanticTokensDeltaParams,
) -> CommitOutcome<Option<lsp_types::SemanticTokensFullDeltaResult>> {
    let text_document = params.text_document;
    let previous_result_id = params.previous_result_id;
    let (tokens, commit) = match full_tokens_with_commit(snap, &text_document.uri) {
        Ok(computed) => computed,
        Err(error) => return (Err(error), None),
    };
    // Diff only against the baseline the client says it holds; anything else
    // (evicted on close, another document, a stale id) falls back to full.
    let baseline = snap
        .cx()
        .token_baselines
        .get(&commit.path)
        .filter(|baseline| baseline.result_id.0.to_string() == previous_result_id);
    let Some(baseline) = baseline else {
        return (
            Ok(Some(lsp_types::SemanticTokensFullDeltaResult::Tokens(
                tokens,
            ))),
            Some(commit),
        );
    };

    let previous = baseline.tokens.as_slice();
    let current = commit.baseline.tokens.as_slice();
    let prefix = previous
        .iter()
        .zip(current)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = previous[prefix..]
        .iter()
        .rev()
        .zip(current[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    // Edit offsets count *integers*, five per token (LSP flattens the data).
    let edits = if prefix == previous.len() && previous.len() == current.len() {
        Vec::new()
    } else {
        vec![lsp_types::SemanticTokensEdit {
            start: u32::try_from(5 * prefix).unwrap_or(u32::MAX),
            delete_count: u32::try_from(5 * (previous.len() - prefix - suffix)).unwrap_or(u32::MAX),
            data: Some(current[prefix..current.len() - suffix].to_vec()),
        }]
    };
    let delta = lsp_types::SemanticTokensDelta {
        result_id: tokens.result_id,
        edits,
    };
    (
        Ok(Some(lsp_types::SemanticTokensFullDeltaResult::TokensDelta(
            delta,
        ))),
        Some(commit),
    )
}

pub(super) fn semantic_tokens_range(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::SemanticTokensRangeParams,
) -> Result<Option<lsp_types::SemanticTokensRangeResult>, LspError> {
    let (text_document, range) = (params.text_document, params.range);
    let path = crate::paths::canonical_document_path(snap.roots(), &text_document.uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    let codec = PositionCodec::new(file.text(db), snap.cx().encoding);
    let start = codec.position_to_offset(range.start)?;
    let end = codec.position_to_offset(range.end)?;
    let tokens = baml_ide::semantic_tokens_in_range(db, file, u32::from(start), u32::from(end));
    Ok(Some(lsp_types::SemanticTokensRangeResult::Tokens(
        lsp_types::SemanticTokens {
            result_id: None,
            data: encode_semantic_tokens(&tokens, &codec),
        },
    )))
}

pub(super) fn inlay_hint(
    snap: &crate::snapshot::Snapshot,
    params: lsp_types::InlayHintParams,
) -> Result<Option<Vec<lsp_types::InlayHint>>, LspError> {
    let (text_document, range) = (params.text_document, params.range);
    let path = crate::paths::canonical_document_path(snap.roots(), &text_document.uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    let codec = PositionCodec::new(file.text(db), snap.cx().encoding);
    let start = codec.position_to_offset(range.start)?;
    let end = codec.position_to_offset(range.end)?;
    let hints = baml_ide::file_annotations(db, file)
        .iter()
        .filter(|annotation| start <= annotation.offset && annotation.offset <= end)
        .map(|annotation| lsp_types::InlayHint {
            position: codec.offset_to_position(u32::from(annotation.offset)),
            label: lsp_types::InlayHintLabel::String(annotation.label.clone()),
            kind: Some(match annotation.kind {
                baml_ide::AnnotationKind::Type => lsp_types::InlayHintKind::TYPE,
                baml_ide::AnnotationKind::Parameter => lsp_types::InlayHintKind::PARAMETER,
            }),
            text_edits: None,
            tooltip: None,
            padding_left: Some(annotation.padding_left),
            padding_right: Some(annotation.padding_right),
            data: None,
        })
        .collect::<Vec<_>>();
    Ok(Some(hints))
}

/// One lens per runnable item in the file, each carrying a fully-resolved
/// [`OPEN_PANEL_COMMAND`].
#[expect(
    clippy::needless_pass_by_value,
    reason = "dispatch-table signature: handlers own their params"
)]
pub(super) fn code_lens(
    snap: &Snapshot,
    params: lsp_types::CodeLensParams,
) -> Result<Option<Vec<lsp_types::CodeLens>>, LspError> {
    let path = paths::canonical_document_path(snap.roots(), &params.text_document.uri)?;
    let db = snap.db();
    let Some(file) = db.get_file(&path) else {
        return Err(LspError::FileNotFound(path));
    };
    // Lenses run things, and only a workspace root has a runtime to run them
    // in; stdlib and dependency files are read-only views.
    let Some(project) = snap
        .roots()
        .root_for_path(&path)
        .filter(|entry| entry.kind == baml_db::SourceRootKind::Workspace)
        .map(|entry| entry.path.to_string_lossy().into_owned())
    else {
        return Ok(None);
    };
    let codec = PositionCodec::new(file.text(db), snap.cx().encoding);
    let lenses = baml_ide::file_actions(db, file)
        .into_iter()
        .map(|action| {
            let (title, args) = match action.kind {
                baml_ide::FileActionKind::RunInPlayground => (
                    "▶ Open 🐑 Playground",
                    OpenPanelArgs {
                        project_path: Some(project.clone()),
                        function_name: Some(action.name),
                        ..OpenPanelArgs::default()
                    },
                ),
                baml_ide::FileActionKind::RunTest => (
                    "▶ Run test",
                    OpenPanelArgs {
                        project_path: Some(project.clone()),
                        test_name: Some(action.name),
                        ..OpenPanelArgs::default()
                    },
                ),
                baml_ide::FileActionKind::RunTestSet => (
                    "▶ Run testset",
                    OpenPanelArgs {
                        project_path: Some(project.clone()),
                        testset_name: Some(action.name),
                        ..OpenPanelArgs::default()
                    },
                ),
            };
            lsp_types::CodeLens {
                range: codec.byte_range_to_lsp(action.name_span),
                command: Some(lsp_types::Command {
                    title: title.to_owned(),
                    command: OPEN_PANEL_COMMAND.to_owned(),
                    arguments: Some(vec![serde_json::to_value(&args).unwrap_or_else(|error| {
                        unreachable!("OpenPanelArgs always serializes: {error}")
                    })]),
                }),
                data: None,
            }
        })
        .collect();
    Ok(Some(lenses))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_advertise_the_negotiated_encoding() {
        assert_eq!(
            server_capabilities(PositionEncoding::UTF8, false).position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF8)
        );
        assert_eq!(
            server_capabilities(PositionEncoding::UTF16, false).position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF16)
        );
    }

    #[test]
    fn capabilities_are_incremental_sync_push_diagnostics_only() {
        let capabilities = server_capabilities(PositionEncoding::UTF16, false);
        let Some(TextDocumentSyncCapability::Options(sync)) = capabilities.text_document_sync
        else {
            panic!("text sync options expected");
        };
        assert_eq!(sync.change, Some(TextDocumentSyncKind::INCREMENTAL));
        assert_eq!(sync.will_save, Some(false));
        assert!(capabilities.diagnostic_provider.is_none());
    }

    /// Lenses and the command they invoke are advertised together, and only
    /// when the host can actually run it — a lens for a command the server
    /// would reject is a button that does nothing.
    #[test]
    fn code_lens_is_advertised_only_with_a_playground_host() {
        let without = server_capabilities(PositionEncoding::UTF16, false);
        assert!(without.code_lens_provider.is_none());
        assert!(without.execute_command_provider.is_none());

        let with = server_capabilities(PositionEncoding::UTF16, true);
        assert_eq!(
            with.code_lens_provider
                .map(|options| options.resolve_provider),
            Some(Some(true))
        );
        assert_eq!(
            with.execute_command_provider
                .map(|options| options.commands),
            Some(vec![OPEN_PANEL_COMMAND.to_owned()])
        );
    }

    /// The lens payload is the command payload: what a lens serializes must
    /// round-trip through the argument the client sends back.
    #[test]
    fn open_panel_args_round_trip_in_the_wire_spelling() {
        let args = OpenPanelArgs {
            project_path: Some("/ws".to_owned()),
            test_name: Some("adds".to_owned()),
            ..OpenPanelArgs::default()
        };
        let value = serde_json::to_value(&args).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "projectPath": "/ws", "testName": "adds" }),
            "absent fields are omitted, present ones are camelCase"
        );
        assert_eq!(
            serde_json::from_value::<OpenPanelArgs>(value).unwrap(),
            args
        );
        // A bare open carries no argument at all.
        assert_eq!(
            serde_json::from_value::<OpenPanelArgs>(serde_json::json!({})).unwrap(),
            OpenPanelArgs::default()
        );
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
