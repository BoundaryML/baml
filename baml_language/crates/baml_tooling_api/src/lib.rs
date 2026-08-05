//! Host-neutral TypeScript tooling over the canonical BAML compiler database.
//!
//! Hosts provide complete source snapshots. This crate never reparses BAML or
//! guesses symbol identity: diagnostics and navigation come from
//! `ProjectDatabase`, `baml_lsp2_actions`, and `baml_surface`.

mod emit;
mod protocol;

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    str::FromStr,
};

use baml_base::SourceFile;
use baml_db::baml_compiler_diagnostics::Severity;
use baml_lsp2_actions::{definition_at, usages_at};
use baml_project::ProjectDatabase;
use baml_surface::{Package, Resolved, SymbolId};
pub use emit::{EntrySpec, GeneratedFile, Segment, SegmentMap, SegmentRole, Target, VirtualModule};
pub use protocol::ToolingProtocol;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use text_size::TextSize;

pub const PROTOCOL_VERSION: &str = "baml.tooling.v1";
pub const TYPESCRIPT_IMPORTS_CAPABILITY: &str = "typescriptImports.v1";
pub const RENAME_CAPABILITY: &str = "rename.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub protocol: String,
    pub compiler_version: String,
    pub features: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            protocol: PROTOCOL_VERSION.to_string(),
            compiler_version: baml_version::CANONICAL_VERSION.to_string(),
            features: vec![
                TYPESCRIPT_IMPORTS_CAPABILITY.to_string(),
                RENAME_CAPABILITY.to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInput {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: PathBuf,
    pub start_utf8: u32,
    pub length_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolingDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub revision: u64,
    pub diagnostics: Vec<ToolingDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLayout {
    pub config_path: PathBuf,
    pub roots: Vec<PathBuf>,
    pub source_files: Vec<PathBuf>,
    pub watch_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub markdown: String,
    pub location: Location,
    pub symbol_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: String,
    pub documentation: String,
    pub symbol_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub location: Location,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolingError {
    #[error("stale update for {path}: version {received} is not newer than {current}")]
    StaleUpdate {
        path: PathBuf,
        received: u64,
        current: u64,
    },
    #[error("BAML file is not in this project: {0}")]
    UnknownFile(PathBuf),
    #[error("BAML symbol is not available at this position")]
    NoSymbol,
    #[error("unsupported rename: {0}")]
    UnsupportedRename(String),
    #[error("invalid BAML identifier: {0}")]
    InvalidIdentifier(String),
    #[error("rename would collide with existing symbol: {0}")]
    RenameCollision(String),
    #[error("BAML project has compiler errors")]
    CompilerErrors,
    #[error("failed to emit BAML bytecode: {0}")]
    Bytecode(String),
}

/// Long-lived compiler session. All offsets crossing this boundary are UTF-8
/// byte offsets, matching the compiler's native spans.
pub struct ProjectSession {
    root: PathBuf,
    config_path: PathBuf,
    target: Target,
    db: ProjectDatabase,
    versions: HashMap<PathBuf, u64>,
    revision: u64,
    last_good: HashMap<String, VirtualModule>,
}

impl ProjectSession {
    pub fn open(root: impl AsRef<Path>, files: Vec<SourceInput>, target: Target) -> Self {
        let root = canonical(root.as_ref());
        let mut db = ProjectDatabase::new();
        db.set_project_root(&root);
        let mut versions = HashMap::new();
        for file in files {
            let path = canonical(&file.path);
            db.add_or_update_file(&path, &file.text);
            versions.insert(path, 0);
        }
        Self {
            config_path: root.join("baml.toml"),
            root,
            target,
            db,
            versions,
            revision: 1,
            last_good: HashMap::new(),
        }
    }

    pub fn update_file(
        &mut self,
        path: &Path,
        text: Option<&str>,
        version: u64,
    ) -> Result<u64, ToolingError> {
        let path = canonical(path);
        let current = self.versions.get(&path).copied().unwrap_or(0);
        if version <= current {
            return Err(ToolingError::StaleUpdate {
                path,
                received: version,
                current,
            });
        }
        match text {
            Some(text) => {
                self.db.add_or_update_file(&path, text);
                self.versions.insert(path, version);
            }
            None => {
                self.db.remove_file(&path);
                self.versions.remove(&path);
            }
        }
        self.revision = self.revision.saturating_add(1);
        Ok(self.revision)
    }

    pub fn check(&self) -> CheckResult {
        let result = self.db.check();
        let diagnostics = result
            .diagnostics
            .into_iter()
            .map(|diagnostic| {
                let location = diagnostic.primary_span().and_then(|span| {
                    result.file_paths.get(&span.file_id).map(|path| Location {
                        path: path.clone(),
                        start_utf8: span.range.start().into(),
                        length_utf8: u32::from(span.range.len()),
                    })
                });
                ToolingDiagnostic {
                    code: diagnostic.code().to_string(),
                    message: diagnostic.message_with_primary_label().into_owned(),
                    severity: match diagnostic.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                        Severity::Info => "info",
                    }
                    .to_string(),
                    location,
                }
            })
            .collect();
        CheckResult {
            revision: self.revision,
            diagnostics,
        }
    }

    pub fn emit_virtual_module(&self, entry: &EntrySpec) -> Result<VirtualModule, ToolingError> {
        if self
            .check()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "error")
        {
            return Err(ToolingError::CompilerErrors);
        }
        Ok(emit::emit_virtual_module(self, entry))
    }

    /// Editor emission preserves a last-known-good declaration across an
    /// invalid intermediate compiler revision.
    pub fn emit_editor_module(&mut self, entry: &EntrySpec) -> VirtualModule {
        let key = entry.cache_key();
        match self.emit_virtual_module(entry) {
            Ok(module) => {
                self.last_good.insert(key, module.clone());
                module
            }
            Err(_) => self.last_good.get(&key).cloned().map_or_else(
                || VirtualModule::poison(key, self.revision, self.fingerprint()),
                |mut module| {
                    module.stale = true;
                    module.revision = self.revision;
                    module
                },
            ),
        }
    }

    pub fn definition_at(&self, path: &Path, offset_utf8: u32) -> Vec<Location> {
        let Some(file) = self.db.get_file(path) else {
            return Vec::new();
        };
        definition_at(&self.db, file, TextSize::from(offset_utf8))
            .into_iter()
            .map(|location| self.location(location.file, location.range))
            .collect()
    }

    pub fn references_at(&self, path: &Path, offset_utf8: u32) -> Vec<Location> {
        let Some(file) = self.db.get_file(path) else {
            return Vec::new();
        };
        usages_at(&self.db, file, TextSize::from(offset_utf8))
            .into_iter()
            .map(|location| self.location(location.file, location.range))
            .collect()
    }

    pub fn definition_for_symbol(&self, symbol_id: &str) -> Vec<Location> {
        self.symbol_location(symbol_id).into_iter().collect()
    }

    pub fn references_for_symbol(&self, symbol_id: &str) -> Vec<Location> {
        let Some(definition) = self.symbol_location(symbol_id) else {
            return Vec::new();
        };
        self.references_at(&definition.path, definition.start_utf8)
    }

    pub fn hover_for_symbol(&self, symbol_id: &str) -> Option<Hover> {
        let id = SymbolId::from_str(symbol_id).ok()?;
        let resolved = id.resolve(&self.db)?;
        let (symbol, documentation) = match resolved {
            Resolved::Symbol(symbol) => (symbol, symbol.docstring(&self.db).unwrap_or_default()),
            Resolved::Member(owner, _) => (owner, owner.docstring(&self.db).unwrap_or_default()),
            Resolved::Package(_) | Resolved::Namespace(_) => return None,
        };
        let location = self.symbol_location(symbol_id)?;
        Some(Hover {
            markdown: format!(
                "```baml\n{} {}\n```\n\n{}",
                symbol.kind().as_str(),
                symbol
                    .name(&self.db)
                    .map_or_else(String::new, |n| n.to_string()),
                documentation
            ),
            location,
            symbol_id: symbol_id.to_string(),
        })
    }

    pub fn completions(&self, entry: &EntrySpec) -> Vec<CompletionItem> {
        self.filtered_exports(entry)
            .items
            .into_iter()
            .filter(|item| !item.synthetic)
            .map(|item| CompletionItem {
                label: item.name,
                kind: item.kind.as_str().to_string(),
                detail: item.id.clone(),
                documentation: item.docstring.unwrap_or_default(),
                symbol_id: item.id,
            })
            .collect()
    }

    pub fn prepare_rename(&self, symbol_id: &str) -> Result<Location, ToolingError> {
        let location = self
            .symbol_location(symbol_id)
            .ok_or(ToolingError::NoSymbol)?;
        let file = self
            .db
            .get_file(&location.path)
            .ok_or(ToolingError::NoSymbol)?;
        baml_lsp2_actions::prepare_rename(&self.db, file, TextSize::from(location.start_utf8))
            .map_err(|error| ToolingError::UnsupportedRename(error.to_string()))?;
        Ok(location)
    }

    pub fn rename(&self, symbol_id: &str, new_name: &str) -> Result<WorkspaceEdit, ToolingError> {
        let definition = self
            .symbol_location(symbol_id)
            .ok_or(ToolingError::NoSymbol)?;
        let file = self
            .db
            .get_file(&definition.path)
            .ok_or(ToolingError::NoSymbol)?;
        let locations = baml_lsp2_actions::rename(
            &self.db,
            file,
            TextSize::from(definition.start_utf8),
            new_name,
        )
        .map_err(|error| match error {
            baml_lsp2_actions::RenameError::InvalidIdentifier(_) => {
                ToolingError::InvalidIdentifier(new_name.to_string())
            }
            baml_lsp2_actions::RenameError::Collision(_) => {
                ToolingError::RenameCollision(new_name.to_string())
            }
            other => ToolingError::UnsupportedRename(other.to_string()),
        })?;
        Ok(WorkspaceEdit {
            edits: locations
                .into_iter()
                .map(|location| TextEdit {
                    location: self.location(location.file, location.range),
                    new_text: new_name.to_string(),
                })
                .collect(),
        })
    }

    pub fn layout(&self) -> ProjectLayout {
        let source_files: Vec<_> = self.db.non_builtin_file_paths().collect();
        let mut watch_files = source_files.clone();
        watch_files.push(self.config_path.clone());
        watch_files.sort();
        watch_files.dedup();
        ProjectLayout {
            config_path: self.config_path.clone(),
            roots: vec![self.root.clone()],
            source_files,
            watch_files,
        }
    }

    pub fn watch_files(&self) -> Vec<PathBuf> {
        self.layout().watch_files
    }

    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(PROTOCOL_VERSION);
        hasher.update(baml_version::CANONICAL_VERSION);
        hasher.update(self.target.as_str());
        if let Ok(config) = std::fs::read(&self.config_path) {
            hasher.update(&config);
        }
        let mut files: BTreeMap<PathBuf, String> = BTreeMap::new();
        for file in self.db.get_source_files() {
            files.insert(file.path(&self.db), file.text(&self.db).clone());
        }
        for (path, text) in files {
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(text.as_bytes());
        }
        hex::encode(hasher.finalize())
    }

    pub fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn project_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.root.to_string_lossy().as_bytes());
        hex::encode(hasher.finalize())[..16].to_string()
    }

    fn location(&self, file: SourceFile, range: text_size::TextRange) -> Location {
        Location {
            path: file.path(&self.db),
            start_utf8: range.start().into(),
            length_utf8: u32::from(range.len()),
        }
    }

    fn symbol_location(&self, symbol_id: &str) -> Option<Location> {
        let id = SymbolId::from_str(symbol_id).ok()?;
        match id.resolve(&self.db)? {
            Resolved::Symbol(symbol) => {
                Some(self.location(symbol.file(&self.db), symbol.name_span(&self.db)?))
            }
            Resolved::Member(owner, member) => {
                Some(self.location(owner.file(&self.db), member.name_span(&self.db)))
            }
            Resolved::Package(_) | Resolved::Namespace(_) => None,
        }
    }

    fn filtered_exports(&self, entry: &EntrySpec) -> baml_surface::PackageExport {
        let mut export = baml_surface::export_package(&self.db, Package::user(&self.db));
        if let EntrySpec::File(path) = entry {
            let path = canonical(path).to_string_lossy().into_owned();
            export.items.retain(|item| {
                canonical(Path::new(&item.source.file))
                    .to_string_lossy()
                    .as_ref()
                    == path
            });
        }
        export
    }
}

/// Sessions keyed by canonical project root for native hosts serving multiple
/// TypeScript projects in one process.
#[derive(Default)]
pub struct ToolingWorkspace {
    sessions: HashMap<PathBuf, ProjectSession>,
}

impl ToolingWorkspace {
    pub fn open(
        &mut self,
        root: impl AsRef<Path>,
        files: Vec<SourceInput>,
        target: Target,
    ) -> &mut ProjectSession {
        let root = canonical(root.as_ref());
        self.sessions
            .insert(root.clone(), ProjectSession::open(&root, files, target));
        self.sessions
            .get_mut(&root)
            .expect("the project session was just inserted")
    }

    pub fn get(&self, root: &Path) -> Option<&ProjectSession> {
        self.sessions.get(&canonical(root))
    }

    pub fn get_mut(&mut self, root: &Path) -> Option<&mut ProjectSession> {
        self.sessions.get_mut(&canonical(root))
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(source: &str) -> (tempfile::TempDir, ProjectSession, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.baml");
        std::fs::write(&path, source).unwrap();
        let session = ProjectSession::open(
            dir.path(),
            vec![SourceInput {
                path: path.clone(),
                text: source.to_string(),
            }],
            Target::Node,
        );
        (dir, session, path)
    }

    #[test]
    fn emits_declarations_and_exact_source_segments() {
        let (_dir, session, path) = session(
            "/// A person.\nclass Person {\n  name string\n}\n\nfunction Greet(p: Person) -> string {\n  \"hi\"\n}\n",
        );
        let module = session
            .emit_virtual_module(&EntrySpec::File(path.clone()))
            .unwrap();
        assert!(module.declaration.contains("export interface Person"));
        assert!(module.declaration.contains("readonly Greet:"));
        let person = module
            .map
            .segments
            .iter()
            .find(|segment| segment.symbol_id == "T:user.Person")
            .unwrap();
        assert_eq!(
            &std::fs::read_to_string(path).unwrap()[person.source_start_utf8 as usize..][..6],
            "Person"
        );
    }

    #[test]
    fn emits_enum_values_in_declarations_and_runtime_modules() {
        let (_dir, session, path) = session("enum Color {\n  Red\n  Blue\n}\n");
        let module = session.emit_virtual_module(&EntrySpec::File(path)).unwrap();
        assert!(module.declaration.contains("export enum Color"));
        assert!(module.code.contains("export const Color = {"));
        assert!(module.code.contains("Red: \"Red\""));
        assert!(module.code.contains("Blue: \"Blue\""));
    }

    #[test]
    fn stale_updates_are_rejected() {
        let (_dir, mut session, path) = session("class One {}\n");
        session
            .update_file(&path, Some("class Two {}\n"), 2)
            .unwrap();
        assert!(matches!(
            session.update_file(&path, Some("class Three {}\n"), 1),
            Err(ToolingError::StaleUpdate { .. })
        ));
    }

    #[test]
    fn fingerprint_changes_with_overlay() {
        let (_dir, mut session, path) = session("class One {}\n");
        let before = session.fingerprint();
        session
            .update_file(&path, Some("class Two {}\n"), 1)
            .unwrap();
        assert_ne!(before, session.fingerprint());
    }

    #[test]
    fn file_entries_filter_project_exports_but_client_is_aggregate() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("one.baml");
        let two = dir.path().join("two.baml");
        let mut session = ProjectSession::open(
            dir.path(),
            vec![
                SourceInput {
                    path: one.clone(),
                    text: "class One {}\n".into(),
                },
                SourceInput {
                    path: two,
                    text: "class Two {}\n".into(),
                },
            ],
            Target::Node,
        );
        let one_module = session.emit_editor_module(&EntrySpec::File(one));
        let client = session.emit_editor_module(&EntrySpec::Client);
        assert!(one_module.declaration.contains("interface One"));
        assert!(!one_module.declaration.contains("interface Two"));
        assert!(client.declaration.contains("interface One"));
        assert!(client.declaration.contains("interface Two"));
    }

    #[test]
    fn unicode_segments_are_utf8_source_and_utf16_generated() {
        let (_dir, session, path) = session("/// 😀 docs\nclass Person {}\n");
        let module = session
            .emit_virtual_module(&EntrySpec::File(path.clone()))
            .unwrap();
        let segment = module
            .map
            .segments
            .iter()
            .find(|segment| {
                segment.symbol_id == "T:user.Person" && segment.role == SegmentRole::Declaration
            })
            .unwrap();
        assert_eq!(segment.gen_length_utf16, 6);
        assert_eq!(
            &std::fs::read_to_string(path).unwrap()[segment.source_start_utf8 as usize..][..6],
            "Person"
        );
    }

    #[test]
    fn editor_keeps_last_good_declaration_on_compile_error() {
        let (_dir, mut session, path) = session("class Person {}\n");
        let good = session.emit_editor_module(&EntrySpec::File(path.clone()));
        session
            .update_file(&path, Some("class Person {"), 1)
            .unwrap();
        let stale = session.emit_editor_module(&EntrySpec::File(path));
        assert!(stale.stale);
        assert_eq!(stale.declaration, good.declaration);
    }

    #[test]
    fn runtime_module_respects_explicit_target() {
        let (_dir, session, _) = session("class Person {}\n");
        assert!(
            emit::emit_runtime_module(&session)
                .unwrap()
                .contains("@boundaryml/baml-bridge")
        );
    }
}
