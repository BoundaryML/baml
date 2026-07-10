use baml_project::position::{LspPositionCodec, PositionEncoding};
use lsp_types::{
    CodeLens, CodeLensOptions, CompletionOptions, HoverProviderCapability, InlayHintOptions,
    InlayHintServerCapabilities, SaveOptions, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, WorkDoneProgressOptions, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities,
};

use super::{BexMulitProject, LspError, WithDiagnostics, commands, wasm_helpers};
use crate::bex_lsp::{multi_project::commands::BexLspCommand, protocol, request::BexLspRequest};

/// Server capabilities advertised during the LSP `initialize` handshake.
///
/// Defined here so that both the native stdio server and the WASM bridge
/// share a single source of truth for what the LSP implementation supports.
pub(super) fn server_capabilities(position_encoding: PositionEncoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(position_encoding.as_lsp_kind()),
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
        code_action_provider: None,
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
                    token_modifiers: vec![],
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                ..Default::default()
            },
        )),
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

fn initialize_result(position_encoding: PositionEncoding) -> lsp_types::InitializeResult {
    lsp_types::InitializeResult {
        capabilities: server_capabilities(position_encoding),
        server_info: Some(lsp_types::ServerInfo {
            name: "baml-lsp".to_string(),
            version: Some(baml_version::CANONICAL_VERSION.to_string()),
        }),
    }
}

impl BexLspRequest for BexMulitProject {
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
        // Projects are process-owned and may still be serving the standalone
        // playground or a replacement browser LSP session. The transport
        // lifecycle tears down only this connection's dispatcher/sender.
        Ok(())
    }

    fn on_request_initialize(
        &self,
        params: lsp_request_params!("initialize"),
    ) -> Result<lsp_request_result!("initialize"), LspError> {
        let position_encoding = PositionEncoding::negotiate(
            params
                .capabilities
                .general
                .as_ref()
                .and_then(|general| general.position_encodings.as_deref()),
        );
        self.session_config
            .set(super::SessionConfig { position_encoding })
            .map_err(|_| LspError::InvalidParams("initialize may only be sent once".to_string()))?;

        let mut roots = Vec::new();

        if let Some(folders) = &params.workspace_folders {
            for folder in folders {
                if let Ok(path) = self.get_path_from_uri_unchecked(&folder.uri) {
                    roots.push(path);
                }
            }
        }

        #[allow(deprecated)]
        if roots.is_empty() {
            if let Some(root_uri) = &params.root_uri {
                if let Ok(path) = self.get_path_from_uri_unchecked(root_uri) {
                    roots.push(path);
                }
            }
        }

        tracing::info!(
            "Workspace roots: {:?}",
            roots.iter().map(vfs::VfsPath::as_str).collect::<Vec<_>>()
        );

        *self.workspace_roots.lock().unwrap() = roots;

        Ok(initialize_result(position_encoding))
    }

    fn on_request_text_document_code_lens(
        &self,
        params: lsp_request_params!("textDocument/codeLens"),
    ) -> Result<lsp_request_result!("textDocument/codeLens"), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path.clone())?;
        let encoding = self.position_encoding()?;

        let lenses = {
            let project = project_handle.project.try_lock_db()?;
            let lsp_db = project.db();
            let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
                return Ok(None);
            };
            let text = source_file.text(lsp_db);
            let codec = LspPositionCodec::new(text, encoding);

            // Use compiler2 file_actions — finds functions + tests via
            // file_symbol_contributions (Salsa-cached, no type inference needed).
            let file_actions = baml_lsp2_actions::file_actions(lsp_db, source_file);

            file_actions
                .into_iter()
                .map(|action| -> Result<CodeLens, LspError> {
                    let range = codec.text_range_to_range(action.name_span)?;
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
                    Ok(CodeLens {
                        range,
                        command: Some(command),
                        data: None,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(Some(lenses))
    }

    fn on_request_text_document_inlay_hint(
        &self,
        params: lsp_request_params!("textDocument/inlayHint"),
    ) -> Result<lsp_request_result!("textDocument/inlayHint"), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let project = project_handle.project.try_lock_db()?;
        let lsp_db = project.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        let text = source_file.text(lsp_db);
        let codec = LspPositionCodec::new(text, self.position_encoding()?);

        // Compute the byte-offset bounds of the requested range.
        let requested_range = codec.range_to_text_range(params.range)?;
        let range_start = requested_range.start();
        let range_end = requested_range.end();

        // Compute inline annotations using compiler2 (type hints + param hints).
        let hints = baml_lsp2_actions::annotations(lsp_db, source_file);

        let lsp_hints: Vec<lsp_types::InlayHint> = hints
            .iter()
            .filter(|h| h.offset >= range_start && h.offset < range_end)
            .map(|h| -> Result<lsp_types::InlayHint, LspError> {
                Ok(lsp_types::InlayHint {
                    position: codec.offset_to_position(h.offset.into())?,
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
            })
            .collect::<Result<Vec<_>, _>>()?;

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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let project = project_handle.project.try_lock_db()?;
        let lsp_db = project.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };
        let text = source_file.text(lsp_db);

        // Get the semantic tokens using compiler2 (hybrid CST + type-aware).
        // Always returns tokens in document order.
        let tokens = baml_lsp2_actions::semantic_tokens(lsp_db, source_file);

        // Convert to LSP delta-encoded format. Multiline compiler spans are
        // split because VS Code does not advertise multiline token support.
        let codec = LspPositionCodec::new(text, self.position_encoding()?);
        let mut lsp_tokens = Vec::with_capacity(tokens.len());
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;
        let mut prev_end_offset = None;

        for token in tokens {
            for segment in codec.semantic_token_segments(token.range)? {
                if prev_end_offset.is_some_and(|previous| segment.start_offset < previous) {
                    return Err(LspError::InvalidParams(
                        "compiler produced overlapping semantic tokens".to_string(),
                    ));
                }

                let delta_line = segment.start.line.checked_sub(prev_line).ok_or_else(|| {
                    LspError::InvalidParams(
                        "compiler produced out-of-order semantic tokens".to_string(),
                    )
                })?;
                let delta_start = if delta_line == 0 {
                    segment
                        .start
                        .character
                        .checked_sub(prev_start)
                        .ok_or_else(|| {
                            LspError::InvalidParams(
                                "compiler produced out-of-order semantic tokens".to_string(),
                            )
                        })?
                } else {
                    segment.start.character
                };

                lsp_tokens.push(lsp_types::SemanticToken {
                    delta_line,
                    delta_start,
                    length: segment.length,
                    token_type: token.token_type.legend_index(),
                    token_modifiers_bitset: 0,
                });

                prev_line = segment.start.line;
                prev_start = segment.start.character;
                prev_end_offset = Some(segment.end_offset);
            }
        }

        Ok(Some(lsp_types::SemanticTokensResult::Tokens(
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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path.clone())?;

        let actions: Vec<lsp_types::CodeActionOrCommand> = {
            let project = project_handle.project.try_lock_db()?;
            let lsp_db = project.db();
            let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
                return Ok(None);
            };
            let text = source_file.text(lsp_db);
            let codec = LspPositionCodec::new(text, self.position_encoding()?);

            // Convert the LSP range to a byte range for fixes_at.
            let range = codec.range_to_text_range(params.range)?;

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
                    self.fs.get_path_from_str(
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
                        .get_path_from_str(&first_key, "workspace/executeCommand")?
                };

                self.validate_owned_path(&project_path)?;
                let _ = self.get_or_create_project(project_path.clone())?;

                if let Some(port) = self.playground_sender.lsp_playground_port() {
                    self.sender
                        .send_notification(lsp_server::Notification::new(
                            "baml/openPlayground".to_string(),
                            serde_json::json!({
                                "port": port,
                                "projectPath": project_path.as_str(),
                                "functionName": &function_name,
                                "testName": &test_name,
                                "testsetName": &testset_name,
                            }),
                        ))?;
                }
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
        use lsp_types::CompletionItemKind;

        // Use compiler2 completions_at — context-aware completions from CST + HIR/TIR.
        let completions = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, _project, offset| {
                baml_lsp2_actions::completions_at(db, source_file, offset)
            },
        )?;

        // Convert domain Completion → LSP CompletionItem.
        let items: Vec<_> = completions
            .into_iter()
            .map(|item| lsp_types::CompletionItem {
                label: item.label,
                kind: Some(match item.kind {
                    baml_lsp2_actions::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                    baml_lsp2_actions::CompletionKind::Function => CompletionItemKind::FUNCTION,
                    baml_lsp2_actions::CompletionKind::Class => CompletionItemKind::CLASS,
                    baml_lsp2_actions::CompletionKind::Enum => CompletionItemKind::ENUM,
                    baml_lsp2_actions::CompletionKind::EnumVariant => {
                        CompletionItemKind::ENUM_MEMBER
                    }
                    baml_lsp2_actions::CompletionKind::Field => CompletionItemKind::FIELD,
                    baml_lsp2_actions::CompletionKind::Variable => CompletionItemKind::VARIABLE,
                    baml_lsp2_actions::CompletionKind::Primitive => {
                        CompletionItemKind::TYPE_PARAMETER
                    }
                    baml_lsp2_actions::CompletionKind::TypeAlias => {
                        CompletionItemKind::TYPE_PARAMETER
                    }
                    baml_lsp2_actions::CompletionKind::TemplateString => {
                        CompletionItemKind::FUNCTION
                    }
                    baml_lsp2_actions::CompletionKind::Client => CompletionItemKind::MODULE,
                    baml_lsp2_actions::CompletionKind::Generator => CompletionItemKind::MODULE,
                    baml_lsp2_actions::CompletionKind::Test => CompletionItemKind::METHOD,
                    baml_lsp2_actions::CompletionKind::RetryPolicy => CompletionItemKind::MODULE,
                    baml_lsp2_actions::CompletionKind::Method => CompletionItemKind::METHOD,
                    baml_lsp2_actions::CompletionKind::Module => CompletionItemKind::MODULE,
                    baml_lsp2_actions::CompletionKind::Parameter => CompletionItemKind::FIELD,
                }),
                detail: item.detail,
                insert_text: item.insert_text,
                sort_text: item.sort_text,
                ..Default::default()
            })
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
            |db, source_file, _project, offset| baml_lsp2_actions::type_at(db, source_file, offset),
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
        let position_encoding = self.position_encoding()?;
        self.compute_on_position(
            &params.text_document_position_params,
            |db, source_file, _, offset| {
                let loc = baml_lsp2_actions::definition_at(db, source_file, offset)?;
                let file_id = loc.file.file_id(db);
                let path = db.file_id_to_path(file_id)?;
                let target_uri = wasm_helpers::from_file_path(path).ok()?;
                let target_text = loc.file.text(db);
                let range = match LspPositionCodec::new(target_text, position_encoding)
                    .text_range_to_range(loc.range)
                {
                    Ok(range) => range,
                    Err(error) => return Some(Err(error.into())),
                };
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
        let position_encoding = self.position_encoding()?;
        let references: Vec<lsp_types::Location> = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, _, offset| -> Result<Vec<_>, LspError> {
                // Use compiler2 usages_at — returns Vec<Location> (file + TextRange).
                let usages = baml_lsp2_actions::usages_at(db, source_file, offset);
                let mut locations = Vec::with_capacity(usages.len());
                for loc in usages {
                    let file_id = loc.file.file_id(db);
                    let Some(path) = db.file_id_to_path(file_id) else {
                        continue;
                    };
                    let Ok(target_uri) = wasm_helpers::from_file_path(path) else {
                        continue;
                    };
                    let target_text = loc.file.text(db);
                    let range = LspPositionCodec::new(target_text, position_encoding)
                        .text_range_to_range(loc.range)?;
                    locations.push(lsp_types::Location {
                        uri: target_uri,
                        range,
                    });
                }
                Ok(locations)
            },
        )??;

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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let mut diagnostics = match project_handle
            .project
            .diagnostics_by_file(self.position_encoding()?)
        {
            super::diagnostics::DiagnosticRead::Ready(candidate) => candidate.documents,
            super::diagnostics::DiagnosticRead::Busy => {
                return Err(LspError::RequestFailed(
                    "Project diagnostics are temporarily busy".to_string(),
                ));
            }
            super::diagnostics::DiagnosticRead::Poisoned => {
                project_handle
                    .project
                    .mark_broken("serving pull diagnostics");
                return Err(LspError::InternalError(
                    "Project database is poisoned".to_string(),
                ));
            }
        };
        let diagnostics = diagnostics
            .remove(std::path::Path::new(path.as_str()))
            .unwrap_or_default();
        Ok(lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(
                lsp_types::RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: lsp_types::FullDocumentDiagnosticReport {
                        result_id: None,
                        items: diagnostics,
                    },
                },
            ),
        ))
    }

    fn on_request_workspace_symbol(
        &self,
        params: lsp_request_params!("workspace/symbol"),
    ) -> Result<lsp_request_result!("workspace/symbol"), LspError> {
        let query = &params.query;
        let mut symbols = Vec::new();
        let encoding = self.position_encoding()?;

        let projects = self.projects.lock().unwrap();
        for project_handle in projects.values() {
            let Ok(db_guard) = project_handle.project.try_lock_db() else {
                continue;
            };
            let lsp_db = db_guard.db();

            // Use compiler2 search_symbols — iterates all user source files and
            // filters by the query string. file_outline is Salsa-cached per file,
            // so repeat calls for unchanged files are free.
            let source_files = lsp_db.get_source_files();
            let results = baml_lsp2_actions::search_symbols(lsp_db, &source_files, query);

            for sym in results {
                let file_id = sym.file.file_id(lsp_db);
                let Some(path) = lsp_db.file_id_to_path(file_id) else {
                    continue;
                };
                let Ok(uri) = wasm_helpers::from_file_path(path) else {
                    continue;
                };
                let text = sym.file.text(lsp_db);
                let range =
                    LspPositionCodec::new(text, encoding).text_range_to_range(sym.name_span)?;

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
            codec: &LspPositionCodec<'_>,
        ) -> Result<lsp_types::DocumentSymbol, LspError> {
            let range = codec.text_range_to_range(item.name_span)?;

            let children = if item.children.is_empty() {
                None
            } else {
                Some(
                    item.children
                        .iter()
                        .map(|child| convert_outline_item(child, codec))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };

            #[allow(deprecated)]
            Ok(lsp_types::DocumentSymbol {
                name: item.name.clone(),
                kind: definition_kind_to_lsp_symbol_kind(item.kind),
                detail: None,
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children,
            })
        }

        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let project = project_handle.project.try_lock_db()?;
        let lsp_db = project.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        let text = source_file.text(lsp_db);
        let codec = LspPositionCodec::new(text, self.position_encoding()?);
        let outline = baml_lsp2_actions::file_outline(lsp_db, source_file);

        let symbols: Vec<_> = outline
            .iter()
            .map(|item| convert_outline_item(item, &codec))
            .collect::<Result<Vec<_>, _>>()?;

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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;
        // Get current file text from the project database.
        let text = {
            let db = project_handle.project.try_lock_db()?;
            let Some(source_file) = db.get_file(std::path::Path::new(path.as_str())) else {
                return Err(LspError::FileNotFound(path));
            };
            source_file.text(&**db).clone()
        };

        // Map LSP FormattingOptions → baml_fmt FormatOptions.
        let options = baml_fmt::FormatOptions::default();

        // Run the formatter. On parse errors, return no edits (silently skip).
        let formatted = match baml_fmt::format(&text, &options) {
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

        let codec = LspPositionCodec::new(&text, self.position_encoding()?);

        Ok(Some(vec![lsp_types::TextEdit {
            range: codec.document_range(),
            new_text: formatted,
        }]))
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

impl BexMulitProject {
    fn compute_on_position<T>(
        &self,
        params: &lsp_types::TextDocumentPositionParams,
        op: impl FnOnce(
            &baml_project::ProjectDatabase,
            baml_db::SourceFile,
            baml_workspace::Project,
            text_size::TextSize,
        ) -> T,
    ) -> Result<T, LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;
        let position = params.position;

        let project = project_handle.project.try_lock_db()?;
        let lsp_db = project.db();
        let Some(project) = project.project() else {
            return Err(LspError::ProjectNotFound(path));
        };
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Err(LspError::FileNotFound(path));
        };
        let text = source_file.text(lsp_db);
        let codec = LspPositionCodec::new(text, self.position_encoding()?);
        let offset = text_size::TextSize::from(codec.position_to_offset(position)?);

        Ok(op(lsp_db, source_file, project, offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_metadata_matches_protocol_contract() {
        let result = initialize_result(PositionEncoding::Utf8);
        assert_eq!(
            result.capabilities.position_encoding,
            Some(lsp_types::PositionEncodingKind::UTF8)
        );
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
    fn position_encoding_prefers_utf8_and_never_selects_utf32() {
        assert_eq!(PositionEncoding::negotiate(None), PositionEncoding::Utf16);
        assert_eq!(
            PositionEncoding::negotiate(Some(&[lsp_types::PositionEncodingKind::UTF32])),
            PositionEncoding::Utf16
        );
        assert_eq!(
            PositionEncoding::negotiate(Some(&[
                lsp_types::PositionEncodingKind::UTF16,
                lsp_types::PositionEncodingKind::UTF8,
            ])),
            PositionEncoding::Utf8
        );
    }

    #[test]
    fn dispatcher_clone_observes_config_initialized_later() {
        let config = std::sync::Arc::new(std::sync::OnceLock::new());
        let clone_created_before_initialize = std::sync::Arc::clone(&config);
        assert!(matches!(
            super::super::read_session_config(&clone_created_before_initialize),
            Err(LspError::ServerNotInitialized(_))
        ));

        config
            .set(super::super::SessionConfig {
                position_encoding: PositionEncoding::Utf8,
            })
            .unwrap();

        assert_eq!(
            super::super::read_session_config(&clone_created_before_initialize).unwrap(),
            super::super::SessionConfig {
                position_encoding: PositionEncoding::Utf8,
            }
        );

        let replacement_browser_session = std::sync::OnceLock::new();
        assert!(matches!(
            super::super::read_session_config(&replacement_browser_session),
            Err(LspError::ServerNotInitialized(_))
        ));
    }
}
