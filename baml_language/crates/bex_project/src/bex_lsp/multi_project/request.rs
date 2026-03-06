use lsp_types::{
    CodeLens, CodeLensOptions, CompletionOptions, HoverProviderCapability, InlayHintOptions,
    InlayHintServerCapabilities, SaveOptions, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, WorkDoneProgressOptions, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities,
};

use super::{BexMulitProject, LspError, WithDiagnostics, commands, wasm_helpers};
use crate::bex_lsp::{multi_project::commands::BexLspCommand, request::BexLspRequest};

/// Server capabilities advertised during the LSP `initialize` handshake.
///
/// Defined here so that both the native stdio server and the WASM bridge
/// share a single source of truth for what the LSP implementation supports.
pub(super) fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
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
        ..Default::default()
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
        let mut projects = self.projects.lock().unwrap();
        projects.clear();
        Ok(())
    }

    fn on_request_initialize(
        &self,
        params: lsp_request_params!("initialize"),
    ) -> Result<lsp_request_result!("initialize"), LspError> {
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
            "Workspace roots: {:?}",
            roots.iter().map(vfs::VfsPath::as_str).collect::<Vec<_>>()
        );

        *self.workspace_roots.lock().unwrap() = roots;

        Ok(lsp_types::InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(lsp_types::ServerInfo {
                name: "baml-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    fn on_request_text_document_code_lens(
        &self,
        params: lsp_request_params!("textDocument/codeLens"),
    ) -> Result<lsp_request_result!("textDocument/codeLens"), LspError> {
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path.clone())?;

        let lenses = {
            let project = project_handle.project.try_lock_db()?;
            let lsp_db = project.db();
            let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
                return Ok(None);
            };
            let text = source_file.text(lsp_db);
            let line_starts = compute_line_starts(text);
            let encoding = self.position_encoding;

            // Use compiler2 file_actions — finds functions + tests via
            // file_symbol_contributions (Salsa-cached, no type inference needed).
            let file_actions = baml_lsp2_actions::file_actions(lsp_db, source_file);

            file_actions
                .into_iter()
                .map(|action| {
                    let range = super::diagnostics::span_to_lsp_range(
                        action.name_span,
                        text,
                        &line_starts,
                        encoding,
                    );
                    let command = match action.kind {
                        baml_lsp2_actions::FileActionKind::RunInPlayground => {
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name: Some(action.name),
                            }
                            .to_lsp_command()
                        }
                        baml_lsp2_actions::FileActionKind::RunTest => {
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name: Some(action.name),
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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let project = project_handle.project.try_lock_db()?;
        let lsp_db = project.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        let text = source_file.text(lsp_db);

        // Compute the byte-offset bounds of the requested range.
        let range_start = text_size::TextSize::from(
            u32::try_from(baml_project::position::lsp_position_to_offset(
                text,
                &params.range.start,
            ))
            .unwrap_or(0),
        );
        let range_end = text_size::TextSize::from(
            u32::try_from(baml_project::position::lsp_position_to_offset(
                text,
                &params.range.end,
            ))
            .unwrap_or(u32::MAX),
        );

        // Compute inline annotations using compiler2 (type hints + param hints).
        let hints = baml_lsp2_actions::annotations(lsp_db, source_file);

        let lsp_hints: Vec<lsp_types::InlayHint> = hints
            .into_iter()
            .filter(|h| h.offset >= range_start && h.offset < range_end)
            .map(|h| lsp_types::InlayHint {
                position: baml_project::position::offset_to_lsp_position(
                    text,
                    usize::from(h.offset),
                ),
                label: lsp_types::InlayHintLabel::String(h.label),
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

        // Convert to LSP delta-encoded format
        let line_index = baml_project::position::LineIndex::new(text);
        let mut lsp_tokens = Vec::with_capacity(tokens.len());
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;

        for token in &tokens {
            let start_offset: u32 = token.range.start().into();
            let end_offset: u32 = token.range.end().into();
            let length = end_offset - start_offset;

            let Some(pos) = line_index.offset_to_position(start_offset) else {
                continue;
            };

            let delta_line = pos.line - prev_line;
            let delta_start = if delta_line == 0 {
                pos.character - prev_start
            } else {
                pos.character
            };

            lsp_tokens.push(lsp_types::SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type: token.token_type.legend_index(),
                token_modifiers_bitset: 0,
            });

            prev_line = pos.line;
            prev_start = pos.character;
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

            // Convert the LSP range to a byte range for fixes_at.
            let start_offset = text_size::TextSize::from(
                u32::try_from(baml_project::position::lsp_position_to_offset(
                    text,
                    &params.range.start,
                ))
                .unwrap_or(0),
            );
            let end_offset = text_size::TextSize::from(
                u32::try_from(baml_project::position::lsp_position_to_offset(
                    text,
                    &params.range.end,
                ))
                .unwrap_or(u32::MAX),
            );
            let range = text_size::TextRange::new(start_offset, end_offset);

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

                let _ = self.get_or_create_project(project_path.clone())?;

                self.playground_sender.send_playground_notification(
                    crate::bex_lsp::PlaygroundNotification::OpenPlayground {
                        project: project_path.as_str().to_string(),
                        function_name,
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
        let position_encoding = self.position_encoding;
        self.compute_on_position(
            &params.text_document_position_params,
            |db, source_file, _, offset| {
                let loc = baml_lsp2_actions::definition_at(db, source_file, offset)?;
                let file_id = loc.file.file_id(db);
                let path = db.file_id_to_path(file_id)?;
                let target_uri = wasm_helpers::from_file_path(path).ok()?;
                let target_text = loc.file.text(db);
                let line_starts = compute_line_starts(target_text);
                let range = super::diagnostics::span_to_lsp_range(
                    loc.range,
                    target_text,
                    &line_starts,
                    position_encoding,
                );
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
        let position_encoding = self.position_encoding;
        let references: Vec<lsp_types::Location> = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, _, offset| {
                // Use compiler2 usages_at — returns Vec<Location> (file + TextRange).
                let usages = baml_lsp2_actions::usages_at(db, source_file, offset);

                usages
                    .into_iter()
                    .filter_map(|loc| {
                        let file_id = loc.file.file_id(db);
                        let path = db.file_id_to_path(file_id)?;
                        let target_uri = wasm_helpers::from_file_path(path).ok()?;
                        let target_text = loc.file.text(db);
                        let line_starts = compute_line_starts(target_text);
                        let range = super::diagnostics::span_to_lsp_range(
                            loc.range,
                            target_text,
                            &line_starts,
                            position_encoding,
                        );
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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let mut diagnostics = project_handle
            .project
            .diagnostics_by_file(self.position_encoding);
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
                let range = super::diagnostics::span_to_lsp_range(
                    sym.name_span,
                    text,
                    &compute_line_starts(text),
                    super::diagnostics::PositionEncoding::UTF16,
                );

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
            text: &str,
            line_starts: &[u32],
            encoding: super::diagnostics::PositionEncoding,
        ) -> lsp_types::DocumentSymbol {
            let range =
                super::diagnostics::span_to_lsp_range(item.name_span, text, line_starts, encoding);

            let children = if item.children.is_empty() {
                None
            } else {
                Some(
                    item.children
                        .iter()
                        .map(|child| convert_outline_item(child, text, line_starts, encoding))
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

        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;

        let project = project_handle.project.try_lock_db()?;
        let lsp_db = project.db();
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        let text = source_file.text(lsp_db);
        let line_starts = compute_line_starts(text);
        let encoding = self.position_encoding;
        let outline = baml_lsp2_actions::file_outline(lsp_db, source_file);

        let symbols: Vec<_> = outline
            .iter()
            .map(|item| convert_outline_item(item, text, &line_starts, encoding))
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
        let path = self.get_path_from_uri(&params.text_document.uri)?;
        let root_path = Self::get_baml_project_root(&path)?;
        let project_handle = self.get_or_create_project(root_path)?;
        // Get current file text from the project database.
        let text = {
            let db = project_handle.project.try_lock_db()?;
            let Some(source_file) = db.get_file(std::path::Path::new(path.as_str())) else {
                return Err(LspError::FileNotFound(path));
            };
            source_file.text(&*db).clone()
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

        // Compute end position of the original text for the replacement range.
        let line_count = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
        let last_line_len =
            u32::try_from(text.lines().last().map_or(0, str::len)).unwrap_or(u32::MAX);
        let (end_line, end_char) = if text.ends_with('\n') {
            (line_count, 0)
        } else {
            (line_count.saturating_sub(1), last_line_len)
        };

        Ok(Some(vec![lsp_types::TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
                end: lsp_types::Position {
                    line: end_line,
                    character: end_char,
                },
            },
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
        DefinitionKind::TypeAlias => lsp_types::SymbolKind::CLASS,
        DefinitionKind::Client => lsp_types::SymbolKind::STRUCT,
        DefinitionKind::Test => lsp_types::SymbolKind::METHOD,
        DefinitionKind::Generator => lsp_types::SymbolKind::INTERFACE,
        DefinitionKind::TemplateString => lsp_types::SymbolKind::FUNCTION,
        DefinitionKind::RetryPolicy => lsp_types::SymbolKind::STRUCT,
        DefinitionKind::Field => lsp_types::SymbolKind::FIELD,
        DefinitionKind::Method => lsp_types::SymbolKind::METHOD,
        DefinitionKind::Variant => lsp_types::SymbolKind::ENUM_MEMBER,
        // Locals don't appear in the outline but handle them gracefully.
        DefinitionKind::Binding | DefinitionKind::Parameter => lsp_types::SymbolKind::VARIABLE,
    }
}

/// Alias for the `compute_line_starts` helper from the diagnostics module.
#[inline]
fn compute_line_starts(source: &str) -> Vec<u32> {
    super::diagnostics::compute_line_starts(source)
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
        let offset = text_size::TextSize::from(
            u32::try_from(baml_project::position::lsp_position_to_offset(
                text, &position,
            ))
            .unwrap_or(0),
        );

        Ok(op(lsp_db, source_file, project, offset))
    }
}
