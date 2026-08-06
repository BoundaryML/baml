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
                let session = match self.workspace.open(&root, files, target) {
                    Ok(session) => session,
                    Err(error) => return tooling_error(&error),
                };
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
                    let path = match resolve_entry(&request.specifier, &request.importer) {
                        Ok(path) => path,
                        Err(message) => return error_response("unresolved_specifier", message),
                    };
                    // An unknown entry must fail loudly: filtering the project
                    // exports down to zero items would hand the editor an empty
                    // declaration while the bundler build succeeds, hiding the
                    // resolution bug from the user.
                    if !session.has_file(&path) {
                        return error_response(
                            "unknown_file",
                            ToolingError::UnknownFile(path).to_string(),
                        );
                    }
                    EntrySpec::File(path)
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
                        id: format!("\0baml:{}:runtime", session.project_id()),
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
            Some(Request::Close(request)) => {
                // Drop both halves of the mapping: the session (and with it
                // the compiler database) and the id -> root entry that would
                // otherwise keep resolving to a root with nothing behind it.
                let released = match self.roots.remove(&request.project_id) {
                    Some(root) => self.workspace.close(&root),
                    None => false,
                };
                Response::Closed(proto::ProjectClosed { released })
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

/// Resolves a module specifier against its importer. Only absolute and
/// explicitly relative (`./`, `../`) specifiers are meaningful here: bare
/// specifiers (`dep/baml_src/widget.baml`) require Node's package resolution
/// (`node_modules` walk-up, pnpm symlinks), which only the TypeScript host can
/// perform — hosts must resolve those to absolute paths before calling.
fn resolve_entry(specifier: &str, importer: &str) -> Result<PathBuf, String> {
    // `Path::is_absolute` is compile-time platform-dependent and returns false
    // for `/…` paths under the wasm32 host, so the leading-`/` check is a
    // string test; `is_absolute` still covers drive-letter paths on Windows.
    let is_absolute = specifier.starts_with('/') || std::path::Path::new(specifier).is_absolute();
    let joined = if is_absolute {
        PathBuf::from(specifier)
    } else if specifier.starts_with('.') {
        PathBuf::from(importer)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(specifier)
    } else {
        return Err(format!(
            "bare specifier {specifier} must be resolved to an absolute path by the host before emission",
        ));
    };
    Ok(crate::normalize_lexical(&joined))
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

    fn open_project(
        protocol: &mut ToolingProtocol,
        root: &std::path::Path,
        path: &std::path::Path,
        source: &str,
    ) -> String {
        let request = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Open(proto::ProjectOpen {
                project_root: display(root),
                files: vec![proto::SourceFile {
                    path: display(path),
                    text: source.into(),
                }],
                target: "node".into(),
            })),
        };
        let response =
            proto::ToolingResponse::decode(protocol.dispatch(&request.encode_to_vec()).as_slice())
                .unwrap();
        let Some(Response::Project(state)) = response.response else {
            panic!("open failed: {response:?}");
        };
        state.project_id
    }

    fn module_declaration(
        protocol: &mut ToolingProtocol,
        project_id: &str,
        specifier: &str,
        importer: &std::path::Path,
    ) -> String {
        let response = module_response(protocol, project_id, specifier, importer);
        let Response::Module(module) = response else {
            panic!("module request failed: {response:?}");
        };
        module.declaration
    }

    fn module_response(
        protocol: &mut ToolingProtocol,
        project_id: &str,
        specifier: &str,
        importer: &std::path::Path,
    ) -> proto::tooling_response::Response {
        let request = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Module(
                proto::ModuleRequest {
                    project_id: project_id.to_string(),
                    specifier: specifier.into(),
                    importer: display(importer),
                },
            )),
        };
        let response =
            proto::ToolingResponse::decode(protocol.dispatch(&request.encode_to_vec()).as_slice())
                .unwrap();
        response.response.expect("module response is present")
    }

    #[test]
    fn unknown_file_entries_fail_loudly_instead_of_emitting_empty_modules() {
        // A file the session never saw (e.g. a host resolving a bare
        // specifier to the wrong location) must produce an error, not an
        // empty declaration that renders as a silent blank in the editor.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        std::fs::write(&path, "class Person { name string }\n").unwrap();
        let mut protocol = ToolingProtocol::default();
        let project_id = open_project(
            &mut protocol,
            dir.path(),
            &path,
            "class Person { name string }\n",
        );
        let response = module_response(
            &mut protocol,
            &project_id,
            "./ghost.baml",
            &dir.path().join("index.ts"),
        );
        let Some(Response::Error(error)) = Some(response) else {
            panic!("unknown entry must error");
        };
        assert_eq!(error.code, "unknown_file");
        assert!(error.message.contains("ghost.baml"));
    }

    #[test]
    fn bare_module_specifiers_are_rejected_for_the_host_to_resolve() {
        // Node package resolution (node_modules walk-up, pnpm symlinks) is
        // only possible host-side; joining a bare specifier onto the importer
        // directory would silently target a nonexistent path.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        std::fs::write(&path, "class Person { name string }\n").unwrap();
        let mut protocol = ToolingProtocol::default();
        let project_id = open_project(
            &mut protocol,
            dir.path(),
            &path,
            "class Person { name string }\n",
        );
        let response = module_response(
            &mut protocol,
            &project_id,
            "baml-dep/baml_src/widget.baml",
            &dir.path().join("index.ts"),
        );
        let Some(Response::Error(error)) = Some(response) else {
            panic!("bare specifier must error");
        };
        assert_eq!(error.code, "unresolved_specifier");
        assert!(error.message.contains("baml-dep/baml_src/widget.baml"));
    }

    #[test]
    fn absolute_specifiers_pass_through_on_every_host() {
        // Regression: `Path::is_absolute` returns false for `/…` under the
        // wasm32 host, so the absolute check must be a string test — an
        // absolute specifier must reach the session unchanged everywhere.
        let resolved = resolve_entry("/dep/baml_src/widget.baml", "/app/src/index.ts").unwrap();
        assert_eq!(resolved, PathBuf::from("/dep/baml_src/widget.baml"));
    }

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
    fn project_open_rejects_snapshots_carrying_non_baml_files() {
        // The protocol is public: a native or WASM client can put `App.tsx`
        // in the opening snapshot next to valid BAML. That must fail with
        // NonBamlPath and leave no session behind, rather than opening a
        // project whose diagnostics are TSX-parsed-as-BAML noise.
        let dir = tempfile::tempdir().unwrap();
        let baml = dir.path().join("main.baml");
        let tsx = dir.path().join("App.tsx");
        let tsx_text = "export default function App() { return null }\n";
        std::fs::write(&baml, "class Person { name string }\n").unwrap();
        std::fs::write(&tsx, tsx_text).unwrap();
        let request = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Open(proto::ProjectOpen {
                project_root: display(dir.path()),
                files: vec![
                    proto::SourceFile {
                        path: display(&baml),
                        text: "class Person { name string }\n".into(),
                    },
                    proto::SourceFile {
                        path: display(&tsx),
                        text: tsx_text.into(),
                    },
                ],
                target: "node".into(),
            })),
        };
        let mut protocol = ToolingProtocol::default();
        let response =
            proto::ToolingResponse::decode(protocol.dispatch(&request.encode_to_vec()).as_slice())
                .unwrap();
        match response.response {
            Some(Response::Error(error)) => {
                assert_eq!(error.code, "tooling_error");
                assert!(error.message.contains("App.tsx"), "{}", error.message);
            }
            other => panic!("a snapshot carrying App.tsx must not open: {other:?}"),
        }
        assert!(protocol.workspace.get(dir.path()).is_none());
    }

    #[test]
    fn relative_module_specifiers_are_normalized_against_the_importer() {
        // `./main.baml` joins to `<dir>/./main.baml`; the lookup must still
        // hit the canonical project source.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        std::fs::write(&path, "class Person { name string }\n").unwrap();
        let mut protocol = ToolingProtocol::default();
        let project_id = open_project(
            &mut protocol,
            dir.path(),
            &path,
            "class Person { name string }\n",
        );
        let declaration = module_declaration(
            &mut protocol,
            &project_id,
            "./main.baml",
            &dir.path().join("index.ts"),
        );
        assert!(declaration.contains("export interface Person"));
    }

    #[test]
    fn protocol_instances_isolate_same_root_sessions() {
        // Each tooling protocol instance owns its sessions: two clients
        // serving the same root (e.g. two WASM instances in one process)
        // must never observe each other's overlays.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        std::fs::write(&path, "class One {}\n").unwrap();
        let mut first = ToolingProtocol::default();
        let mut second = ToolingProtocol::default();
        let first_id = open_project(&mut first, dir.path(), &path, "class One {}\n");
        let second_id = open_project(&mut second, dir.path(), &path, "class One {}\n");

        let update = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Update(
                proto::ProjectUpdate {
                    project_id: first_id.clone(),
                    file: Some(proto::SourceFile {
                        path: display(&path),
                        text: "class Two {}\n".into(),
                    }),
                    version: 1,
                    remove: false,
                },
            )),
        };
        first.dispatch(&update.encode_to_vec());

        assert!(
            module_declaration(&mut first, &first_id, "./main.baml", &path)
                .contains("interface Two")
        );
        assert!(
            module_declaration(&mut second, &second_id, "./main.baml", &path)
                .contains("interface One")
        );
    }

    fn close_project(protocol: &mut ToolingProtocol, project_id: &str) -> bool {
        let request = proto::ToolingRequest {
            request: Some(proto::tooling_request::Request::Close(
                proto::ProjectRequest {
                    project_id: project_id.to_string(),
                },
            )),
        };
        let response =
            proto::ToolingResponse::decode(protocol.dispatch(&request.encode_to_vec()).as_slice())
                .unwrap();
        let Some(Response::Closed(closed)) = response.response else {
            panic!("close failed: {response:?}");
        };
        closed.released
    }

    #[test]
    fn closing_a_project_releases_its_session() {
        // `dispose()` on either host must actually drop the compiler database
        // rather than advertise a lifecycle it never performs: a superseded
        // lane would otherwise survive every config-driven reopen for the
        // life of the process.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        std::fs::write(&path, "class Person { name string }\n").unwrap();
        let mut protocol = ToolingProtocol::default();
        let project_id = open_project(
            &mut protocol,
            dir.path(),
            &path,
            "class Person { name string }\n",
        );
        assert!(protocol.workspace.get(dir.path()).is_some());

        assert!(close_project(&mut protocol, &project_id));
        // The session is gone from the workspace, not merely unreachable by
        // id, so the database it owned is dropped.
        assert!(protocol.workspace.get(dir.path()).is_none());

        // A released project id is dead: further work fails loudly instead of
        // silently resurrecting a session.
        let Response::Error(error) =
            module_response(&mut protocol, &project_id, "./main.baml", &path)
        else {
            panic!("a closed project must not serve modules");
        };
        assert_eq!(error.code, "unknown_project");

        // Closing twice is a no-op, not an error: a host disposing a lane it
        // already replaced must not have to track which is which.
        assert!(!close_project(&mut protocol, &project_id));

        // The root is reusable afterwards — close released the session, it
        // did not poison the root.
        let reopened = open_project(
            &mut protocol,
            dir.path(),
            &path,
            "class Person { name string }\n",
        );
        assert!(
            module_declaration(&mut protocol, &reopened, "./main.baml", &path)
                .contains("interface Person")
        );
    }
}
