//! Shared TypeScript SDK emitter. Produces a structurally correct
//! `baml_sdk/` tree from a `SymbolPool`: one `index.ts` per namespace
//! directory, plus `_inlinedbaml.ts` and `_typemap.ts` at the SDK root.
//!
//! Every BAML top renders to real TS: `export class` with a typed
//! constructor, `export enum`, `export type` aliases, and
//! `defineFunction(...)` / `defineInstanceFunction(...)` bindings (sync +
//! async + companions). Types come from `translate_ty`; cross-leaf
//! references resolve through namespace imports. The five runtime-owned
//! stdlib types (media + `Stream`) re-export from the configured runtime
//! package rather than getting a generated body.
//!
//! This is a close port of `sdkgen_python_pydantic2`, with two structural
//! differences: the in-directory filename (`__init__.py` → `index.ts`), and
//! that TypeScript emits no declaration file — the Python `__init__.pyi` stub has
//! no `.d.ts` counterpart, because the generated `.ts` is already fully
//! typed. Re-exports use TS syntax (`export * as child from "./child/index.js"`).

pub mod sdkgen_typescript;
pub mod sdkgen_typescript_web;

mod emit;
mod leaf;
mod routing;
mod translate_ty;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{Name, Symbol, SymbolPool, Ty};
pub use baml_codegen_types::{NamingConvention, OutputType};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};

use crate::{
    emit::{build_emitted, typemap_file::render_typemap_module},
    leaf::{LeafBody, group_and_sort, render_index_ts},
    routing::{LeafPath, route, route_class_ref},
};

fn collect_interface_tys(ty: &Ty, out: &mut BTreeSet<Name>) {
    match ty {
        Ty::Interface(name, generics, associated, _) => {
            out.insert(name.clone());
            for nested in generics.iter().chain(associated.iter().map(|(_, ty)| ty)) {
                collect_interface_tys(nested, out);
            }
        }
        Ty::Class(_, args, _) => args.iter().for_each(|ty| collect_interface_tys(ty, out)),
        Ty::List(inner, _) => collect_interface_tys(inner, out),
        Ty::Map { key, value, .. } => {
            collect_interface_tys(key, out);
            collect_interface_tys(value, out);
        }
        Ty::Union(items, _) => items.iter().for_each(|ty| collect_interface_tys(ty, out)),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                collect_interface_tys(&param.ty, out);
            }
            collect_interface_tys(ret, out);
            collect_interface_tys(throws, out);
        }
        Ty::Future(value, error, _) => {
            collect_interface_tys(value, out);
            collect_interface_tys(error, out);
        }
        Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::TypeAlias(..)
        | Ty::Literal(..)
        | Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. } => {}
    }
}

fn public_interface_tokens(pool: &SymbolPool) -> BTreeSet<Name> {
    fn function(value: &baml_codegen_types::Function, out: &mut BTreeSet<Name>) {
        for arg in &value.arguments {
            collect_interface_tys(&arg.ty, out);
        }
        collect_interface_tys(&value.return_type, out);
        if let Some(throws) = &value.throws {
            collect_interface_tys(throws, out);
        }
        for (_, watcher) in &value.watchers {
            collect_interface_tys(watcher, out);
        }
    }
    let mut out = BTreeSet::new();
    for symbol in pool.values() {
        match symbol {
            Symbol::Function(value) => function(value, &mut out),
            Symbol::Class(value) => {
                for property in &value.properties {
                    collect_interface_tys(&property.ty, &mut out);
                }
                for method in value.static_methods.iter().chain(&value.instance_methods) {
                    function(method, &mut out);
                }
            }
            Symbol::TypeAlias(value) => collect_interface_tys(&value.resolves_to, &mut out),
            Symbol::Enum(_) => {}
        }
    }
    out
}

fn render_interface_tokens(tokens: impl Iterator<Item = Name>) -> String {
    let mut out = String::new();
    for name in tokens {
        let bare = name.name();
        let fqn = name.render_dotted(false);
        let _ = writeln!(
            out,
            "\n/** Erased runtime token for BAML interface `{fqn}`. */\nexport const {bare} = Object.freeze({{ __baml_interface_fqn__: {} }} as const);",
            ts_string(&fqn),
        );
    }
    out
}

/// Target-specific settings supplied by the SDK generator modules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratorConfig {
    runtime_package: &'static str,
}

impl GeneratorConfig {
    pub const fn new(runtime_package: &'static str) -> Self {
        Self { runtime_package }
    }
}

/// Banner prepended to every generated `.ts` file. Mirrors the Python
/// `PYTHON_BANNER` (and the legacy TS emitter's `CONTENT_PREFIX`).
/// `@ts-nocheck` is intentionally NOT included — the generated `.ts`
/// is meant to typecheck cleanly under `tsc --noEmit`, and a blanket
/// `@ts-nocheck` would gut that type-checking.
fn banner(runtime_package: &str) -> String {
    format!(
        "\
/* eslint-disable */
// prettier-ignore
// ----------------------------------------------------------------------------
//
//  Welcome to Baml! To use this generated code, please run the following:
//
//  $ pnpm add {runtime_package}
//  $ npm install {runtime_package}
//  $ yarn add {runtime_package}
//
// ----------------------------------------------------------------------------
// This file was generated by BAML: please do not edit it. Instead, edit the
// BAML files and re-generate this code using: baml-cli generate

"
    )
}

/// Build a TypeScript SDK output tree for `pool`. Returned
/// paths are relative to the `baml_sdk/` output root.
pub fn to_source_code(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
    config: GeneratorConfig,
) -> HashMap<PathBuf, String> {
    to_source_code_with_metadata(pool, baml_bytecode, None, naming_convention, config)
}

pub fn to_source_code_with_metadata(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
    naming_convention: NamingConvention,
    config: GeneratorConfig,
) -> HashMap<PathBuf, String> {
    // Only `PreserveCase` is wired up so far.
    assert!(
        matches!(naming_convention, NamingConvention::PreserveCase),
        "sdkgen_typescript only supports naming_convention = PreserveCase \
         (got {naming_convention})",
    );
    let mut out: HashMap<PathBuf, String> = HashMap::new();
    let interface_tokens = public_interface_tokens(pool);

    // Every symbol routes to exactly one leaf. Dedup via `BTreeSet`.
    let mut leaves: BTreeSet<LeafPath> = BTreeSet::new();
    for key in pool.keys() {
        leaves.insert(route(key));
    }
    for name in &interface_tokens {
        leaves.insert(route_class_ref(name));
    }

    // `baml/` and the root leaf are always emitted.
    leaves.insert(LeafPath {
        segments: vec!["baml".to_string()],
    });
    leaves.insert(LeafPath {
        segments: vec!["baml".to_string(), "reflect".to_string()],
    });
    leaves.insert(LeafPath {
        segments: Vec::new(),
    });

    // Walk every leaf's ancestor chain to discover all directories that
    // need an `index.ts` and the set of immediate subdirectory children.
    let mut all_dirs: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();

    children.entry(Vec::new()).or_default();
    all_dirs.insert(Vec::new());

    for leaf in &leaves {
        all_dirs.insert(leaf.segments.clone());
        for i in 0..leaf.segments.len() {
            let prefix: Vec<String> = leaf.segments[..i].to_vec();
            children
                .entry(prefix.clone())
                .or_default()
                .insert(leaf.segments[i].clone());
            all_dirs.insert(prefix);
        }
    }

    // Build the populated-leaf bodies.
    let triples = build_emitted(pool);
    let bodies: BTreeMap<LeafPath, LeafBody> = group_and_sort(triples);

    // Emit every directory's `index.ts`. No sibling `index.d.ts`: the
    // generated `.ts` is fully typed, so a declaration file is redundant.
    for dir in &all_dirs {
        let kids = children.get(dir).cloned().unwrap_or_default();
        let leaf_path = LeafPath {
            segments: dir.clone(),
        };
        let empty_body = LeafBody {
            leaf: leaf_path.clone(),
            symbols: Vec::new(),
        };
        let body = bodies.get(&leaf_path).unwrap_or(&empty_body);
        let is_root = dir.is_empty();

        let mut content = render_index_ts(body, &kids, is_root, config.runtime_package);
        content.push_str(&render_interface_tokens(
            interface_tokens
                .iter()
                .filter(|name| route_class_ref(name) == leaf_path)
                .cloned(),
        ));
        out.insert(init_ts_path(dir), content);
    }

    // Root-only data modules.
    out.insert(
        PathBuf::from("_inlinedbaml.ts"),
        render_inlinedbaml(baml_bytecode, embedded_baml_toml),
    );
    out.insert(
        PathBuf::from("_typemap.ts"),
        render_typemap_module(&bodies, "baml_sdk", config.runtime_package),
    );

    // Prepend the do-not-edit banner to every `.ts` file.
    let banner = banner(config.runtime_package);
    for (path, content) in &mut out {
        let is_ts = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "ts");
        if is_ts {
            content.insert_str(0, &banner);
        }
    }

    out
}

fn init_ts_path(dir: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for seg in dir {
        path.push(seg);
    }
    path.push("index.ts");
    path
}

/// Base64 characters per emitted source line. Long enough that the line count
/// stays small, short enough that the file is still diffable.
const BYTECODE_BASE64_LINE: usize = 4096;

fn render_inlinedbaml(bytecode: &[u8], embedded_baml_toml: Option<&str>) -> String {
    // The bytecode ships as base64 rather than a decimal `new Uint8Array([...])`
    // literal. That literal cost up to 5 source characters per byte (~3.8 on
    // average, measured); base64 costs 1.33, which shrinks this module by ~65%.
    //
    // Size matters here beyond disk: bundlers and the Cloudflare Workers test
    // pool move this module around as a unit, and workerd caps a single
    // WebSocket message at 1 MiB — a large enough program made the pool drop
    // the connection with a 1009 "Message is too large".
    //
    // `atob` is the one decoder available everywhere this runs: browsers,
    // Workers, and Node (global since 16). BYTECODE stays a Uint8Array, so
    // `initializeRuntimeFromBytecode` is unaffected.
    let encoded = BASE64_STANDARD.encode(bytecode);
    let mut out = String::with_capacity(encoded.len() + encoded.len() / 256 + 512);
    out.push_str("const BYTECODE_BASE64 = [\n");
    for chunk in encoded.as_bytes().chunks(BYTECODE_BASE64_LINE) {
        out.push_str("  \"");
        // base64 output is ASCII, so the chunk is always valid UTF-8 and needs
        // no escaping.
        out.push_str(std::str::from_utf8(chunk).expect("base64 output is ASCII"));
        out.push_str("\",\n");
    }
    out.push_str("].join(\"\");\n\n");
    out.push_str("function decodeBytecode(encoded: string): Uint8Array {\n");
    out.push_str("  const binary = atob(encoded);\n");
    out.push_str("  const bytes = new Uint8Array(binary.length);\n");
    out.push_str("  for (let i = 0; i < binary.length; i += 1) {\n");
    out.push_str("    bytes[i] = binary.charCodeAt(i);\n");
    out.push_str("  }\n");
    out.push_str("  return bytes;\n");
    out.push_str("}\n\n");
    out.push_str("export const BYTECODE = decodeBytecode(BYTECODE_BASE64);\n");
    let manifest = embedded_baml_toml
        .map(ts_string)
        .unwrap_or_else(|| "undefined".to_string());
    let _ = writeln!(out, "export const BAML_TOML = {manifest};");
    out
}

/// Render `s` as a TypeScript double-quoted string literal. JS escaping
/// rules are byte-compatible with Python's for the ASCII range.
pub(crate) fn ts_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use baml_base::Name as BaseName;
    use baml_codegen_types::{
        Class, Enum, EnumVariant, Function, FunctionArgument, Name, Origin, Symbol, SymbolPool, Ty,
    };

    use super::*;

    const HEADER_LEN_MARKER: &str = "/* eslint-disable */";

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn origin(span: u32) -> Origin {
        Origin {
            source_file_path: "main.baml".to_string(),
            span_start: span,
        }
    }

    fn class_sym(n: &Name, span: u32) -> Symbol {
        Symbol::Class(Class {
            name: n.clone(),
            generic_params: Vec::new(),
            docstring: None,
            properties: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: origin(span),
        })
    }

    fn enum_sym(n: &Name, span: u32) -> Symbol {
        Symbol::Enum(Enum {
            name: n.clone(),
            docstring: None,
            variants: vec![EnumVariant {
                name: BaseName::new("POSITIVE"),
                docstring: None,
                value: "POSITIVE".to_string(),
            }],
            origin: origin(span),
        })
    }

    fn func_sym(span: u32) -> Symbol {
        Symbol::Function(Function {
            name: BaseName::new("extract_resume"),
            generic_params: Vec::new(),
            docstring: None,
            arguments: vec![FunctionArgument {
                name: BaseName::new("x"),
                docstring: None,
                ty: Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
                default: None,
            }],
            return_type: Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            throws: None,
            watchers: Vec::new(),
            origin: origin(span),
        })
    }

    fn emit_sdk(pool: &SymbolPool) -> HashMap<PathBuf, String> {
        to_source_code(
            pool,
            &[],
            NamingConvention::PreserveCase,
            GeneratorConfig::new("@boundaryml/baml-bridge"),
        )
    }

    #[test]
    fn empty_pool_emits_structural_files() {
        let out = emit_sdk(&SymbolPool::new());
        for f in [
            "index.ts",
            "_inlinedbaml.ts",
            "_typemap.ts",
            "baml/index.ts",
        ] {
            assert!(out.contains_key(&PathBuf::from(f)), "missing {f}");
        }
        // No declaration files are emitted.
        assert!(!out.keys().any(|p| p.to_string_lossy().ends_with(".d.ts")));
        let root = &out[&PathBuf::from("index.ts")];
        assert!(root.contains(HEADER_LEN_MARKER));
        assert!(root.contains(
            "initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE, _inlinedbaml.BAML_TOML);"
        ));
        assert!(root.contains("setTypeMap(_TYPE_MAP);"));
        assert!(root.contains("export * as baml from \"./baml/index.js\";"));
        assert!(!root.contains("export const b"));
        assert!(!root.contains("BAML_PLACEHOLDER"));
    }

    #[test]
    fn class_body_renders_in_leaf() {
        let mut pool = SymbolPool::new();
        let n = name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class_sym(&n, 0));
        let out = emit_sdk(&pool);
        let leaf = &out[&PathBuf::from("lorem/index.ts")];
        assert!(leaf.contains("export class Resume {"));
        assert!(!out.contains_key(&PathBuf::from("lorem/index.d.ts")));
    }

    #[test]
    fn enum_body_renders_in_leaf() {
        let mut pool = SymbolPool::new();
        let n = name("user", &["ipsum"], "Sentiment");
        pool.insert(n.clone(), enum_sym(&n, 0));
        let out = emit_sdk(&pool);
        let leaf = &out[&PathBuf::from("ipsum/index.ts")];
        assert!(leaf.contains("export enum Sentiment {"));
        assert!(leaf.contains("POSITIVE = \"POSITIVE\","));
    }

    #[test]
    fn function_fans_out_sync_and_async() {
        let mut pool = SymbolPool::new();
        let n = name("user", &["lorem"], "extract_resume");
        pool.insert(n, func_sym(0));
        let out = emit_sdk(&pool);
        let leaf = &out[&PathBuf::from("lorem/index.ts")];
        assert!(leaf.contains(
            "export const extract_resume = defineFunction(\"user.lorem.extract_resume\", \"sync\", [\"x\"])"
        ));
        assert!(leaf.contains(
            "export const extract_resume_async = defineFunction(\"user.lorem.extract_resume\", \"async\", [\"x\"])"
        ));
    }

    #[test]
    fn leaf_namespace_is_directory_with_index() {
        let mut pool = SymbolPool::new();
        let n = name("user", &["lorem"], "Resume");
        pool.insert(n.clone(), class_sym(&n, 0));
        let out = emit_sdk(&pool);
        assert!(out.contains_key(&PathBuf::from("lorem/index.ts")));
        assert!(!out.contains_key(&PathBuf::from("lorem.ts")));
    }

    #[test]
    fn container_index_only_reexports_children() {
        let mut pool = SymbolPool::new();
        let n = name("aws", &["s3"], "Bucket");
        pool.insert(n.clone(), class_sym(&n, 0));
        let out = emit_sdk(&pool);
        let vendor = &out[&PathBuf::from("vendor/index.ts")];
        assert!(vendor.contains("export * as aws from \"./aws/index.js\";"));
        assert!(!vendor.contains("export const"));
        assert!(!vendor.contains("export * from"));
    }

    #[test]
    fn vendor_creates_interior_containers() {
        let mut pool = SymbolPool::new();
        let n = name("aws", &["s3"], "Bucket");
        pool.insert(n.clone(), class_sym(&n, 0));
        let out = emit_sdk(&pool);
        assert!(out.contains_key(&PathBuf::from("vendor/index.ts")));
        assert!(out.contains_key(&PathBuf::from("vendor/aws/index.ts")));
        assert!(out.contains_key(&PathBuf::from("vendor/aws/s3/index.ts")));
        assert!(
            out[&PathBuf::from("vendor/aws/index.ts")]
                .contains("export * as s3 from \"./s3/index.js\";")
        );
        assert!(out[&PathBuf::from("vendor/aws/s3/index.ts")].contains("export class Bucket {"));
    }

    #[test]
    fn root_namespace_symbol_in_root_index() {
        let mut pool = SymbolPool::new();
        let n = name("user", &[], "Foo");
        pool.insert(n.clone(), class_sym(&n, 0));
        let out = emit_sdk(&pool);
        let root = &out[&PathBuf::from("index.ts")];
        assert!(root.contains("export class Foo {"));
        assert!(!root.contains("export const b"));
    }

    #[test]
    fn stream_class_emits_in_base_leaf_with_suffix() {
        // spec2: `Resume$stream` is emitted beside `Resume` in the `lorem`
        // leaf, keeping its `$stream` suffix — there is no `stream_types/`.
        let mut pool = SymbolPool::new();
        let n = name("user", &["lorem"], "Resume$stream");
        pool.insert(n.clone(), class_sym(&n, 0));
        let out = emit_sdk(&pool);
        assert!(!out.contains_key(&PathBuf::from("stream_types/lorem/index.ts")));
        let leaf = &out[&PathBuf::from("lorem/index.ts")];
        assert!(leaf.contains("export class Resume$stream {"));
        let typemap = &out[&PathBuf::from("_typemap.ts")];
        assert!(typemap.contains("\"user.lorem.Resume$stream\": () => (__leaf_"));
    }

    #[test]
    fn inlinedbaml_embeds_bytecode() {
        let out = to_source_code(
            &SymbolPool::new(),
            &[0, 1, 2, 253, 254, 255],
            NamingConvention::PreserveCase,
            GeneratorConfig::new("@boundaryml/baml-bridge"),
        );
        let inlined = &out[&PathBuf::from("_inlinedbaml.ts")];
        assert!(inlined.contains("export const BYTECODE = decodeBytecode(BYTECODE_BASE64);"));
        // The exact base64 for those six bytes, so a change to the encoding
        // fails here rather than at runtime in a generated SDK.
        assert!(
            inlined.contains(&format!(
                "  \"{}\",\n",
                BASE64_STANDARD.encode([0, 1, 2, 253, 254, 255])
            )),
            "{inlined}"
        );
        // The decoder must be self-contained: no import, and byte-exact.
        assert!(inlined.contains("const binary = atob(encoded);"));
    }

    #[test]
    fn inlinedbaml_base64_round_trips_and_wraps_long_bytecode() {
        // Enough bytes to span several emitted lines, with every byte value
        // represented so a mangled alphabet or a bad chunk boundary shows up.
        let bytecode: Vec<u8> = (0..(BYTECODE_BASE64_LINE * 3))
            .map(|i| u8::try_from(i % 256).expect("i % 256 fits in u8"))
            .collect();
        let out = to_source_code(
            &SymbolPool::new(),
            &bytecode,
            NamingConvention::PreserveCase,
            GeneratorConfig::new("@boundaryml/baml-bridge"),
        );
        let inlined = &out[&PathBuf::from("_inlinedbaml.ts")];

        let joined: String = inlined
            .lines()
            .filter(|l| l.starts_with("  \"") && l.ends_with("\","))
            .map(|l| &l[3..l.len() - 2])
            .collect();
        assert_eq!(
            BASE64_STANDARD
                .decode(joined)
                .expect("emitted base64 decodes"),
            bytecode,
            "emitted base64 must round-trip to the original bytecode"
        );
    }

    #[test]
    fn ts_string_escapes() {
        assert_eq!(ts_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(ts_string("\t\r"), "\"\\t\\r\"");
        assert_eq!(ts_string("\u{1}"), "\"\\x01\"");
    }
}
