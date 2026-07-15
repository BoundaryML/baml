//! Swift SDK generator for BAML.
//!
//! Mirrors `sdkgen_python_pydantic2`'s public entry point: consumes a
//! [`baml_codegen_types::SymbolPool`] plus borsh-serialized bytecode and
//! returns generated Swift sources as `(relative_path, content)` pairs.
//! The paths are relative to the generated package's `Sources/Baml/`
//! output root (the harness / CLI decides where that root lives).
//!
//! Phase 1 scope: free functions over the primitive subset (see
//! `translate_ty`). Functions whose signature contains an unsupported
//! type, that declare generics, defaults, or that are `$`-companions
//! are skipped — the generated package must always compile; coverage
//! widens phase by phase.
//!
//! Unlike Python (which binds callables at runtime with
//! `define_function`), Swift cannot synthesize functions, so this
//! generator emits real `func` bodies that call
//! `BamlRuntime.shared.callSync(...)` / `call(...)` from the
//! `BamlBridge` runtime package.

mod translate_ty;

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use base64::Engine as _;

use baml_codegen_types::{Function, Symbol, SymbolPool};
pub use baml_codegen_types::{NamingConvention, OutputType};
use translate_ty::translate_ty;

/// Build the Swift SDK output tree using precompiled BAML bytecode as
/// the runtime payload. Returned paths are relative to the generated
/// `Sources/Baml/` root.
pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    let mut out: HashMap<PathBuf, String> = HashMap::new();

    out.insert(
        PathBuf::from("_InlinedBaml.swift"),
        render_inlined_baml(baml_bytecode),
    );

    // namespace path (joined) -> function renderings, both BTree-sorted
    // for deterministic output.
    let mut namespaces: BTreeMap<Vec<String>, BTreeMap<String, String>> = BTreeMap::new();
    for (key, symbol) in pool {
        let Symbol::Function(function) = symbol else {
            continue; // classes/enums/aliases arrive in Phase 2+
        };
        // Only user-package free functions for now; stdlib (`baml`) and
        // vendor packages land with their capability phases.
        if key.pkg.as_str() != "user" {
            continue;
        }
        let ns: Vec<String> = key
            .namespace_path
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        if let Some(rendered) = render_function(key, function) {
            namespaces
                .entry(ns)
                .or_default()
                .insert(function.name.as_str().to_string(), rendered);
        }
    }

    let root_fns = namespaces.remove(&Vec::new()).unwrap_or_default();
    out.insert(PathBuf::from("Baml.swift"), render_root(&root_fns));

    // One file per top-level namespace segment; deeper segments nest as
    // enums inside it.
    let mut by_top: BTreeMap<String, BTreeMap<Vec<String>, BTreeMap<String, String>>> =
        BTreeMap::new();
    for (ns, fns) in namespaces {
        by_top
            .entry(ns[0].clone())
            .or_default()
            .insert(ns, fns);
    }
    for (top, ns_map) in by_top {
        out.insert(
            PathBuf::from(format!("{top}.swift")),
            render_namespace_file(&top, &ns_map),
        );
    }

    out
}

/// Render one free function as a sync + async pair, or `None` if any
/// part of its signature is outside the supported subset.
fn render_function(key: &baml_codegen_types::Name, function: &Function) -> Option<String> {
    let bare = function.name.as_str();
    // `$stream` / `$build_request` companions come with their own
    // phases; `$` is not a Swift identifier character anyway.
    if bare.contains('$') || !function.generic_params.is_empty() {
        return None;
    }

    let mut params = Vec::new(); // (label, swift_ty)
    for arg in &function.arguments {
        if arg.default.is_some() {
            return None; // optional args are Phase 2 (BamlOptional design)
        }
        params.push((escape_ident(arg.name.as_str()), translate_ty(&arg.ty)?));
    }

    let ret = match &function.return_type {
        baml_codegen_types::Ty::Unit => None,
        other => Some(translate_ty(other)?),
    };

    let fqn = key.to_string();
    let param_list = params
        .iter()
        .map(|(name, ty)| format!("{name}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let args_literal = if params.is_empty() {
        "[]".to_string()
    } else {
        format!(
            "[{}]",
            params
                .iter()
                .map(|(name, _)| format!("(\"{}\", {name})", name.trim_matches('`')))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let fn_name = escape_ident(bare);
    let async_name = escape_ident(&format!("{bare}_async"));
    let mut out = String::new();
    let doc = function
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();

    match &ret {
        Some(ret_ty) => {
            let _ = write!(
                out,
                "{doc}public static func {fn_name}({param_list}) throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 \treturn try BamlRuntime.shared.callSync(\"{fqn}\", args: {args_literal})\n\
                 }}\n\n\
                 {doc}public static func {async_name}({param_list}) async throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 \treturn try await BamlRuntime.shared.call(\"{fqn}\", args: {args_literal})\n\
                 }}\n"
            );
        }
        None => {
            let _ = write!(
                out,
                "{doc}public static func {fn_name}({param_list}) throws {{\n\
                 \t_ = Baml._initialized\n\
                 \ttry BamlRuntime.shared.callSyncVoid(\"{fqn}\", args: {args_literal})\n\
                 }}\n\n\
                 {doc}public static func {async_name}({param_list}) async throws {{\n\
                 \t_ = Baml._initialized\n\
                 \ttry await BamlRuntime.shared.callVoid(\"{fqn}\", args: {args_literal})\n\
                 }}\n"
            );
        }
    }
    Some(out)
}

fn render_docstring(doc: &str) -> String {
    let mut out = String::new();
    for line in doc.lines() {
        let _ = writeln!(out, "/// {line}");
    }
    out
}

/// Backtick-escape Swift keywords that can appear as BAML identifiers.
fn escape_ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "associatedtype", "class", "deinit", "enum", "extension", "func", "import", "init",
        "inout", "internal", "let", "operator", "private", "protocol", "public", "static",
        "struct", "subscript", "typealias", "var", "break", "case", "continue", "default",
        "defer", "do", "else", "fallthrough", "for", "guard", "if", "in", "repeat", "return",
        "switch", "where", "while", "as", "catch", "false", "is", "nil", "rethrows", "self",
        "Self", "super", "throw", "throws", "true", "try",
    ];
    if KEYWORDS.contains(&name) {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}

fn render_root(root_fns: &BTreeMap<String, String>) -> String {
    let mut out = String::from(
        "// Generated by BAML. DO NOT EDIT.\n\
         import BamlBridge\nimport Foundation\n\n\
         /// Root namespace of the generated BAML SDK. Touching\n\
         /// `_initialized` (every generated entry point does) loads the\n\
         /// inlined bytecode into the native runtime exactly once.\n\
         public enum Baml {\n\
         \tstatic let _initialized: Bool = {\n\
         \t\tBamlRuntime.shared.initialize(bytecode: _BamlInlined.bytecode)\n\
         \t\treturn true\n\
         \t}()\n",
    );
    for rendered in root_fns.values() {
        out.push('\n');
        out.push_str(&indent(rendered, 1));
    }
    out.push_str("}\n");
    out
}

fn render_namespace_file(
    top: &str,
    ns_map: &BTreeMap<Vec<String>, BTreeMap<String, String>>,
) -> String {
    let mut out = String::from("// Generated by BAML. DO NOT EDIT.\nimport BamlBridge\nimport Foundation\n\nextension Baml {\n");
    out.push_str(&render_ns_enum(top, &[top.to_string()], ns_map, 1));
    out.push_str("}\n");
    out
}

/// Recursively render `enum <seg> { fns…; child enums… }`.
fn render_ns_enum(
    seg: &str,
    path: &[String],
    ns_map: &BTreeMap<Vec<String>, BTreeMap<String, String>>,
    depth: usize,
) -> String {
    let tab = "\t".repeat(depth);
    let mut out = format!("{tab}public enum {} {{\n", escape_ident(seg));
    if let Some(fns) = ns_map.get(path) {
        for rendered in fns.values() {
            out.push('\n');
            out.push_str(&indent(rendered, depth + 1));
        }
    }
    // Immediate children: paths extending `path` by one segment.
    let mut children: Vec<String> = Vec::new();
    for ns in ns_map.keys() {
        if ns.len() == path.len() + 1 && ns.starts_with(path) {
            children.push(ns[path.len()].clone());
        }
    }
    children.dedup();
    for child in children {
        let mut child_path = path.to_vec();
        child_path.push(child.clone());
        out.push('\n');
        out.push_str(&render_ns_enum(&child, &child_path, ns_map, depth + 1));
    }
    let _ = writeln!(out, "{tab}}}");
    out
}

fn indent(block: &str, depth: usize) -> String {
    let tab = "\t".repeat(depth);
    let mut out = String::new();
    for line in block.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "{tab}{line}");
        }
    }
    out
}

/// The borsh bytecode payload, base64-encoded, as ONE multiline string
/// literal. Two rejected alternatives, both fatal at engine sizes: a
/// `[UInt8]` literal type-checks element-by-element, and a `"…" + "…"`
/// chunk chain builds a `+` expression whose type-check is
/// super-linear in the number of chunks (observed: 55+ minutes for a
/// multi-MB payload). A `"""…"""` literal is a single token — instant —
/// and the embedded newlines are skipped by the base64 decoder via
/// `.ignoreUnknownCharacters`.
fn render_inlined_baml(baml_bytecode: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(baml_bytecode);
    let mut out = String::from(
        "// Generated by BAML. DO NOT EDIT.\n\
         import Foundation\n\n\
         enum _BamlInlined {\n    static let bytecodeBase64: String = \"\"\"\n",
    );
    // Fixed-width lines inside the literal for editor/diff friendliness.
    const CHUNK: usize = 96;
    for chunk in b64.as_bytes().chunks(CHUNK) {
        out.push_str("        ");
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ascii"));
        out.push('\n');
    }
    out.push_str("        \"\"\"\n");
    out.push_str(
        "\n    static var bytecode: Data {\n        \
         Data(base64Encoded: bytecodeBase64, options: .ignoreUnknownCharacters)!\n    }\n}\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytecode_payload_round_trips_via_base64() {
        let pool = SymbolPool::default();
        let bytecode = vec![0u8, 1, 2, 250, 251, 252];
        let files = to_source_code_with_bytecode(&pool, &bytecode, NamingConvention::PreserveCase);

        let inlined = &files[&PathBuf::from("_InlinedBaml.swift")];
        // Collect the bare base64 lines between the `"""` delimiters.
        let b64: String = inlined
            .lines()
            .skip_while(|l| !l.contains("\"\"\""))
            .skip(1)
            .take_while(|l| !l.contains("\"\"\""))
            .map(str::trim)
            .collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("valid base64");
        assert_eq!(decoded, bytecode);

        assert!(files[&PathBuf::from("Baml.swift")].contains("public enum Baml"));
    }

    #[test]
    fn translate_ty_primitive_subset() {
        use baml_codegen_types::Ty;
        let t = |ty: &Ty| crate::translate_ty::translate_ty(ty);
        assert_eq!(t(&Ty::Int).as_deref(), Some("Int"));
        assert_eq!(t(&Ty::Float).as_deref(), Some("Double"));
        assert_eq!(t(&Ty::List(Box::new(Ty::Int))).as_deref(), Some("[Int]"));
        assert_eq!(
            t(&Ty::Map {
                key: Box::new(Ty::String),
                value: Box::new(Ty::List(Box::new(Ty::Int)))
            })
            .as_deref(),
            Some("[String: [Int]]")
        );
        // string?[] → [String?]
        assert_eq!(
            t(&Ty::List(Box::new(Ty::Union(vec![Ty::String, Ty::Null])))).as_deref(),
            Some("[String?]")
        );
        // (int | string)[] — not yet
        assert_eq!(t(&Ty::List(Box::new(Ty::Union(vec![Ty::Int, Ty::String])))), None);
        // map with non-string key — not yet
        assert_eq!(
            t(&Ty::Map {
                key: Box::new(Ty::Int),
                value: Box::new(Ty::Int)
            }),
            None
        );
    }
}
