//! Swift SDK generator for BAML.
//!
//! Mirrors `sdkgen_python_pydantic2`'s public entry point: consumes a
//! [`baml_codegen_types::SymbolPool`] plus borsh-serialized bytecode and
//! returns generated Swift sources as `(relative_path, content)` pairs.
//! The paths are relative to the generated package's `Sources/Baml/`
//! output root (the harness / CLI decides where that root lives).
//!
//! Phase 2 scope: free functions (required + optional args), classes as
//! Equatable/Sendable structs, enums, non-recursive type aliases —
//! over the type subset in `translate_ty`. Symbols whose signature the
//! translator can't spell are skipped (a fixpoint removes classes with
//! unsupported fields, then anything referencing them) — the generated
//! package must always compile; coverage widens phase by phase.
//!
//! Recursive classes: Swift structs can't contain themselves, so any
//! field whose (optional-stripped) class target can reach the
//! containing class through direct (non-List/Map) references is boxed
//! with the runtime's `@BamlIndirect` CoW wrapper.
//!
//! Unlike Python (which binds callables at runtime with
//! `define_function`), Swift cannot synthesize functions, so this
//! generator emits real `func` bodies that call
//! `BamlRuntime.shared.callSync(...)` / `call(...)` from the
//! `BamlBridge` runtime package.

mod emit;
mod translate_ty;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use base64::Engine as _;

use baml_codegen_types::{Class, Name, Symbol, SymbolPool, Ty};
pub use baml_codegen_types::{NamingConvention, OutputType};
use emit::{RenderedField, indent_lines, render_class, render_enum, render_function,
    render_type_alias, sort_key};
use translate_ty::{TranslateCtx, translate_ty};

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

    let ctx = build_translate_ctx(pool);
    let boxed_fields = compute_boxed_fields(pool, &ctx);

    // namespace path -> (sort_key -> rendered decl), BTree-sorted for
    // deterministic output.
    let mut namespaces: BTreeMap<Vec<String>, BTreeMap<String, String>> = BTreeMap::new();
    for (key, symbol) in pool {
        if key.pkg.as_str() != "user" || key.is_stream() {
            continue;
        }
        let fqn = key.to_string();
        let rendered = match symbol {
            Symbol::Function(function) => render_function(key, function, &ctx),
            Symbol::Class(class) => {
                if !ctx.supported_classes.contains(&fqn) {
                    None
                } else {
                    render_supported_class(class, key, &ctx, &boxed_fields)
                }
            }
            Symbol::Enum(enum_) => Some(render_enum(enum_, key)),
            Symbol::TypeAlias(alias) => {
                if ctx.supported_aliases.contains(&fqn) {
                    render_type_alias(alias, &ctx)
                } else {
                    None
                }
            }
        };
        let Some(rendered) = rendered else { continue };
        let ns: Vec<String> = key
            .namespace_path
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        let bare = key.bare_name().to_string();
        namespaces
            .entry(ns)
            .or_default()
            .insert(sort_key(symbol, &bare), rendered);
    }

    // Ensure ancestor namespaces exist so deep paths (`a.b.Thing`)
    // get their intermediate enums rendered.
    let paths: Vec<Vec<String>> = namespaces.keys().cloned().collect();
    for path in paths {
        for depth in 1..path.len() {
            namespaces.entry(path[..depth].to_vec()).or_default();
        }
    }

    let root_decls = namespaces.remove(&Vec::new()).unwrap_or_default();
    out.insert(PathBuf::from("Baml.swift"), render_root(&root_decls));

    // One file per top-level namespace segment; deeper segments nest as
    // enums inside it.
    let mut by_top: BTreeMap<String, BTreeMap<Vec<String>, BTreeMap<String, String>>> =
        BTreeMap::new();
    for (ns, decls) in namespaces {
        by_top.entry(ns[0].clone()).or_default().insert(ns, decls);
    }
    for (top, ns_map) in by_top {
        out.insert(
            PathBuf::from(format!("{top}.swift")),
            render_namespace_file(&top, &ns_map),
        );
    }

    out
}

/// Fixpoint over named types: start assuming every candidate class /
/// alias is supported, then repeatedly drop any whose definition uses
/// an unsupported type, until stable. Enums are always supported.
/// Generic and `$stream` classes are excluded up front (later phases).
fn build_translate_ctx(pool: &SymbolPool) -> TranslateCtx {
    let mut supported_classes: BTreeSet<String> = BTreeSet::new();
    let mut supported_aliases: BTreeSet<String> = BTreeSet::new();
    let mut supported_enums: BTreeSet<String> = BTreeSet::new();

    for (key, symbol) in pool {
        if key.pkg.as_str() != "user" || key.is_stream() {
            continue;
        }
        match symbol {
            Symbol::Class(class) if class.generic_params.is_empty() => {
                supported_classes.insert(key.to_string());
            }
            Symbol::Enum(_) => {
                supported_enums.insert(key.to_string());
            }
            Symbol::TypeAlias(alias) if !alias.recursive => {
                supported_aliases.insert(key.to_string());
            }
            _ => {}
        }
    }

    loop {
        let ctx = TranslateCtx {
            supported_classes: supported_classes.clone(),
            supported_enums: supported_enums.clone(),
            supported_aliases: supported_aliases.clone(),
        };
        let mut changed = false;
        for (key, symbol) in pool {
            if key.pkg.as_str() != "user" || key.is_stream() {
                continue;
            }
            let fqn = key.to_string();
            match symbol {
                Symbol::Class(class) => {
                    if supported_classes.contains(&fqn)
                        && class
                            .properties
                            .iter()
                            .any(|p| translate_ty(&p.ty, &ctx).is_none())
                    {
                        supported_classes.remove(&fqn);
                        changed = true;
                    }
                }
                Symbol::TypeAlias(alias) => {
                    if supported_aliases.contains(&fqn)
                        && translate_ty(&alias.resolves_to, &ctx).is_none()
                    {
                        supported_aliases.remove(&fqn);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        if !changed {
            return TranslateCtx {
                supported_classes,
                supported_enums,
                supported_aliases,
            };
        }
    }
}

/// Direct (non-heap) class targets of a field type: bare class refs
/// and refs behind Optional (null-unions) store inline in a Swift
/// struct; List/Map contents are already heap-allocated and never
/// force boxing. Aliases resolve through (they're non-recursive here).
fn direct_class_targets<'p>(
    ty: &Ty,
    pool: &'p SymbolPool,
    out: &mut Vec<String>,
) {
    match ty {
        Ty::Class(name, args) if args.is_empty() => out.push(name.to_string()),
        Ty::Union(members) => {
            for member in members {
                direct_class_targets(member, pool, out);
            }
        }
        Ty::TypeAlias(name) => {
            if let Some(Symbol::TypeAlias(alias)) = pool.get(name) {
                if !alias.recursive {
                    direct_class_targets(&alias.resolves_to, pool, out);
                }
            }
        }
        _ => {}
    }
}

/// `(class FQN, field name)` pairs that must be `@BamlIndirect`-boxed:
/// the field's direct class target can reach the containing class back
/// through direct references (self-recursion, mutual recursion, SCCs).
fn compute_boxed_fields(pool: &SymbolPool, ctx: &TranslateCtx) -> BTreeSet<(String, String)> {
    // Adjacency over supported classes via direct references.
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut field_targets: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (key, symbol) in pool {
        let Symbol::Class(class) = symbol else { continue };
        let fqn = key.to_string();
        if !ctx.supported_classes.contains(&fqn) {
            continue;
        }
        for prop in &class.properties {
            let mut targets = Vec::new();
            direct_class_targets(&prop.ty, pool, &mut targets);
            targets.retain(|t| ctx.supported_classes.contains(t));
            if !targets.is_empty() {
                edges.entry(fqn.clone()).or_default().extend(targets.iter().cloned());
                field_targets.insert((fqn.clone(), prop.name.as_str().to_string()), targets);
            }
        }
    }

    // reaches(b, a): DFS over direct edges.
    let reaches = |from: &str, to: &str| -> bool {
        let mut stack = vec![from.to_string()];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(node) = stack.pop() {
            if node == to {
                return true;
            }
            if !seen.insert(node.clone()) {
                continue;
            }
            if let Some(next) = edges.get(&node) {
                stack.extend(next.iter().cloned());
            }
        }
        false
    };

    let mut boxed = BTreeSet::new();
    for ((class_fqn, field), targets) in &field_targets {
        if targets.iter().any(|t| reaches(t, class_fqn)) {
            boxed.insert((class_fqn.clone(), field.clone()));
        }
    }
    boxed
}

fn render_supported_class(
    class: &Class,
    key: &Name,
    ctx: &TranslateCtx,
    boxed_fields: &BTreeSet<(String, String)>,
) -> Option<String> {
    let fqn = key.to_string();
    let mut fields = Vec::new();
    for prop in &class.properties {
        fields.push(RenderedField {
            name: escape_ident(prop.name.as_str()),
            ty: translate_ty(&prop.ty, ctx)?,
            boxed: boxed_fields.contains(&(fqn.clone(), prop.name.as_str().to_string())),
            doc: prop.docstring.clone(),
        });
    }
    Some(render_class(class, key, &fields))
}

/// Backtick-escape Swift keywords that can appear as BAML identifiers.
pub(crate) fn escape_ident(name: &str) -> String {
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

fn render_root(root_decls: &BTreeMap<String, String>) -> String {
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
    for rendered in root_decls.values() {
        out.push('\n');
        out.push_str(&indent_lines(rendered, 1));
    }
    out.push_str("}\n");
    out
}

fn render_namespace_file(
    top: &str,
    ns_map: &BTreeMap<Vec<String>, BTreeMap<String, String>>,
) -> String {
    let mut out = String::from(
        "// Generated by BAML. DO NOT EDIT.\nimport BamlBridge\nimport Foundation\n\nextension Baml {\n",
    );
    out.push_str(&render_ns_enum(top, &[top.to_string()], ns_map, 1));
    out.push_str("}\n");
    out
}

/// Recursively render `enum <seg> { decls…; child enums… }`.
fn render_ns_enum(
    seg: &str,
    path: &[String],
    ns_map: &BTreeMap<Vec<String>, BTreeMap<String, String>>,
    depth: usize,
) -> String {
    let tab = "\t".repeat(depth);
    let mut out = format!("{tab}public enum {} {{\n", escape_ident(seg));
    if let Some(decls) = ns_map.get(path) {
        for rendered in decls.values() {
            out.push('\n');
            out.push_str(&indent_lines(rendered, depth + 1));
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
        let ctx = TranslateCtx {
            supported_classes: BTreeSet::new(),
            supported_enums: BTreeSet::new(),
            supported_aliases: BTreeSet::new(),
        };
        let t = |ty: &Ty| translate_ty(ty, &ctx);
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
        assert_eq!(
            t(&Ty::List(Box::new(Ty::Union(vec![Ty::Int, Ty::String])))),
            None
        );
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
