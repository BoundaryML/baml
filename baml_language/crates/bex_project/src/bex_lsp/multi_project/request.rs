//! LSP request handlers.
//!
//! Every handler follows the same discipline:
//!
//! - Database access is a *bounded* read ([`super::read_for_request`]):
//!   busy projects produce typed `ContentModified`/`RequestFailed` errors
//!   instead of stalling the loop or bursting `-32001`.
//! - Positions and ranges cross the LSP boundary through one
//!   [`PositionCodec`] built with the encoding negotiated during
//!   `initialize`. Compiler APIs stay byte-based.

use lsp_types::{
    CodeActionProviderCapability, CodeLens, CodeLensOptions, CompletionOptions,
    HoverProviderCapability, InlayHintOptions, InlayHintServerCapabilities, SaveOptions,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    WorkDoneProgressOptions, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};

use super::{BexMultiProject, LspError, commands, read_for_request, wasm_helpers};
use crate::bex_lsp::{
    multi_project::commands::BexLspCommand,
    position_codec::{PositionCodec, PositionEncoding},
    protocol,
    request::BexLspRequest,
};

/// Server capabilities advertised during the LSP `initialize` handshake.
///
/// Defined here so that both the native stdio server and the WASM bridge
/// share a single source of truth for what the LSP implementation supports.
/// `encoding` is the negotiated position encoding; advertising it is
/// mandatory whenever the client offered `positionEncodings`.
pub(super) fn server_capabilities(encoding: PositionEncoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(encoding.to_lsp_kind()),
        // Diagnostics are delivered via push (`publishDiagnostics`) only.
        // Pull diagnostics (`textDocument/diagnostic`) is disabled to avoid
        // the editor showing each diagnostic twice.
        diagnostic_provider: None,
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec!['@'.to_string(), '"'.to_string(), '.'.to_string()]),
            ..Default::default()
        }),
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(true),
        }),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        execute_command_provider: Some(lsp_types::ExecuteCommandOptions {
            commands: vec![commands::OpenBamlPanel::COMMAND_ID.to_string()],
            work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(),
        }),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        references_provider: Some(lsp_types::OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: baml_lsp2_actions::TOKEN_TYPES
                        .iter()
                        .map(|t| lsp_types::SemanticTokenType::new(t.as_str()))
                        .collect(),
                    token_modifiers: baml_lsp2_actions::TOKEN_MODIFIERS
                        .iter()
                        .map(|name| lsp_types::SemanticTokenModifier::new(name))
                        .collect(),
                },
                full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                range: Some(true),
                ..Default::default()
            },
        )),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                will_save: Some(false),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
                ..Default::default()
            },
        )),
        document_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        workspace_symbol_provider: Some(lsp_types::OneOf::Left(true)),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(lsp_types::OneOf::Left(true)),
            }),
            ..Default::default()
        }),
        inlay_hint_provider: Some(lsp_types::OneOf::Right(
            InlayHintServerCapabilities::Options(InlayHintOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                resolve_provider: Some(false),
            }),
        )),
        experimental: Some(serde_json::json!({
            "baml": {
                "toolchainVersion": baml_version::CANONICAL_VERSION,
                "lspProtocol": protocol::BAML_LSP_PROTOCOL_VERSION,
                "minSupportedClientLspProtocol": protocol::MIN_SUPPORTED_VSCODE_LSP_PROTOCOL,
                "playgroundProtocol": protocol::BAML_PLAYGROUND_PROTOCOL_VERSION,
                "minSupportedClientPlaygroundProtocol": protocol::MIN_SUPPORTED_PLAYGROUND_PROTOCOL,
                "capabilities": protocol::CAPABILITIES,
            }
        })),
        ..Default::default()
    }
}

fn initialize_result(encoding: PositionEncoding) -> lsp_types::InitializeResult {
    lsp_types::InitializeResult {
        capabilities: server_capabilities(encoding),
        server_info: Some(lsp_types::ServerInfo {
            name: "baml-lsp".to_string(),
            version: Some(baml_version::CANONICAL_VERSION.to_string()),
        }),
    }
}

impl BexLspRequest for BexMultiProject {
    fn request_sender(
        &self,
    ) -> Box<
        dyn Fn(lsp_server::RequestId, Result<serde_json::Value, LspError>) -> Result<(), LspError>
            + '_,
    > {
        let sender = self.sender.clone();
        Box::new(
            move |id: lsp_server::RequestId, result: Result<serde_json::Value, LspError>| {
                sender.send_response(id, result)
            },
        )
    }

    fn on_request_shutdown(
        &self,
        _params: lsp_request_params!("shutdown"),
    ) -> Result<lsp_request_result!("shutdown"), LspError> {
        let mut projects = self.projects.lock().unwrap();
        projects.clear();
        Ok(())
    }

    fn on_request_initialize(
        &self,
        params: lsp_request_params!("initialize"),
    ) -> Result<lsp_request_result!("initialize"), LspError> {
        // Negotiate the position encoding first: UTF-8 when offered,
        // UTF-16 baseline otherwise. Everything after this reads the cell.
        let encoding = self.negotiate_encoding(&params.capabilities);
        let snippet_support = self.negotiate_snippet_support(&params.capabilities);

        let mut roots = Vec::new();

        if let Some(folders) = &params.workspace_folders {
            for folder in folders {
                if let Ok(path) = self.get_path_from_uri(&folder.uri) {
                    roots.push(path);
                }
            }
        }

        #[allow(deprecated)]
        if roots.is_empty() {
            if let Some(root_uri) = &params.root_uri {
                if let Ok(path) = self.get_path_from_uri(root_uri) {
                    roots.push(path);
                }
            }
        }

        tracing::info!(
            "Workspace roots: {:?}; position encoding: {:?}; snippet support: {}",
            roots.iter().map(vfs::VfsPath::as_str).collect::<Vec<_>>(),
            encoding,
            snippet_support,
        );

        *self.workspace_roots.lock().unwrap() = roots;

        Ok(initialize_result(encoding))
    }

    fn on_request_text_document_code_lens(
        &self,
        params: lsp_request_params!("textDocument/codeLens"),
    ) -> Result<lsp_request_result!("textDocument/codeLens"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path.clone())?;

        let lenses = {
            let guard = read_for_request(&project_handle.project)?;
            let lsp_db = guard.db();
            let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
                return Ok(None);
            };
            let text = source_file.text(lsp_db);
            let codec = PositionCodec::new(text, encoding);

            // Use compiler2 file_actions — finds functions + tests via
            // file_symbol_contributions (Salsa-cached, no type inference needed).
            let file_actions = baml_lsp2_actions::file_actions(lsp_db, source_file);

            file_actions
                .into_iter()
                .map(|action| {
                    let range = codec.byte_range_to_lsp(action.name_span);
                    let command = match action.kind {
                        baml_lsp2_actions::FileActionKind::RunInPlayground => {
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name: Some(action.name),
                                test_name: None,
                                testset_name: None,
                                title: None,
                            }
                            .to_lsp_command()
                        }
                        baml_lsp2_actions::FileActionKind::RunTest => {
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name: None,
                                test_name: Some(action.name),
                                testset_name: None,
                                title: Some("▶ Run test".to_string()),
                            }
                            .to_lsp_command()
                        }
                        baml_lsp2_actions::FileActionKind::RunTestSet => {
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name: None,
                                test_name: None,
                                testset_name: Some(action.name),
                                title: Some("▶ Run testset".to_string()),
                            }
                            .to_lsp_command()
                        }
                    };
                    CodeLens {
                        range,
                        command: Some(command),
                        data: None,
                    }
                })
                .collect()
        };

        Ok(Some(lenses))
    }

    fn on_request_text_document_inlay_hint(
        &self,
        params: lsp_request_params!("textDocument/inlayHint"),
    ) -> Result<lsp_request_result!("textDocument/inlayHint"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let guard = read_for_request(&project_handle.project)?;
        let lsp_db = guard.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        let text = source_file.text(lsp_db);
        let codec = PositionCodec::new(text, encoding);
        let range = codec.range_to_byte_range(params.range)?;

        // Inline annotations are Salsa-memoized per file revision; the
        // handler only converts the cached hints into the requested range.
        let hints = baml_lsp2_actions::file_annotations(lsp_db, source_file);

        let lsp_hints: Vec<lsp_types::InlayHint> = hints
            .iter()
            .filter(|h| h.offset >= range.start() && h.offset < range.end())
            .map(|h| lsp_types::InlayHint {
                position: codec.offset_to_position(h.offset.into()),
                label: lsp_types::InlayHintLabel::String(h.label.clone()),
                kind: Some(match h.kind {
                    baml_lsp2_actions::AnnotationKind::Type => lsp_types::InlayHintKind::TYPE,
                    baml_lsp2_actions::AnnotationKind::Parameter => {
                        lsp_types::InlayHintKind::PARAMETER
                    }
                }),
                padding_left: Some(h.padding_left),
                padding_right: Some(h.padding_right),
                text_edits: None,
                tooltip: None,
                data: None,
            })
            .collect();

        if lsp_hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lsp_hints))
        }
    }

    fn on_request_text_document_semantic_tokens_full(
        &self,
        params: lsp_request_params!("textDocument/semanticTokens/full"),
    ) -> Result<lsp_request_result!("textDocument/semanticTokens/full"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let guard = read_for_request(&project_handle.project)?;
        let lsp_db = guard.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };
        let text = source_file.text(lsp_db);
        let codec = PositionCodec::new(text, encoding);

        // Semantic tokens are Salsa-memoized per file revision; the
        // handler only delta-encodes the cached classification.
        let tokens = baml_lsp2_actions::semantic_tokens(lsp_db, source_file);
        let lsp_tokens = encode_semantic_tokens(tokens, &codec);
        let result_id = self.cache_semantic_tokens(&path, lsp_tokens.clone());

        Ok(Some(lsp_types::SemanticTokensResult::Tokens(
            lsp_types::SemanticTokens {
                result_id: Some(result_id),
                data: lsp_tokens,
            },
        )))
    }

    /// Incremental semantic tokens — rust-analyzer's `full/delta`. Diffs the new
    /// token array against the cached one for the client's `previous_result_id`
    /// and returns just the edits; falls back to the full set on a cache miss.
    fn on_request_text_document_semantic_tokens_full_delta(
        &self,
        params: lsp_request_params!("textDocument/semanticTokens/full/delta"),
    ) -> Result<lsp_request_result!("textDocument/semanticTokens/full/delta"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let guard = read_for_request(&project_handle.project)?;
        let lsp_db = guard.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };
        let text = source_file.text(lsp_db);
        let codec = PositionCodec::new(text, encoding);

        let tokens = baml_lsp2_actions::semantic_tokens(lsp_db, source_file);
        let new_tokens = encode_semantic_tokens(tokens, &codec);

        // The previous token array iff its result_id matches what the client holds.
        let key = crate::fs::FsPath::from_vfs(&path);
        let prev = {
            let cache = self.semantic_tokens_cache.lock().unwrap();
            cache
                .get(&key)
                .filter(|(id, _)| *id == params.previous_result_id)
                .map(|(_, toks)| toks.clone())
        };
        let result_id = self.cache_semantic_tokens(&path, new_tokens.clone());

        match prev {
            Some(prev_tokens) => Ok(Some(lsp_types::SemanticTokensFullDeltaResult::TokensDelta(
                lsp_types::SemanticTokensDelta {
                    result_id: Some(result_id),
                    edits: diff_semantic_tokens(&prev_tokens, &new_tokens),
                },
            ))),
            None => Ok(Some(lsp_types::SemanticTokensFullDeltaResult::Tokens(
                lsp_types::SemanticTokens {
                    result_id: Some(result_id),
                    data: new_tokens,
                },
            ))),
        }
    }

    /// Viewport semantic tokens — rust-analyzer's `highlight_range`. Resolves
    /// only the scopes the requested range touches.
    fn on_request_text_document_semantic_tokens_range(
        &self,
        params: lsp_request_params!("textDocument/semanticTokens/range"),
    ) -> Result<lsp_request_result!("textDocument/semanticTokens/range"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let guard = read_for_request(&project_handle.project)?;
        let lsp_db = guard.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };
        let text = source_file.text(lsp_db);
        let codec = PositionCodec::new(text, encoding);

        let range = codec.range_to_byte_range(params.range)?;
        let tokens = baml_lsp2_actions::tokens::semantic_tokens_in_range(
            lsp_db,
            source_file,
            range.start().into(),
            range.end().into(),
        );
        let lsp_tokens = encode_semantic_tokens(&tokens, &codec);

        Ok(Some(lsp_types::SemanticTokensRangeResult::Tokens(
            lsp_types::SemanticTokens {
                result_id: None,
                data: lsp_tokens,
            },
        )))
    }

    fn on_request_text_document_code_action(
        &self,
        params: lsp_request_params!("textDocument/codeAction"),
    ) -> Result<lsp_request_result!("textDocument/codeAction"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path.clone())?;

        let actions: Vec<lsp_types::CodeActionOrCommand> = {
            let guard = read_for_request(&project_handle.project)?;
            let lsp_db = guard.db();
            let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
                return Ok(None);
            };
            let text = source_file.text(lsp_db);
            let codec = PositionCodec::new(text, encoding);
            let range = codec.range_to_byte_range(params.range)?;

            // Use compiler2 fixes_at — currently returns "Open in Playground".
            let fixes = baml_lsp2_actions::fixes_at(lsp_db, source_file, range);

            fixes
                .into_iter()
                .map(|fix| {
                    let command = match fix.kind {
                        baml_lsp2_actions::FixKind::OpenInPlayground { function_name } => {
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name,
                                test_name: None,
                                testset_name: None,
                                title: None,
                            }
                            .to_lsp_code_action()
                        }
                    };
                    lsp_types::CodeActionOrCommand::CodeAction(command)
                })
                .collect()
        };

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    fn on_request_workspace_execute_command(
        &self,
        mut params: lsp_request_params!("workspace/executeCommand"),
    ) -> Result<lsp_request_result!("workspace/executeCommand"), LspError> {
        use super::commands;
        match params.command.as_str() {
            commands::OpenBamlPanel::COMMAND_ID => {
                if params.arguments.len() != 1 {
                    return Err(LspError::InvalidCommandArguments {
                        command: params.command.clone(),
                        message: format!("Invalid argument count: {} != 1", params.arguments.len()),
                    });
                }
                let args = params.arguments.remove(0);
                let commands::OpenBamlPanel {
                    project_path,
                    function_name,
                    test_name,
                    testset_name,
                    ..
                } = serde_json::from_value(args).map_err(|e| {
                    LspError::InvalidCommandArguments {
                        command: params.command.clone(),
                        message: format!("Invalid arguments: {e}"),
                    }
                })?;

                let project_path = if let Some(pp) = project_path {
                    self.fs.get_path_from_vfs_path(
                        &crate::fs::FsPath::from_str(pp),
                        "workspace/executeCommand",
                    )?
                } else {
                    let first_key = {
                        let projects = self.projects.lock().unwrap();
                        projects
                            .keys()
                            .next()
                            .cloned()
                            .ok_or(LspError::NoProjectsFound)?
                    };
                    self.fs
                        .get_path_from_vfs_path(&first_key, "workspace/executeCommand")?
                };

                let _ = self.get_or_create_project(project_path.clone())?;

                self.playground_sender.send_playground_notification(
                    crate::bex_lsp::PlaygroundNotification::OpenPlayground {
                        project: project_path.as_str().to_string(),
                        function_name,
                        test_name,
                        testset_name,
                    },
                );

                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn on_request_text_document_completion(
        &self,
        params: lsp_request_params!("textDocument/completion"),
    ) -> Result<lsp_request_result!("textDocument/completion"), LspError> {
        // Use compiler2 completions_at — context-aware completions from CST + HIR/TIR.
        let completions = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, offset, _| baml_lsp2_actions::completions_at(db, source_file, offset),
        )?;

        // Convert domain Completion → LSP CompletionItem.
        let snippet_support = self.snippet_support_for_request()?;
        let items: Vec<_> = completions
            .into_iter()
            .map(|item| completion_to_lsp(item, snippet_support))
            .collect();

        if items.is_empty() {
            return Ok(None);
        }

        Ok(Some(lsp_types::CompletionResponse::List(
            lsp_types::CompletionList {
                is_incomplete: true,
                items,
            },
        )))
    }

    fn on_request_text_document_hover(
        &self,
        params: lsp_request_params!("textDocument/hover"),
    ) -> Result<lsp_request_result!("textDocument/hover"), LspError> {
        let type_info = self.compute_on_position(
            &params.text_document_position_params,
            |db, source_file, offset, _| baml_lsp2_actions::type_at(db, source_file, offset),
        )?;

        match type_info {
            Some(info) => {
                let content = info.to_hover_markdown();
                Ok(Some(lsp_types::Hover {
                    contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: content,
                    }),
                    range: None,
                }))
            }
            None => Ok(None),
        }
    }

    fn on_request_text_document_definition(
        &self,
        params: lsp_request_params!("textDocument/definition"),
    ) -> Result<lsp_request_result!("textDocument/definition"), LspError> {
        self.compute_on_position(
            &params.text_document_position_params,
            |db, source_file, offset, encoding| {
                let loc = baml_lsp2_actions::definition_at(db, source_file, offset)?;
                let file_id = loc.file.file_id(db);
                let path = db.file_id_to_path(file_id)?;
                let target_uri = wasm_helpers::from_file_path(path).ok()?;
                // The definition may live in another file: convert with the
                // target file's codec, not the request file's.
                let target_codec = PositionCodec::new(loc.file.text(db), encoding);
                let range = target_codec.byte_range_to_lsp(loc.range);
                Some(Ok(lsp_types::GotoDefinitionResponse::Scalar(
                    lsp_types::Location {
                        uri: target_uri,
                        range,
                    },
                )))
            },
        )?
        .transpose()
    }

    fn on_request_text_document_references(
        &self,
        params: lsp_request_params!("textDocument/references"),
    ) -> Result<lsp_request_result!("textDocument/references"), LspError> {
        let references: Vec<lsp_types::Location> = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, offset, encoding| {
                // Use compiler2 usages_at — returns Vec<Location> (file + TextRange).
                let usages = baml_lsp2_actions::usages_at(db, source_file, offset);

                // Building a codec scans the whole target file for its line
                // table; many usages land in the same file, so build one
                // codec per distinct file, not per result.
                let mut codecs = std::collections::HashMap::new();
                usages
                    .into_iter()
                    .filter_map(|loc| {
                        let file_id = loc.file.file_id(db);
                        let path = db.file_id_to_path(file_id)?;
                        let target_uri = wasm_helpers::from_file_path(path).ok()?;
                        let target_codec = codecs
                            .entry(file_id)
                            .or_insert_with(|| PositionCodec::new(loc.file.text(db), encoding));
                        let range = target_codec.byte_range_to_lsp(loc.range);
                        Some(lsp_types::Location {
                            uri: target_uri,
                            range,
                        })
                    })
                    .collect()
            },
        )?;

        if references.is_empty() {
            Ok(None)
        } else {
            Ok(Some(references))
        }
    }

    fn on_request_text_document_diagnostic(
        &self,
        params: lsp_request_params!("textDocument/diagnostic"),
    ) -> Result<lsp_request_result!("textDocument/diagnostic"), LspError> {
        // Pull diagnostics is not advertised, but answer correctly for
        // clients that ask anyway: same candidate + codec conversion as the
        // push path.
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let candidate = {
            let guard = read_for_request(&project_handle.project)?;
            crate::project::collect_diagnostic_candidate(&guard)
        };
        let documents = super::diagnostics::candidate_to_publishable(&candidate, encoding);
        let items = documents
            .into_iter()
            .find(|doc| doc.path.as_path() == std::path::Path::new(path.as_str()))
            .map(|doc| doc.diagnostics)
            .unwrap_or_default();

        Ok(lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(
                lsp_types::RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: lsp_types::FullDocumentDiagnosticReport {
                        result_id: None,
                        items,
                    },
                },
            ),
        ))
    }

    fn on_request_workspace_symbol(
        &self,
        params: lsp_request_params!("workspace/symbol"),
    ) -> Result<lsp_request_result!("workspace/symbol"), LspError> {
        let encoding = self.encoding_for_request()?;
        let query = &params.query;
        let mut symbols = Vec::new();

        // Fan-out across every project: skip busy projects instead of
        // serializing bounded waits (a symbol search should not stack
        // N seconds of deadlines).
        let projects: Vec<_> = {
            let projects = self.projects.lock().unwrap();
            projects.values().cloned().collect()
        };
        for project_handle in projects {
            let Ok(Some(guard)) = project_handle.project.read_source_nowait() else {
                continue;
            };
            let lsp_db = guard.db();

            // Use compiler2 search_symbols — iterates all user source files and
            // filters by the query string. file_outline is Salsa-cached per file,
            // so repeat calls for unchanged files are free.
            let source_files = lsp_db.get_source_files();
            let results = baml_lsp2_actions::search_symbols(lsp_db, &source_files, query);

            // One codec per distinct file: a broad query matches many
            // symbols in the same file, and codec construction scans the
            // whole file for its line table.
            let mut codecs = std::collections::HashMap::new();
            for sym in results {
                let file_id = sym.file.file_id(lsp_db);
                let Some(path) = lsp_db.file_id_to_path(file_id) else {
                    continue;
                };
                let Ok(uri) = wasm_helpers::from_file_path(path) else {
                    continue;
                };
                let codec = codecs
                    .entry(file_id)
                    .or_insert_with(|| PositionCodec::new(sym.file.text(lsp_db), encoding));
                let range = codec.byte_range_to_lsp(sym.name_span);

                symbols.push(lsp_types::WorkspaceSymbol {
                    name: sym.name,
                    kind: definition_kind_to_lsp_symbol_kind(sym.kind),
                    tags: None,
                    container_name: sym.container_name,
                    location: lsp_types::OneOf::Left(lsp_types::Location { uri, range }),
                    data: None,
                });
            }
        }

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lsp_types::WorkspaceSymbolResponse::Nested(symbols)))
        }
    }

    fn on_request_text_document_document_symbol(
        &self,
        params: lsp_request_params!("textDocument/documentSymbol"),
    ) -> Result<lsp_request_result!("textDocument/documentSymbol"), LspError> {
        fn convert_outline_item(
            item: &baml_lsp2_actions::OutlineItem,
            codec: &PositionCodec<'_>,
        ) -> lsp_types::DocumentSymbol {
            let range = codec.byte_range_to_lsp(item.name_span);

            let children = if item.children.is_empty() {
                None
            } else {
                Some(
                    item.children
                        .iter()
                        .map(|child| convert_outline_item(child, codec))
                        .collect(),
                )
            };

            #[allow(deprecated)]
            lsp_types::DocumentSymbol {
                name: item.name.clone(),
                kind: definition_kind_to_lsp_symbol_kind(item.kind),
                detail: None,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children,
            }
        }

        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let guard = read_for_request(&project_handle.project)?;
        let lsp_db = guard.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        let codec = PositionCodec::new(source_file.text(lsp_db), encoding);
        let outline = baml_lsp2_actions::file_outline(lsp_db, source_file);

        let symbols: Vec<_> = outline
            .iter()
            .map(|item| convert_outline_item(item, &codec))
            .collect();

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lsp_types::DocumentSymbolResponse::Nested(symbols)))
        }
    }

    fn on_request_text_document_formatting(
        &self,
        params: lsp_request_params!("textDocument/formatting"),
    ) -> Result<lsp_request_result!("textDocument/formatting"), LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;
        // Format the current source file in the project database. Keep the
        // text for edit comparison and diagnostics, but reuse the existing
        // Salsa input instead of constructing a second ProjectDatabase and
        // reparsing cloned source through `baml_fmt::format`.
        let (text, formatted) = {
            let guard = read_for_request(&project_handle.project)?;
            let lsp_db = guard.db();
            let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
                return Err(LspError::FileNotFound(path));
            };
            let text = source_file.text(lsp_db).clone();
            let formatted =
                baml_fmt::format_salsa(lsp_db, source_file, baml_fmt::FormatOptions::default());
            (text, formatted)
        };

        // Run the formatter. On parse errors, return no edits (silently skip).
        let formatted = match formatted {
            Ok(f) => f,
            Err(baml_fmt::FormatterError::ParseErrors { .. }) => return Ok(None),
            Err(baml_fmt::FormatterError::StrongAstError(e)) => {
                return Err(crate::RuntimeError::Other(format!(
                    "Failed to build strong AST: {}",
                    e.print_with_file_context(path.as_str(), &text)
                ))
                .into());
            }
        };

        // No change → no edits.
        if formatted == text {
            return Ok(None);
        }

        // Replace the whole document: [0:0, document end) in the negotiated
        // encoding (the old line-count arithmetic miscounted `\r\n` and
        // non-ASCII last lines).
        let codec = PositionCodec::new(&text, encoding);
        Ok(Some(vec![lsp_types::TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
                end: codec.document_end(),
            },
            new_text: formatted,
        }]))
    }
}

fn completion_to_lsp(
    item: baml_lsp2_actions::Completion,
    snippet_support: bool,
) -> lsp_types::CompletionItem {
    use lsp_types::{CompletionItemKind, InsertTextFormat};

    let (insert_text, insert_text_format) = match item.insert_text_format {
        baml_lsp2_actions::CompletionInsertTextFormat::Snippet if !snippet_support => (None, None),
        baml_lsp2_actions::CompletionInsertTextFormat::PlainText => {
            (item.insert_text, Some(InsertTextFormat::PLAIN_TEXT))
        }
        baml_lsp2_actions::CompletionInsertTextFormat::Snippet => {
            (item.insert_text, Some(InsertTextFormat::SNIPPET))
        }
    };

    lsp_types::CompletionItem {
        label: item.label,
        kind: Some(match item.kind {
            baml_lsp2_actions::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
            baml_lsp2_actions::CompletionKind::Function => CompletionItemKind::FUNCTION,
            baml_lsp2_actions::CompletionKind::Class => CompletionItemKind::CLASS,
            baml_lsp2_actions::CompletionKind::Enum => CompletionItemKind::ENUM,
            baml_lsp2_actions::CompletionKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
            baml_lsp2_actions::CompletionKind::Field => CompletionItemKind::FIELD,
            baml_lsp2_actions::CompletionKind::Variable => CompletionItemKind::VARIABLE,
            baml_lsp2_actions::CompletionKind::Primitive
            | baml_lsp2_actions::CompletionKind::TypeAlias => CompletionItemKind::TYPE_PARAMETER,
            baml_lsp2_actions::CompletionKind::TemplateString => CompletionItemKind::FUNCTION,
            baml_lsp2_actions::CompletionKind::Client
            | baml_lsp2_actions::CompletionKind::Generator
            | baml_lsp2_actions::CompletionKind::RetryPolicy
            | baml_lsp2_actions::CompletionKind::Module => CompletionItemKind::MODULE,
            baml_lsp2_actions::CompletionKind::Test | baml_lsp2_actions::CompletionKind::Method => {
                CompletionItemKind::METHOD
            }
            baml_lsp2_actions::CompletionKind::Parameter => CompletionItemKind::FIELD,
        }),
        detail: item.detail,
        insert_text,
        insert_text_format,
        sort_text: item.sort_text,
        ..Default::default()
    }
}

/// Delta-encode classified tokens through the connection's negotiated codec
/// into the LSP wire format. `delta_start` and `length` are in negotiated code
/// units, not bytes. Shared by the `full` and `range` requests.
/// Multiline tokens are split into same-line segments — VS Code does not
/// advertise the multiline token capability, and splitting is valid for every
/// client.
fn encode_semantic_tokens(
    tokens: &[baml_lsp2_actions::tokens::SemanticToken],
    codec: &PositionCodec<'_>,
) -> Vec<lsp_types::SemanticToken> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
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

/// Minimal single-edit diff of two encoded token arrays (rust-analyzer's
/// approach): trim the common prefix and suffix at token granularity and
/// replace only the differing middle. `start`/`delete_count` are offsets into
/// the flat LSP `u32` stream (5 integers per token), so they scale by 5.
fn diff_semantic_tokens(
    prev: &[lsp_types::SemanticToken],
    new: &[lsp_types::SemanticToken],
) -> Vec<lsp_types::SemanticTokensEdit> {
    let mut p = 0;
    while p < prev.len() && p < new.len() && prev[p] == new[p] {
        p += 1;
    }
    let mut s = 0;
    while s < prev.len() - p
        && s < new.len() - p
        && prev[prev.len() - 1 - s] == new[new.len() - 1 - s]
    {
        s += 1;
    }
    let deleted = prev.len() - p - s;
    let data = new[p..new.len() - s].to_vec();
    if deleted == 0 && data.is_empty() {
        return Vec::new();
    }
    vec![lsp_types::SemanticTokensEdit {
        start: u32::try_from(p * 5).unwrap_or(u32::MAX),
        delete_count: u32::try_from(deleted * 5).unwrap_or(u32::MAX),
        data: Some(data),
    }]
}

#[cfg(test)]
mod semantic_tokens_delta_tests {
    use super::diff_semantic_tokens;

    fn tok(line: u32) -> lsp_types::SemanticToken {
        lsp_types::SemanticToken {
            delta_line: line,
            delta_start: 0,
            length: 1,
            token_type: 0,
            token_modifiers_bitset: 0,
        }
    }

    #[test]
    fn identical_yields_no_edits() {
        let a = vec![tok(1), tok(2), tok(3)];
        assert!(diff_semantic_tokens(&a, &a).is_empty());
    }

    #[test]
    fn middle_replacement_scales_by_five() {
        let edits = diff_semantic_tokens(&[tok(1), tok(2), tok(3)], &[tok(1), tok(9), tok(3)]);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].start, 5); // one unchanged leading token
        assert_eq!(edits[0].delete_count, 5); // one token replaced
        assert_eq!(edits[0].data.as_ref().unwrap(), &[tok(9)]);
    }

    #[test]
    fn append_is_pure_insert() {
        let edits = diff_semantic_tokens(&[tok(1)], &[tok(1), tok(2)]);
        assert_eq!(edits[0].start, 5);
        assert_eq!(edits[0].delete_count, 0);
        assert_eq!(edits[0].data.as_ref().unwrap(), &[tok(2)]);
    }

    #[test]
    fn truncate_is_pure_delete() {
        let edits = diff_semantic_tokens(&[tok(1), tok(2)], &[tok(1)]);
        assert_eq!(edits[0].start, 5);
        assert_eq!(edits[0].delete_count, 5);
        assert!(edits[0].data.as_ref().unwrap().is_empty());
    }
}

#[cfg(test)]
mod semantic_tokens_encoding_tests {
    use baml_lsp2_actions::tokens::{ModifierSet, SemanticToken, SemanticTokenType};
    use text_size::TextRange;

    use super::encode_semantic_tokens;
    use crate::bex_lsp::position_codec::{PositionCodec, PositionEncoding};

    fn tok(start: u32, end: u32) -> SemanticToken {
        SemanticToken {
            range: TextRange::new(start.into(), end.into()),
            token_type: SemanticTokenType::Variable,
            modifiers: ModifierSet::empty(),
        }
    }

    /// `delta_start`/`length` are code units of the negotiated encoding, not
    /// bytes — the miscount the #3867 revert called out for non-ASCII sources.
    #[test]
    fn utf16_lengths_and_columns_are_code_units_not_bytes() {
        // "名前" is 6 bytes / 2 UTF-16 units; "é" is 2 bytes / 1 unit.
        let text = "let 名前 = \"café\"";
        let codec = PositionCodec::new(text, PositionEncoding::UTF16);
        // `名前` at bytes 4..10, `"café"` at bytes 13..20.
        let out = encode_semantic_tokens(&[tok(4, 10), tok(13, 20)], &codec);

        assert_eq!(out.len(), 2);
        assert_eq!((out[0].delta_start, out[0].length), (4, 2));
        // Same line: delta from col 4 to col 9 (4 + 2 + " = ").
        assert_eq!(
            (out[1].delta_line, out[1].delta_start, out[1].length),
            (0, 5, 6)
        );
    }

    /// A surrogate-pair character counts as 2 UTF-16 units (4 under UTF-8).
    #[test]
    fn surrogate_pairs_count_two_utf16_units() {
        let text = "\u{1D54F} = 1"; // 𝕏: 4 bytes, one astral char
        let utf16 = PositionCodec::new(text, PositionEncoding::UTF16);
        let utf8 = PositionCodec::new(text, PositionEncoding::UTF8);

        assert_eq!(encode_semantic_tokens(&[tok(0, 4)], &utf16)[0].length, 2);
        assert_eq!(encode_semantic_tokens(&[tok(0, 4)], &utf8)[0].length, 4);
    }

    /// Multiline tokens split into per-line segments (clients don't advertise
    /// the multiline token capability), with line-relative `delta_start`.
    #[test]
    fn multiline_token_splits_into_line_segments() {
        let text = "ab\ncdef";
        let codec = PositionCodec::new(text, PositionEncoding::UTF16);
        let out = encode_semantic_tokens(&[tok(0, 5)], &codec);

        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[0].delta_line, out[0].delta_start, out[0].length),
            (0, 0, 2)
        );
        assert_eq!(
            (out[1].delta_line, out[1].delta_start, out[1].length),
            (1, 0, 2)
        );
    }
}

/// Convert a compiler2 `DefinitionKind` to an LSP `SymbolKind`.
///
/// Used by the `textDocument/documentSymbol` and `workspace/symbol` handlers
/// that call `baml_lsp2_actions::file_outline` / `search_symbols`.
fn definition_kind_to_lsp_symbol_kind(
    kind: baml_lsp2_actions::DefinitionKind,
) -> lsp_types::SymbolKind {
    use baml_lsp2_actions::DefinitionKind;
    match kind {
        DefinitionKind::Function => lsp_types::SymbolKind::FUNCTION,
        DefinitionKind::Class => lsp_types::SymbolKind::CLASS,
        DefinitionKind::Enum => lsp_types::SymbolKind::ENUM,
        DefinitionKind::Interface => lsp_types::SymbolKind::INTERFACE,
        DefinitionKind::TypeAlias => lsp_types::SymbolKind::CLASS,
        DefinitionKind::Client => lsp_types::SymbolKind::STRUCT,
        DefinitionKind::Test => lsp_types::SymbolKind::METHOD,
        DefinitionKind::TemplateString => lsp_types::SymbolKind::FUNCTION,
        DefinitionKind::RetryPolicy => lsp_types::SymbolKind::STRUCT,
        DefinitionKind::Let => lsp_types::SymbolKind::CONSTANT,
        DefinitionKind::Field => lsp_types::SymbolKind::FIELD,
        DefinitionKind::Method => lsp_types::SymbolKind::METHOD,
        DefinitionKind::Variant => lsp_types::SymbolKind::ENUM_MEMBER,
        DefinitionKind::AssociatedType => lsp_types::SymbolKind::TYPE_PARAMETER,
        // Locals don't appear in the outline but handle them gracefully.
        DefinitionKind::Binding | DefinitionKind::Parameter => lsp_types::SymbolKind::VARIABLE,
    }
}

impl BexMultiProject {
    /// Store `tokens` as the latest semantic tokens for `path` under a fresh
    /// `result_id`, returning that id so the next `full/delta` can diff against it.
    ///
    /// The cache is connection-scoped (tokens are encoded in this connection's
    /// negotiated encoding); only the id sequence is shared process-wide.
    fn cache_semantic_tokens(
        &self,
        path: &vfs::VfsPath,
        tokens: Vec<lsp_types::SemanticToken>,
    ) -> String {
        let id = self
            .semantic_tokens_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_string();
        self.semantic_tokens_cache
            .lock()
            .unwrap()
            .insert(crate::fs::FsPath::from_vfs(path), (id.clone(), tokens));
        id
    }

    /// Shared scaffolding for position-based requests: bounded read, file
    /// lookup, and incoming position conversion through the negotiated codec.
    ///
    /// The closure receives the database, the file, the byte offset of the
    /// request position, and the negotiated encoding (for converting result
    /// spans that may live in *other* files).
    fn compute_on_position<T>(
        &self,
        params: &lsp_types::TextDocumentPositionParams,
        op: impl FnOnce(
            &baml_project::ProjectDatabase,
            baml_db::SourceFile,
            text_size::TextSize,
            PositionEncoding,
        ) -> T,
    ) -> Result<T, LspError> {
        let encoding = self.encoding_for_request()?;
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;
        let position = params.position;

        let guard = read_for_request(&project_handle.project)?;
        let lsp_db = guard.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Err(LspError::FileNotFound(path));
        };
        let text = source_file.text(lsp_db);
        let codec = PositionCodec::new(text, encoding);
        let offset = codec.position_to_offset(position)?;

        Ok(op(lsp_db, source_file, offset, encoding))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_metadata_matches_protocol_contract() {
        let result = initialize_result(PositionEncoding::UTF16);
        let experimental = result
            .capabilities
            .experimental
            .expect("initialize result should advertise experimental metadata");

        assert_eq!(
            experimental,
            serde_json::json!({
                "baml": {
                    "toolchainVersion": baml_version::CANONICAL_VERSION,
                    "lspProtocol": 1,
                    "minSupportedClientLspProtocol": 1,
                    "playgroundProtocol": 1,
                    "minSupportedClientPlaygroundProtocol": 1,
                    "capabilities": [
                        "openPlayground.v1",
                        "listProjects.v1",
                        "playgroundWebSocket.v1",
                    ],
                }
            })
        );
        assert_eq!(
            result.server_info.and_then(|info| info.version),
            Some(baml_version::CANONICAL_VERSION.to_string())
        );
    }

    #[test]
    fn capabilities_advertise_negotiated_encoding() {
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
    fn capabilities_do_not_request_will_save_notifications() {
        let Some(TextDocumentSyncCapability::Options(options)) =
            server_capabilities(PositionEncoding::UTF16).text_document_sync
        else {
            panic!("expected text document sync options");
        };

        assert_eq!(options.will_save, Some(false));
    }

    #[test]
    fn capabilities_advertise_code_actions() {
        assert!(matches!(
            server_capabilities(PositionEncoding::UTF16).code_action_provider,
            Some(CodeActionProviderCapability::Simple(true))
        ));
    }

    #[test]
    fn completion_conversion_preserves_snippet_format() {
        let completion = baml_lsp2_actions::Completion {
            label: "function".to_string(),
            kind: baml_lsp2_actions::CompletionKind::Keyword,
            detail: Some("function declaration".to_string()),
            insert_text: Some("function ${1:Name}() {\n  $0\n}".to_string()),
            insert_text_format: baml_lsp2_actions::CompletionInsertTextFormat::Snippet,
            sort_text: Some("02_function".to_string()),
        };
        let item = completion_to_lsp(completion.clone(), true);

        assert_eq!(
            item.insert_text.as_deref(),
            Some("function ${1:Name}() {\n  $0\n}")
        );
        assert_eq!(
            item.insert_text_format,
            Some(lsp_types::InsertTextFormat::SNIPPET)
        );

        let item = completion_to_lsp(completion, false);
        assert_eq!(item.insert_text, None);
        assert_eq!(item.insert_text_format, None);
    }
}
