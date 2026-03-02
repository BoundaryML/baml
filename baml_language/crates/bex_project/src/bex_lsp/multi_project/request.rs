use lsp_types::{
    CodeLens, CodeLensOptions, CompletionOptions, DiagnosticOptions, DiagnosticServerCapabilities,
    HoverProviderCapability, InlayHintOptions, InlayHintServerCapabilities, SaveOptions,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    WorkDoneProgressOptions, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};

use super::{BexMulitProject, LspError, WithDiagnostics, commands, wasm_helpers};
use crate::bex_lsp::{multi_project::commands::BexLspCommand, request::BexLspRequest};

const DIAGNOSTIC_NAME: &str = "BAML";

/// Server capabilities advertised during the LSP `initialize` handshake.
///
/// Defined here so that both the native stdio server and the WASM bridge
/// share a single source of truth for what the LSP implementation supports.
pub(super) fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            identifier: Some(DIAGNOSTIC_NAME.into()),
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
                    token_types: baml_lsp_actions::TOKEN_TYPES
                        .iter()
                        .map(|t| {
                            lsp_types::SemanticTokenType::new(match t {
                                baml_lsp_actions::SemanticTokenType::Namespace => "namespace",
                                baml_lsp_actions::SemanticTokenType::Type => "type",
                                baml_lsp_actions::SemanticTokenType::Class => "class",
                                baml_lsp_actions::SemanticTokenType::Enum => "enum",
                                baml_lsp_actions::SemanticTokenType::Interface => "interface",
                                baml_lsp_actions::SemanticTokenType::Struct => "struct",
                                baml_lsp_actions::SemanticTokenType::TypeParameter => {
                                    "typeParameter"
                                }
                                baml_lsp_actions::SemanticTokenType::Parameter => "parameter",
                                baml_lsp_actions::SemanticTokenType::Variable => "variable",
                                baml_lsp_actions::SemanticTokenType::Property => "property",
                                baml_lsp_actions::SemanticTokenType::EnumMember => "enumMember",
                                baml_lsp_actions::SemanticTokenType::Event => "event",
                                baml_lsp_actions::SemanticTokenType::Function => "function",
                                baml_lsp_actions::SemanticTokenType::Method => "method",
                                baml_lsp_actions::SemanticTokenType::Macro => "macro",
                                baml_lsp_actions::SemanticTokenType::Keyword => "keyword",
                                baml_lsp_actions::SemanticTokenType::Modifier => "modifier",
                                baml_lsp_actions::SemanticTokenType::Comment => "comment",
                                baml_lsp_actions::SemanticTokenType::String => "string",
                                baml_lsp_actions::SemanticTokenType::Number => "number",
                                baml_lsp_actions::SemanticTokenType::Regexp => "regexp",
                                baml_lsp_actions::SemanticTokenType::Operator => "operator",
                                baml_lsp_actions::SemanticTokenType::Decorator => "decorator",
                            })
                        })
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
        let project = self.get_or_create_project(root_path.clone())?;

        let lenses = {
            let project = project.project.try_lock_db()?;
            let lsp_db = project.db();
            let Some(project) = project.project() else {
                return Ok(None);
            };
            let functions = baml_project::list_functions(lsp_db, project);
            functions
                .into_iter()
                .filter_map(|func| {
                    let source_file = lsp_db.get_file(&func.file_path)?;
                    let text = source_file.text(lsp_db.db());
                    // TODO: we use two different span_to_lsp_range functions here (we should reduce to use the same one)
                    let range = baml_project::position::span_to_lsp_range(text, &func.span);

                    Some(CodeLens {
                        range,
                        command: Some(
                            super::commands::OpenBamlPanel {
                                project_path: Some(root_path.as_str().to_string()),
                                function_name: Some(func.name),
                            }
                            .to_lsp_command(),
                        ),
                        data: None,
                    })
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
        let Some(baml_project) = project.project() else {
            return Ok(None);
        };
        let Some(source_file) = lsp_db.get_file(std::path::Path::new(path.as_str())) else {
            return Ok(None);
        };

        // Get hints and filter to the requested range before converting to LSP types.
        let hints = baml_lsp_actions::inlay_hints::inlay_hints(lsp_db, source_file, baml_project);
        let text = source_file.text(lsp_db);
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
        let lsp_hints = hints
            .into_iter()
            .filter(|h| h.offset >= range_start && h.offset < range_end) // Constrict to the requested range
            .map(|h| {
                let label = lsp_types::InlayHintLabel::LabelParts(
                    h.label
                        .into_iter()
                        .map(|part| {
                            let location = part.target.and_then(|t| {
                                let uri = wasm_helpers::from_file_path(&t.file_path).ok()?;
                                let target_file_id = lsp_db.path_to_file_id(&t.file_path)?;
                                let target_source = lsp_db.get_file_by_id(target_file_id)?;
                                let target_text = target_source.text(lsp_db);
                                let range =
                                    baml_project::position::span_to_lsp_range(target_text, &t.span);
                                Some(lsp_types::Location { uri, range })
                            });

                            lsp_types::InlayHintLabelPart {
                                value: part.value,
                                tooltip: None, // Nothing here at least for now
                                location,
                                command: None,
                            }
                        })
                        .collect(),
                );
                lsp_types::InlayHint {
                    position: baml_project::position::offset_to_lsp_position(
                        text,
                        usize::from(h.offset),
                    ),
                    label,
                    kind: h.kind.map(|k| match k {
                        baml_lsp_actions::inlay_hints::InlayHintKind::Parameter => {
                            lsp_types::InlayHintKind::PARAMETER
                        }
                        baml_lsp_actions::inlay_hints::InlayHintKind::Type => {
                            lsp_types::InlayHintKind::TYPE
                        }
                    }),
                    padding_left: Some(h.padding_left),
                    padding_right: Some(h.padding_right),
                    text_edits: if h.text_edits.is_empty() {
                        None
                    } else {
                        Some(
                            h.text_edits
                                .iter()
                                .map(|edit| {
                                    let pos = baml_project::position::offset_to_lsp_position(
                                        text,
                                        usize::from(edit.offset),
                                    );
                                    lsp_types::TextEdit {
                                        range: lsp_types::Range::new(pos, pos),
                                        new_text: edit.new_text.clone(),
                                    }
                                })
                                .collect(),
                        )
                    },
                    tooltip: None,
                    data: None,
                }
            })
            .collect();

        Ok(Some(lsp_hints))
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

        // Get the semantic tokens, this function always returns tokens in document order.
        let tokens = baml_lsp_actions::semantic_tokens(lsp_db, source_file);

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
        let project = self.get_or_create_project(root_path.clone())?;

        let first_function = {
            let project = project.project.try_lock_db()?;
            let lsp_db = project.db();
            let Some(project) = project.project() else {
                return Ok(None);
            };
            let functions = baml_project::list_functions(lsp_db, project);
            functions.into_iter().take(1).map(|f| f.name).next()
        };

        Ok(Some(vec![lsp_types::CodeActionOrCommand::CodeAction(
            super::commands::OpenBamlPanel {
                project_path: Some(root_path.as_str().to_string()),
                function_name: first_function,
            }
            .to_lsp_code_action(),
        )]))
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

        let completions = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, project, offset| {
                baml_lsp_actions::complete(db, source_file, project, offset)
            },
        )?;

        // Convert to LSP types
        let items: Vec<_> = completions
            .into_iter()
            .map(|item| lsp_types::CompletionItem {
                label: item.label,
                kind: Some(match item.kind {
                    baml_lsp_actions::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                    baml_lsp_actions::CompletionKind::Function => CompletionItemKind::FUNCTION,
                    baml_lsp_actions::CompletionKind::Class => CompletionItemKind::CLASS,
                    baml_lsp_actions::CompletionKind::Enum => CompletionItemKind::ENUM,
                    baml_lsp_actions::CompletionKind::EnumVariant => {
                        CompletionItemKind::ENUM_MEMBER
                    }
                    baml_lsp_actions::CompletionKind::Field => CompletionItemKind::FIELD,
                    baml_lsp_actions::CompletionKind::Client => CompletionItemKind::MODULE,
                    baml_lsp_actions::CompletionKind::TypeAlias => {
                        CompletionItemKind::TYPE_PARAMETER
                    }
                    baml_lsp_actions::CompletionKind::Property => CompletionItemKind::PROPERTY,
                    baml_lsp_actions::CompletionKind::Snippet => CompletionItemKind::SNIPPET,
                    baml_lsp_actions::CompletionKind::Generator => CompletionItemKind::MODULE,
                    baml_lsp_actions::CompletionKind::Test => CompletionItemKind::METHOD,
                    baml_lsp_actions::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
                    baml_lsp_actions::CompletionKind::TemplateString => {
                        CompletionItemKind::FUNCTION
                    }
                }),
                detail: item.detail,
                insert_text: item.insert_text,
                sort_text: item.sort_text,
                documentation: item.documentation.map(lsp_types::Documentation::String),
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
        let hover_result = self.compute_on_position(
            &params.text_document_position_params,
            |db, source_file, project, offset| {
                baml_lsp_actions::hover::hover(db, source_file, project, offset)
            },
        )?;

        match hover_result {
            Some(hover) => {
                let content = hover.display(baml_lsp_actions::MarkupKind::Markdown);
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
        let definition = self.compute_on_position(
            &params.text_document_position_params,
            |db, source_file, _, offset| {
                baml_lsp_actions::goto_definition::goto_definition(
                    db,
                    source_file.file_id(db),
                    offset,
                )
                .map(|def| {
                    let Ok(target_uri) = wasm_helpers::from_file_path(&def.file_path) else {
                        return Err(LspError::UnknownErrorCode(
                            "Failed to convert path to URI".to_string(),
                        ));
                    };
                    let Some(target_file_id) = db.path_to_file_id(&def.file_path) else {
                        return Err(LspError::InvalidPath {
                            path: def.file_path,
                            message: "File not found".to_string(),
                        });
                    };
                    let Some(target_source) = db.get_file_by_id(target_file_id) else {
                        return Err(LspError::InvalidPath {
                            path: def.file_path,
                            message: "File not found".to_string(),
                        });
                    };
                    let target_text = target_source.text(db);

                    let target_range =
                        baml_project::position::span_to_lsp_range(target_text, &def.span);
                    Ok(lsp_types::GotoDefinitionResponse::Scalar(
                        lsp_types::Location {
                            uri: target_uri,
                            range: target_range,
                        },
                    ))
                })
            },
        )?;

        definition.transpose()
    }

    fn on_request_text_document_references(
        &self,
        params: lsp_request_params!("textDocument/references"),
    ) -> Result<lsp_request_result!("textDocument/references"), LspError> {
        let references = self.compute_on_position(
            &params.text_document_position,
            |db, source_file, _, offset| {
                baml_lsp_actions::find_all_references(db, source_file.file_id(db), offset)
                    .into_iter()
                    .map(|reference| {
                        let Ok(target_uri) = wasm_helpers::from_file_path(&reference.file_path)
                        else {
                            return Err(LspError::UnknownErrorCode(
                                "Failed to convert path to URI".to_string(),
                            ));
                        };
                        let Some(target_file_id) = db.path_to_file_id(&reference.file_path) else {
                            return Err(LspError::InvalidPath {
                                path: reference.file_path,
                                message: "File not found".to_string(),
                            });
                        };
                        let Some(target_source) = db.get_file_by_id(target_file_id) else {
                            return Err(LspError::InvalidPath {
                                path: reference.file_path,
                                message: "File not found".to_string(),
                            });
                        };
                        let target_text = target_source.text(db);

                        let target_range =
                            baml_project::position::span_to_lsp_range(target_text, &reference.span);

                        Ok(lsp_types::Location {
                            uri: target_uri,
                            range: target_range,
                        })
                    })
                    .collect::<Result<Vec<_>, LspError>>()
            },
        )?;

        match references {
            Ok(references) if !references.is_empty() => Ok(Some(references)),
            Ok(_) => Ok(None),
            Err(e) => Err(e),
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
        let query = params.query.to_lowercase();
        let mut symbols = Vec::new();

        let projects = self.projects.lock().unwrap();
        for project_handle in projects.values() {
            let Ok(db_guard) = project_handle.project.try_lock_db() else {
                continue;
            };
            let lsp_db = db_guard.db();
            let Some(project) = db_guard.project() else {
                continue;
            };

            let all_symbols = std::iter::empty()
                .chain(baml_project::list_functions(lsp_db, project))
                .chain(baml_project::list_classes(lsp_db, project))
                .chain(baml_project::list_enums(lsp_db, project))
                .chain(baml_project::list_type_aliases(lsp_db, project))
                .chain(baml_project::list_clients(lsp_db, project))
                .chain(baml_project::list_tests(lsp_db, project))
                .chain(baml_project::list_generators(lsp_db, project));

            for sym in all_symbols {
                if !query.is_empty() && !sym.name.to_lowercase().contains(&query) {
                    continue;
                }
                let Ok(uri) = wasm_helpers::from_file_path(&sym.file_path) else {
                    continue;
                };
                let range = lsp_db
                    .get_file(&sym.file_path)
                    .map(|source_file| {
                        let text = source_file.text(lsp_db.db());
                        baml_project::position::span_to_lsp_range(text, &sym.span)
                    })
                    .unwrap_or_default();

                symbols.push(lsp_types::WorkspaceSymbol {
                    name: sym.name,
                    kind: to_lsp_symbol_kind(sym.kind),
                    tags: None,
                    container_name: None,
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
        fn convert_symbol(
            sym: &baml_db::baml_compiler_hir::FileSymbol,
            text: &str,
        ) -> lsp_types::DocumentSymbol {
            let range = baml_project::position::text_range_to_lsp_range(text, sym.range);
            let selection_range =
                baml_project::position::text_range_to_lsp_range(text, sym.selection_range);

            let children = if sym.children.is_empty() {
                None
            } else {
                Some(
                    sym.children
                        .iter()
                        .map(|child| convert_symbol(child, text))
                        .collect(),
                )
            };

            #[allow(deprecated)]
            lsp_types::DocumentSymbol {
                name: sym.name.clone(),
                kind: to_lsp_symbol_kind(sym.kind),
                detail: None,
                tags: None,
                deprecated: None,
                range,
                selection_range,
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
        let file_symbols = baml_db::baml_compiler_hir::list_file_symbols(lsp_db, source_file);

        let symbols: Vec<_> = file_symbols
            .iter()
            .map(|sym| convert_symbol(sym, text))
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

fn to_lsp_symbol_kind(kind: baml_project::SymbolKind) -> lsp_types::SymbolKind {
    use baml_project::SymbolKind;
    match kind {
        SymbolKind::Function => lsp_types::SymbolKind::FUNCTION,
        SymbolKind::Class => lsp_types::SymbolKind::CLASS,
        SymbolKind::Enum => lsp_types::SymbolKind::ENUM,
        SymbolKind::TypeAlias => lsp_types::SymbolKind::CLASS,
        SymbolKind::Client => lsp_types::SymbolKind::STRUCT,
        SymbolKind::Test => lsp_types::SymbolKind::METHOD,
        SymbolKind::Generator => lsp_types::SymbolKind::INTERFACE,
        SymbolKind::TemplateString => lsp_types::SymbolKind::FUNCTION,
        SymbolKind::RetryPolicy => lsp_types::SymbolKind::STRUCT,
        SymbolKind::Field => lsp_types::SymbolKind::FIELD,
        SymbolKind::EnumVariant => lsp_types::SymbolKind::ENUM_MEMBER,
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
        let offset = text_size::TextSize::from(
            u32::try_from(baml_project::position::lsp_position_to_offset(
                text, &position,
            ))
            .unwrap_or(0),
        );

        Ok(op(lsp_db, source_file, project, offset))
    }
}
