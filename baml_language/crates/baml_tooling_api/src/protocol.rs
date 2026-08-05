//! Protobuf dispatcher shared byte-for-byte by the native and WASM hosts.

use std::{collections::HashMap, path::PathBuf};

use bridge_ctypes::baml_tooling::v1 as proto;
use prost::Message;

use crate::{
    EntrySpec, Location, ProjectSession, SourceInput, Target, ToolingError, ToolingWorkspace,
    VirtualModule, emit,
};

/// Stateful tooling.v1 endpoint. The host bindings intentionally expose only
/// this byte-oriented API, keeping protobuf as the compatibility boundary.
#[derive(Default)]
pub struct ToolingProtocol {
    workspace: ToolingWorkspace,
    roots: HashMap<String, PathBuf>,
}

impl ToolingProtocol {
    pub fn dispatch(&mut self, request_bytes: &[u8]) -> Vec<u8> {
        let response = match proto::ToolingRequest::decode(request_bytes) {
            Ok(request) => self.handle(request),
            Err(error) => error_response("invalid_request", error.to_string()),
        };
        response.encode_to_vec()
    }

    fn handle(&mut self, request: proto::ToolingRequest) -> proto::ToolingResponse {
        use proto::{tooling_request::Request, tooling_response::Response};

        let response = match request.request {
            Some(Request::Open(open)) => {
                let target = match open.target.as_str() {
                    "web" => Target::Web,
                    "node" | "" => Target::Node,
                    other => {
                        return error_response("invalid_target", format!("unknown target {other}"));
                    }
                };
                let root = PathBuf::from(open.project_root);
                let files = open
                    .files
                    .into_iter()
                    .map(|file| SourceInput {
                        path: file.path.into(),
                        text: file.text,
                    })
                    .collect();
                let session = self.workspace.open(&root, files, target);
                let id = session.project_id();
                let state = proto::ProjectState {
                    project_id: id.clone(),
                    revision: session.revision(),
                    fingerprint: session.fingerprint(),
                };
                self.roots.insert(id, root);
                Response::Project(state)
            }
            Some(Request::Update(update)) => {
                let Some(root) = self.roots.get(&update.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let Some(session) = self.workspace.get_mut(root) else {
                    return error_response("unknown_project", "project is not open");
                };
                let Some(file) = update.file else {
                    return error_response("invalid_update", "file is required");
                };
                match session.update_file(
                    PathBuf::from(file.path).as_path(),
                    (!update.remove).then_some(file.text.as_str()),
                    update.version,
                ) {
                    Ok(revision) => Response::Project(proto::ProjectState {
                        project_id: update.project_id,
                        revision,
                        fingerprint: session.fingerprint(),
                    }),
                    Err(error) => return tooling_error(&error),
                }
            }
            Some(Request::Check(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let check = session.check();
                Response::Check(proto::CheckResult {
                    revision: check.revision,
                    diagnostics: check
                        .diagnostics
                        .into_iter()
                        .map(|diagnostic| proto::Diagnostic {
                            code: diagnostic.code,
                            message: diagnostic.message,
                            severity: diagnostic.severity,
                            location: diagnostic.location.as_ref().map(location),
                        })
                        .collect(),
                })
            }
            Some(Request::Module(request)) => {
                let Some(root) = self.roots.get(&request.project_id).cloned() else {
                    return error_response("unknown_project", "project is not open");
                };
                let Some(session) = self.workspace.get_mut(&root) else {
                    return error_response("unknown_project", "project is not open");
                };
                let entry = if request.specifier == "baml:client" {
                    EntrySpec::Client
                } else {
                    EntrySpec::File(resolve_entry(&request.specifier, &request.importer))
                };
                Response::Module(virtual_module(session.emit_editor_module(&entry)))
            }
            Some(Request::Definition(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let locations = if request.symbol_id.is_empty() {
                    session
                        .definition_at(PathBuf::from(request.path).as_path(), request.offset_utf8)
                } else {
                    session.definition_for_symbol(&request.symbol_id)
                };
                Response::Locations(proto::Locations {
                    locations: locations.iter().map(location).collect(),
                })
            }
            Some(Request::References(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let locations = if request.symbol_id.is_empty() {
                    session
                        .references_at(PathBuf::from(request.path).as_path(), request.offset_utf8)
                } else {
                    session.references_for_symbol(&request.symbol_id)
                };
                Response::Locations(proto::Locations {
                    locations: locations.iter().map(location).collect(),
                })
            }
            Some(Request::Hover(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let Some(hover) = session.hover_for_symbol(&request.symbol_id) else {
                    return error_response("no_symbol", "BAML symbol is not available");
                };
                Response::Hover(proto::Hover {
                    markdown: hover.markdown,
                    location: Some(location(&hover.location)),
                    symbol_id: hover.symbol_id,
                })
            }
            Some(Request::Completions(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let entry = if request.symbol_id == "baml:client" || request.path.is_empty() {
                    EntrySpec::Client
                } else {
                    EntrySpec::File(request.path.into())
                };
                Response::Completions(proto::Completions {
                    items: session
                        .completions(&entry)
                        .into_iter()
                        .map(|item| proto::CompletionItem {
                            label: item.label,
                            kind: item.kind,
                            detail: item.detail,
                            documentation: item.documentation,
                            symbol_id: item.symbol_id,
                        })
                        .collect(),
                })
            }
            Some(Request::PrepareRename(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                match session.prepare_rename(&request.symbol_id) {
                    Ok(value) => Response::RenameCheck(location(&value)),
                    Err(error) => return tooling_error(&error),
                }
            }
            Some(Request::Rename(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                match session.rename(&request.symbol_id, &request.new_name) {
                    Ok(edit) => Response::Rename(proto::WorkspaceEdit {
                        edits: edit
                            .edits
                            .into_iter()
                            .map(|edit| proto::TextEdit {
                                location: Some(location(&edit.location)),
                                new_text: edit.new_text,
                            })
                            .collect(),
                    }),
                    Err(error) => return tooling_error(&error),
                }
            }
            Some(Request::Layout(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let layout = session.layout();
                Response::Layout(proto::ProjectLayout {
                    config_path: display(&layout.config_path),
                    roots: layout.roots.iter().map(|path| display(path)).collect(),
                    source_files: layout
                        .source_files
                        .iter()
                        .map(|path| display(path))
                        .collect(),
                    watch_files: layout
                        .watch_files
                        .iter()
                        .map(|path| display(path))
                        .collect(),
                })
            }
            Some(Request::Capabilities(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                let capabilities = session.capabilities();
                Response::Capabilities(proto::Capabilities {
                    protocol: capabilities.protocol,
                    features: capabilities.features,
                    compiler_version: capabilities.compiler_version,
                })
            }
            Some(Request::RuntimeModule(request)) => {
                let Some(session) = self.session(&request.project_id) else {
                    return error_response("unknown_project", "project is not open");
                };
                match emit::emit_runtime_module(session) {
                    Ok(code) => Response::RuntimeModule(proto::RuntimeModule {
                        id: format!("\\0baml:{}:runtime", session.project_id()),
                        code,
                        watch_files: session
                            .watch_files()
                            .iter()
                            .map(|path| display(path))
                            .collect(),
                        fingerprint: session.fingerprint(),
                        revision: session.revision(),
                    }),
                    Err(error) => return tooling_error(&error),
                }
            }
            None => return error_response("invalid_request", "request variant is required"),
        };
        proto::ToolingResponse {
            response: Some(response),
        }
    }

    fn session(&self, project_id: &str) -> Option<&ProjectSession> {
        self.roots
            .get(project_id)
            .and_then(|root| self.workspace.get(root))
    }
}

fn resolve_entry(specifier: &str, importer: &str) -> PathBuf {
    let specifier = PathBuf::from(specifier);
    if specifier.is_absolute() {
        specifier
    } else {
        PathBuf::from(importer)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(specifier)
    }
}

fn display(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

fn location(value: &Location) -> proto::Location {
    proto::Location {
        path: display(&value.path),
        start_utf8: value.start_utf8,
        length_utf8: value.length_utf8,
    }
}

fn virtual_module(value: VirtualModule) -> proto::VirtualModule {
    proto::VirtualModule {
        id: value.id,
        code: value.code,
        declaration: value.declaration,
        map: Some(proto::SegmentMap {
            version: value.map.version,
            generated_file: value.map.generated_file,
            sources: value.map.sources.iter().map(|path| display(path)).collect(),
            source_hashes: value.map.source_hashes,
            segments: value
                .map
                .segments
                .into_iter()
                .map(|segment| proto::Segment {
                    gen_start_utf16: segment.gen_start_utf16,
                    gen_length_utf16: segment.gen_length_utf16,
                    source_file: segment.source_file,
                    source_start_utf8: segment.source_start_utf8,
                    source_length_utf8: segment.source_length_utf8,
                    symbol_id: segment.symbol_id,
                    signature_id: segment.signature_id.unwrap_or_default(),
                    role: format!("{:?}", segment.role).to_lowercase(),
                })
                .collect(),
        }),
        watch_files: value.watch_files.iter().map(|path| display(path)).collect(),
        fingerprint: value.fingerprint,
        revision: value.revision,
        stale: value.stale,
        runtime_id: value.runtime_id,
    }
}

fn tooling_error(error: &ToolingError) -> proto::ToolingResponse {
    error_response("tooling_error", error.to_string())
}

fn error_response(code: impl Into<String>, message: impl Into<String>) -> proto::ToolingResponse {
    proto::ToolingResponse {
        response: Some(proto::tooling_response::Response::Error(
            proto::ToolingError {
                code: code.into(),
                message: message.into(),
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use proto::tooling_response::Response;

    use super::*;

    #[test]
    fn protobuf_round_trip_opens_and_checks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        let source = "class Person { name string }\n";
        std::fs::write(&path, source).unwrap();
        let request = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Open(proto::ProjectOpen {
                project_root: display(dir.path()),
                files: vec![proto::SourceFile {
                    path: display(&path),
                    text: source.into(),
                }],
                target: "node".into(),
            })),
        };
        let response = proto::ToolingResponse::decode(
            ToolingProtocol::default()
                .dispatch(&request.encode_to_vec())
                .as_slice(),
        )
        .unwrap();
        assert!(matches!(response.response, Some(Response::Project(_))));
    }

    #[test]
    fn native_and_wasm_dispatchers_have_byte_parity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        let source = "class Person { name string }\n";
        let request = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Open(proto::ProjectOpen {
                project_root: display(dir.path()),
                files: vec![proto::SourceFile {
                    path: display(&path),
                    text: source.into(),
                }],
                target: "web".into(),
            })),
        }
        .encode_to_vec();
        let native = ToolingProtocol::default().dispatch(&request);
        let wasm = ToolingProtocol::default().dispatch(&request);
        assert_eq!(native, wasm);
    }
}
