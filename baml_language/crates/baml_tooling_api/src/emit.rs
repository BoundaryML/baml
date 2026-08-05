use std::{fmt::Write as _, path::PathBuf, str::FromStr};

use baml_surface::{Resolved, SymbolId, export::ItemDetail};
use serde::{Deserialize, Serialize};

use crate::{ProjectSession, ToolingError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Node,
    Web,
}

impl Target {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Web => "web",
        }
    }

    fn runtime_package(self) -> &'static str {
        match self {
            Self::Node => "@boundaryml/baml-bridge",
            Self::Web => "@boundaryml/baml-bridge-web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "lowercase")]
pub enum EntrySpec {
    File(PathBuf),
    Client,
}

impl EntrySpec {
    pub(crate) fn cache_key(&self) -> String {
        match self {
            Self::File(path) => path.to_string_lossy().into_owned(),
            Self::Client => "baml:client".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentRole {
    Declaration,
    Reference,
    Type,
    Documentation,
    Synthetic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub gen_start_utf16: u32,
    pub gen_length_utf16: u32,
    pub source_file: u32,
    pub source_start_utf8: u32,
    pub source_length_utf8: u32,
    pub symbol_id: String,
    pub signature_id: Option<String>,
    pub role: SegmentRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentMap {
    pub version: u32,
    pub generated_file: String,
    pub sources: Vec<PathBuf>,
    pub source_hashes: Vec<String>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualModule {
    pub id: String,
    pub runtime_id: String,
    pub code: String,
    pub declaration: String,
    pub map: SegmentMap,
    pub watch_files: Vec<PathBuf>,
    pub fingerprint: String,
    pub revision: u64,
    pub stale: bool,
}

impl VirtualModule {
    pub(crate) fn poison(id: String, revision: u64, fingerprint: String) -> Self {
        Self {
            runtime_id: String::new(),
            code: "const unavailable = new Proxy({}, { get() { throw new Error('BAML project has compiler errors'); } });\nexport const b = unavailable;\n".to_string(),
            declaration: "declare const unavailable: { readonly [name: string]: never };\nexport { unavailable as b };\n".to_string(),
            map: SegmentMap {
                version: 1,
                generated_file: id.clone(),
                sources: Vec::new(),
                source_hashes: Vec::new(),
                segments: Vec::new(),
            },
            id,
            watch_files: Vec::new(),
            fingerprint,
            revision,
            stale: true,
        }
    }

    pub fn into_generated(self) -> GeneratedFile {
        GeneratedFile {
            path: PathBuf::from(self.id),
            contents: self.declaration,
            map: self.map,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
    pub map: SegmentMap,
}

struct MappedWriter {
    text: String,
    sources: Vec<PathBuf>,
    source_hashes: Vec<String>,
    segments: Vec<Segment>,
}

impl MappedWriter {
    fn new(session: &ProjectSession) -> Self {
        let mut sources = Vec::new();
        let mut source_hashes = Vec::new();
        for file in session.db.get_source_files() {
            sources.push(file.path(&session.db));
            let digest = sha2::Sha256::digest(file.text(&session.db).as_bytes());
            source_hashes.push(hex::encode(digest));
        }
        Self {
            text: String::new(),
            sources,
            source_hashes,
            segments: Vec::new(),
        }
    }

    fn push(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn mapped_name(
        &mut self,
        session: &ProjectSession,
        name: &str,
        symbol_id: &str,
        synthetic: bool,
    ) {
        self.mapped_value(
            session,
            name,
            symbol_id,
            if synthetic {
                SegmentRole::Synthetic
            } else {
                SegmentRole::Declaration
            },
        );
    }

    fn mapped_value(
        &mut self,
        session: &ProjectSession,
        text: &str,
        symbol_id: &str,
        role: SegmentRole,
    ) {
        let start = u32::try_from(self.text.encode_utf16().count())
            .expect("generated TypeScript declaration exceeds u32 UTF-16 offsets");
        self.text.push_str(text);
        let Ok(id) = SymbolId::from_str(symbol_id) else {
            return;
        };
        let Some(resolved) = id.resolve(&session.db) else {
            return;
        };
        let (file, range) = match resolved {
            Resolved::Symbol(symbol) => {
                let Some(range) = symbol.name_span(&session.db) else {
                    return;
                };
                (symbol.file(&session.db), range)
            }
            Resolved::Member(owner, member) => {
                (owner.file(&session.db), member.name_span(&session.db))
            }
            Resolved::Package(_) | Resolved::Namespace(_) => return,
        };
        let path = file.path(&session.db);
        let Some(source_file) = self.sources.iter().position(|source| source == &path) else {
            return;
        };
        self.segments.push(Segment {
            gen_start_utf16: start,
            gen_length_utf16: u32::try_from(text.encode_utf16().count())
                .expect("generated TypeScript segment exceeds u32 UTF-16 offsets"),
            source_file: u32::try_from(source_file)
                .expect("BAML tooling source table exceeds u32 entries"),
            source_start_utf8: range.start().into(),
            source_length_utf8: u32::from(range.len()),
            symbol_id: symbol_id.to_string(),
            signature_id: None,
            role,
        });
    }

    fn mapped_type(&mut self, session: &ProjectSession, display: &str, head: Option<&str>) {
        let rendered = typescript_ty(display);
        if let Some(symbol_id) = head {
            self.mapped_value(session, &rendered, symbol_id, SegmentRole::Type);
        } else {
            self.push(&rendered);
        }
    }
}

pub(crate) fn emit_virtual_module(session: &ProjectSession, entry: &EntrySpec) -> VirtualModule {
    let id = entry.cache_key();
    let runtime_id = format!("\\0baml:{}:runtime", session.project_id());
    let exports = session.filtered_exports(entry);
    let mut out = MappedWriter::new(session);
    out.push("// Generated by BAML tooling.v1. Do not edit.\n");
    out.push(&format!(
        "// baml-fingerprint: {}\n\n",
        session.fingerprint()
    ));

    for item in &exports.items {
        if let Some(doc) = &item.docstring {
            let doc = format!("/** {} */", escape_doc(doc));
            out.mapped_value(session, &doc, &item.id, SegmentRole::Documentation);
            out.push("\n");
        }
        match &item.detail {
            ItemDetail::Class { fields, .. } => {
                out.push("export interface ");
                out.mapped_name(session, &item.name, &item.id, item.synthetic);
                out.push(" {\n");
                for field in fields {
                    if let Some(doc) = &field.docstring {
                        out.push("  /** ");
                        out.push(&escape_doc(doc));
                        out.push(" */\n");
                    }
                    out.push("  ");
                    out.mapped_name(session, &field.name, &field.id, false);
                    out.push(": ");
                    out.mapped_type(session, &field.ty.display, field.ty.head.as_deref());
                    out.push(";\n");
                }
                out.push("}\n\n");
            }
            ItemDetail::Interface { .. } => {
                out.push("export interface ");
                out.mapped_name(session, &item.name, &item.id, item.synthetic);
                out.push(" {}\n\n");
            }
            ItemDetail::Enum { variants, .. } => {
                out.push("export enum ");
                out.mapped_name(session, &item.name, &item.id, item.synthetic);
                out.push(" {\n");
                for variant in variants {
                    out.push("  ");
                    out.mapped_name(session, &variant.name, &variant.id, false);
                    let _ = writeln!(out.text, " = {:?},", variant.name);
                }
                out.push("}\n\n");
            }
            ItemDetail::TypeAlias { resolved } => {
                out.push("export type ");
                out.mapped_name(session, &item.name, &item.id, item.synthetic);
                out.push(" = ");
                out.mapped_type(session, &resolved.display, resolved.head.as_deref());
                out.push(";\n\n");
            }
            ItemDetail::Function { .. } | ItemDetail::Plain {} => {}
        }
    }

    out.push("export interface BamlClient {\n");
    for item in &exports.items {
        let ItemDetail::Function { signature } = &item.detail else {
            continue;
        };
        out.push("  readonly ");
        out.mapped_name(session, &item.name, &item.id, item.synthetic);
        out.push(": (");
        for (index, param) in signature.params.iter().enumerate() {
            if index > 0 {
                out.push(", ");
            }
            out.push(&param.name);
            if param.optional {
                out.push("?");
            }
            out.push(": ");
            out.mapped_type(session, &param.ty.display, param.ty.head.as_deref());
        }
        out.push(") => Promise<");
        out.mapped_type(
            session,
            &signature.returns.display,
            signature.returns.head.as_deref(),
        );
        out.push(">;\n");
    }
    out.push("}\nexport declare const b: BamlClient;\n");

    let declaration = out.text;
    let mut code = format!("import {{ defineFunction }} from {runtime_id:?};\n");
    for item in &exports.items {
        let ItemDetail::Enum { variants, .. } = &item.detail else {
            continue;
        };
        let _ = writeln!(code, "export const {} = {{", item.name);
        for variant in variants {
            let _ = writeln!(code, "  {}: {:?},", variant.name, variant.name);
        }
        code.push_str("};\n");
    }
    code.push_str("export const b = {\n");
    for item in &exports.items {
        let ItemDetail::Function { signature } = &item.detail else {
            continue;
        };
        let params = signature
            .params
            .iter()
            .map(|param| format!("{:?}", param.name))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            code,
            "  {0}: defineFunction({1:?}, \"async\", [{params}]),",
            item.name,
            item.id
                .split_once(':')
                .map_or(item.id.as_str(), |(_, fqn)| fqn)
        );
    }
    code.push_str("};\n");

    VirtualModule {
        id: id.clone(),
        runtime_id,
        code,
        declaration,
        map: SegmentMap {
            version: 1,
            generated_file: id,
            sources: out.sources,
            source_hashes: out.source_hashes,
            segments: out.segments,
        },
        watch_files: session.watch_files(),
        fingerprint: session.fingerprint(),
        revision: session.revision(),
        stale: false,
    }
}

pub(crate) fn emit_runtime_module(session: &ProjectSession) -> Result<String, ToolingError> {
    let program = session
        .db
        .get_bytecode()
        .map_err(|error| ToolingError::Bytecode(error.to_string()))?;
    let bytes =
        borsh::to_vec(&program).map_err(|error| ToolingError::Bytecode(error.to_string()))?;
    let package = session.target.runtime_package();
    Ok(format!(
        "import {{ defineFunction, initializeRuntimeFromBytecode }} from {package:?};\nconst bytecode = new Uint8Array({bytes:?});\ninitializeRuntimeFromBytecode(bytecode);\nexport {{ defineFunction }};\n"
    ))
}

fn escape_doc(doc: &str) -> String {
    doc.replace("*/", "*\\/").replace(['\r', '\n'], " ")
}

fn typescript_ty(ty: &str) -> String {
    let ty = ty.trim();
    if let Some(inner) = ty.strip_suffix("[]") {
        return format!("Array<{}>", typescript_ty(inner));
    }
    if let Some(inner) = ty.strip_suffix('?') {
        return format!("{} | null", typescript_ty(inner));
    }
    if ty.contains(" | ") {
        return ty
            .split(" | ")
            .map(typescript_ty)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    match ty {
        "string" => "string".to_string(),
        "int" | "float" => "number".to_string(),
        "bool" => "boolean".to_string(),
        "null" => "null".to_string(),
        "bytes" => "Uint8Array".to_string(),
        "void" => "void".to_string(),
        "unknown" => "unknown".to_string(),
        other => other
            .strip_prefix("user.")
            .unwrap_or(other)
            .replace("baml.", ""),
    }
}

use sha2::Digest as _;
