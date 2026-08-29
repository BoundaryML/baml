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
//! with the runtime's `@BamlIndirect` `CoW` wrapper.
//!
//! Unlike Python (which binds callables at runtime with
//! `define_function`), Swift cannot synthesize functions, so this
//! generator emits real `func` bodies that call
//! `BamlRuntime.shared.callSync(...)` / `call(...)` from the
//! `BamlBridge` runtime package.

mod diagnostics;
mod emit;
mod translate_ty;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_base::qualified_name::AI_STREAM_STREAM;
use baml_codegen_types::{Class, Name, Symbol, SymbolPool, Ty, TypeAlias};
pub use baml_codegen_types::{NamingConvention, OutputType};
use base64::Engine as _;
use emit::{
    FnKind, RenderedField, indent_lines, render_callable, render_class, render_enum,
    render_type_alias, sort_key,
};
use translate_ty::{TranslateCtx, normalize_union, translate_ty};

/// Build the Swift SDK output tree using precompiled BAML bytecode as
/// the runtime payload. Returned paths are relative to the generated
/// `Sources/Baml/` root.
pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_with_optional_metadata(pool, baml_bytecode, None)
}

pub fn to_source_code_with_bytecode_and_metadata(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: &str,
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code_with_optional_metadata(pool, baml_bytecode, Some(embedded_baml_toml))
}

fn to_source_code_with_optional_metadata(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
) -> HashMap<PathBuf, String> {
    let mut out: HashMap<PathBuf, String> = HashMap::new();

    out.insert(
        PathBuf::from("_InlinedBaml.swift"),
        render_inlined_baml(baml_bytecode, embedded_baml_toml),
    );

    let ctx = build_translate_ctx(pool);
    let boxed_fields = compute_boxed_fields(pool, &ctx);

    // namespace path -> (sort_key -> rendered decl), BTree-sorted for
    // deterministic output.
    // Iterate in sorted key order for deterministic output (the pool is
    // a HashMap).
    let mut sorted_pool: Vec<(&Name, &Symbol)> = pool.iter().collect();
    sorted_pool.sort_by_key(|(key, _)| *key);

    // Every skip is recorded with the type that failed translation and
    // surfaced in the generated `_BamlSkipped.swift` manifest — absent
    // API must never be silent.
    let mut skips: Vec<diagnostics::Skip> = Vec::new();
    let free_callable_names = allocate_free_callable_names(pool);

    let mut namespaces: BTreeMap<Vec<String>, BTreeMap<String, String>> = BTreeMap::new();
    for (key, symbol) in sorted_pool {
        let fqn = key.to_string();
        // `ai.stream.Stream` is runtime-owned (BamlStream wraps the
        // engine handle) — never emitted as a generated struct.
        if fqn == AI_STREAM_STREAM {
            continue;
        }
        let mut ns = translate_ty::namespace_for(key);
        if key.is_stream() && matches!(symbol, Symbol::Function(_)) {
            // Only `$stream` CLASSES route under stream_types;
            // `$stream` FUNCTION companions sit beside their parent
            // (Python's routing rule, mirrored).
            ns.remove(0);
        }
        let rendered = match symbol {
            Symbol::Function(function) => {
                let binding_name = &free_callable_names[&fqn];
                let rendered =
                    render_callable(&key.to_string(), binding_name, function, FnKind::Free, &ctx);
                if rendered.is_none() {
                    skips.push(diagnostics::Skip {
                        fqn: fqn.clone(),
                        kind: "function",
                        reason: diagnostics::callable_skip_reason(function, &ctx),
                        is_user: key.package().as_str() == "user",
                    });
                }
                rendered
            }
            Symbol::Class(class) => {
                if !ctx.supported_classes.contains(&fqn) {
                    skips.push(diagnostics::Skip {
                        fqn: fqn.clone(),
                        kind: "class",
                        reason: diagnostics::class_skip_reason(class, &ctx),
                        is_user: key.package().as_str() == "user",
                    });
                    None
                } else {
                    render_supported_class(class, key, &ctx, &boxed_fields, &mut skips)
                }
            }
            Symbol::Enum(enum_) => Some(render_enum(enum_, key)),
            Symbol::TypeAlias(alias) => {
                if ctx.supported_aliases.contains(&fqn) {
                    render_alias(alias, key, &ctx)
                } else {
                    skips.push(diagnostics::Skip {
                        fqn: fqn.clone(),
                        kind: "type alias",
                        reason: diagnostics::alias_skip_reason(alias, &ctx),
                        is_user: key.package().as_str() == "user",
                    });
                    None
                }
            }
        };
        let Some(rendered) = rendered else { continue };
        // Sort key uses the RAW name: bare_name() strips `$stream`, so
        // a base function and its `$stream` companion would collide in
        // the decl map and silently overwrite each other.
        let bare = key.name().as_str().to_string();
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

    // A function and a child namespace can share a name in BAML
    // (Python separates module vs attribute lookup), but in Swift the
    // namespace enum and the func collide in one scope. The namespace
    // wins — it carries arbitrarily many symbols — and the colliding
    // function is dropped (e.g. vendor `boundary.id()` vs the
    // `boundary.id.*` namespace).
    let all_paths: Vec<Vec<String>> = namespaces.keys().cloned().collect();
    for path in &all_paths {
        if path.is_empty() {
            continue;
        }
        let (parent, seg) = (path[..path.len() - 1].to_vec(), &path[path.len() - 1]);
        if let Some(decls) = namespaces.get_mut(&parent) {
            if decls.remove(&format!("3:{seg}")).is_some() {
                skips.push(diagnostics::Skip {
                    fqn: path.join("."),
                    kind: "function",
                    is_user: !matches!(path[0].as_str(), "baml" | "vendor"),
                    reason: format!(
                        "name collides with the `{}` child namespace — in Swift \
                         both occupy one scope, and the namespace wins",
                        path.join(".")
                    ),
                });
            }
        }
    }

    skips.sort_by(|a, b| a.fqn.cmp(&b.fqn));
    out.insert(
        PathBuf::from("_BamlSkipped.swift"),
        diagnostics::render_manifest(&skips),
    );

    let root_decls = namespaces.remove(&Vec::new()).unwrap_or_default();
    // Named `BamlRoot.swift`, NOT `Baml.swift`: the stdlib namespace
    // emits `baml.swift`, and macOS filesystems are case-insensitive —
    // the two would clobber each other.
    out.insert(PathBuf::from("BamlRoot.swift"), render_root(&root_decls));

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
        match symbol {
            Symbol::Class(_) => {
                supported_classes.insert(key.to_string());
            }
            Symbol::Enum(_) => {
                supported_enums.insert(key.to_string());
            }
            // Non-recursive aliases become `typealias`; recursive ones
            // are representable only when union-backed (a nominal
            // family-shaped enum — `typealias` can't self-reference).
            Symbol::TypeAlias(alias)
                if !alias.recursive || recursive_union_alias_arms(alias).is_some() =>
            {
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
            nullable_aliases: nullable_aliases_for(pool, &supported_aliases),
        };
        let mut changed = false;
        for (key, symbol) in pool {
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
                Symbol::TypeAlias(alias)
                    if supported_aliases.contains(&fqn) && !alias_definition_ok(alias, &ctx) =>
                {
                    supported_aliases.remove(&fqn);
                    changed = true;
                }
                _ => {}
            }
        }
        if !changed {
            let nullable_aliases = nullable_aliases_for(pool, &supported_aliases);
            return TranslateCtx {
                supported_classes,
                supported_enums,
                supported_aliases,
                nullable_aliases,
            };
        }
    }
}

/// Direct (non-heap) class targets of a field type: bare class refs
/// and refs behind Optional (null-unions) store inline in a Swift
/// struct; List/Map contents are already heap-allocated and never
/// force boxing. Aliases resolve through (they're non-recursive here).
fn direct_class_targets(ty: &Ty, pool: &SymbolPool, out: &mut Vec<String>) {
    match ty {
        // Parameterized targets box exactly like bare ones —
        // `GenericLinkedList<T>` self-references store inline via
        // Optional the same way.
        Ty::Class(name, _, _) => out.push(name.to_string()),
        Ty::Union(members, _) => {
            let (non_null, _) = normalize_union(members);
            // A >=2-arm union renders as an `indirect` BamlUnionN — its
            // payload is heap-boxed, so it breaks cycles on its own.
            // Only the Optional collapse (1 arm) stores inline.
            if non_null.len() == 1 {
                direct_class_targets(&non_null[0], pool, out);
            }
        }
        Ty::TypeAlias(name, _) => {
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
        let Symbol::Class(class) = symbol else {
            continue;
        };
        let fqn = key.to_string();
        if !ctx.supported_classes.contains(&fqn) {
            continue;
        }
        for prop in &class.properties {
            let mut targets = Vec::new();
            direct_class_targets(&prop.ty, pool, &mut targets);
            targets.retain(|t| ctx.supported_classes.contains(t));
            if !targets.is_empty() {
                edges
                    .entry(fqn.clone())
                    .or_default()
                    .extend(targets.iter().cloned());
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
    skips: &mut Vec<diagnostics::Skip>,
) -> Option<String> {
    let fqn = key.to_string();
    let mut fields = Vec::new();
    for prop in &class.properties {
        fields.push(RenderedField {
            name: escape_ident(prop.name.as_str()),
            ty: translate_ty(&prop.ty, ctx)?,
            boxed: boxed_fields.contains(&(fqn.clone(), prop.name.as_str().to_string())),
            doc: prop.docstring.clone(),
            is_rust: matches!(prop.ty, Ty::RustType { .. }),
        });
    }

    // Methods skip individually when their signature is unsupported —
    // an unemittable method never drops the class (fields alone decide
    // supportability, matching Python).
    let mut methods = Vec::new();
    let method_names = allocate_callable_names(
        class
            .static_methods
            .iter()
            .chain(&class.instance_methods)
            .map(|method| method.name.as_str()),
    );
    for method in &class.static_methods {
        let method_fqn = format!("{fqn}.{}", method.name.as_str());
        if let Some(rendered) = render_callable(
            &method_fqn,
            &method_names[method.name.as_str()],
            method,
            FnKind::Static,
            ctx,
        ) {
            methods.push(rendered);
        } else {
            skips.push(diagnostics::Skip {
                fqn: method_fqn,
                kind: "static method",
                reason: diagnostics::callable_skip_reason(method, ctx),
                is_user: key.package().as_str() == "user",
            });
        }
    }
    for method in &class.instance_methods {
        let method_fqn = format!("{fqn}.{}", method.name.as_str());
        if let Some(rendered) = render_callable(
            &method_fqn,
            &method_names[method.name.as_str()],
            method,
            FnKind::Instance,
            ctx,
        ) {
            methods.push(rendered);
        } else {
            skips.push(diagnostics::Skip {
                fqn: method_fqn,
                kind: "instance method",
                reason: diagnostics::callable_skip_reason(method, ctx),
                is_user: key.package().as_str() == "user",
            });
        }
    }

    Some(render_class(class, key, &fields, &methods))
}

fn allocate_free_callable_names(pool: &SymbolPool) -> HashMap<String, String> {
    let mut scopes: BTreeMap<Vec<String>, Vec<(String, String)>> = BTreeMap::new();
    for (name, symbol) in pool {
        if !matches!(symbol, Symbol::Function(_)) {
            continue;
        }
        let mut namespace = translate_ty::namespace_for(name);
        if name.is_stream() {
            namespace.remove(0);
        }
        scopes
            .entry(namespace)
            .or_default()
            .push((name.to_string(), name.name().as_str().to_string()));
    }

    let mut allocated = HashMap::new();
    for callables in scopes.values() {
        let names = allocate_callable_names(callables.iter().map(|(_, raw)| raw.as_str()));
        for (fqn, raw) in callables {
            allocated.insert(fqn.clone(), names[raw].clone());
        }
    }
    allocated
}

/// Preserve authored Swift API names and suffix only synthesized companions
/// when replacing `@` with `_` would collide in the same declaration scope.
fn allocate_callable_names<'a>(
    raw_names: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, String> {
    let mut raw_names: Vec<&str> = raw_names.into_iter().collect();
    raw_names.sort_by_key(|name| (name.contains(['@', '$']), *name));

    let mut used = BTreeSet::new();
    let mut allocated = HashMap::new();
    for raw in raw_names {
        let base = raw.replace(['@', '$'], "_");
        let mut candidate = base.clone();
        let mut suffix = 2;
        while used.contains(&candidate) || used.contains(&format!("{candidate}_async")) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        used.insert(candidate.clone());
        used.insert(format!("{candidate}_async"));
        allocated.insert(raw.to_string(), candidate);
    }
    allocated
}

/// `Some(non-null arms)` when this alias is a recursive union with >=2
/// arms after normalization — the shape that gets a nominal
/// family-surface enum. Null-bearing recursive union aliases are
/// unsupported (the nominal enum can't carry the `?`; no fixture).
pub(crate) fn recursive_union_alias_arms(alias: &TypeAlias) -> Option<(Vec<Ty>, bool)> {
    if !alias.recursive {
        return None;
    }
    let Ty::Union(members, _) = &alias.resolves_to else {
        return None;
    };
    let (non_null, nullable) = normalize_union(members);
    (non_null.len() >= 2 && non_null.len() <= translate_ty::MAX_UNION_ARITY)
        .then_some((non_null, nullable))
}

/// Recursive union aliases whose union carries null (stdlib `json`):
/// their references need a `?` suffix.
fn nullable_aliases_for(
    pool: &SymbolPool,
    supported_aliases: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (key, symbol) in pool {
        let Symbol::TypeAlias(alias) = symbol else {
            continue;
        };
        let fqn = key.to_string();
        if supported_aliases.contains(&fqn)
            && matches!(recursive_union_alias_arms(alias), Some((_, true)))
        {
            out.insert(fqn);
        }
    }
    out
}

/// Can this alias's definition be emitted under `ctx`?
fn alias_definition_ok(alias: &TypeAlias, ctx: &TranslateCtx) -> bool {
    if let Some((arms, _)) = recursive_union_alias_arms(alias) {
        return arms.iter().all(|m| translate_ty(m, ctx).is_some());
    }
    !alias.recursive && translate_ty(&alias.resolves_to, ctx).is_some()
}

/// Emit one supported alias: recursive unions get a nominal enum with
/// the exact `BamlUnionN` surface under the USER'S name (never invented);
/// everything else is a plain `typealias` (union targets spell as
/// `BamlUnionN<...>` via `translate_ty`).
fn render_alias(alias: &TypeAlias, key: &Name, ctx: &TranslateCtx) -> Option<String> {
    if let Some((arms, _)) = recursive_union_alias_arms(alias) {
        return emit::render_recursive_union_alias(key, &arms, ctx);
    }
    render_type_alias(alias, key, ctx)
}

/// Backtick-escape Swift keywords that can appear as BAML identifiers.
pub(crate) fn escape_ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "associatedtype",
        "class",
        "deinit",
        "enum",
        "extension",
        "func",
        "import",
        "init",
        "inout",
        "internal",
        "let",
        "operator",
        "private",
        "protocol",
        "public",
        "static",
        "struct",
        "subscript",
        "typealias",
        "var",
        "break",
        "case",
        "continue",
        "default",
        "defer",
        "do",
        "else",
        "fallthrough",
        "for",
        "guard",
        "if",
        "in",
        "repeat",
        "return",
        "switch",
        "where",
        "while",
        "as",
        "catch",
        "false",
        "is",
        "nil",
        "rethrows",
        "self",
        "Self",
        "super",
        "throw",
        "throws",
        "true",
        "try",
        // Not keywords, but special in type positions when used as
        // nested type names (metatype syntax, existentials).
        "Type",
        "Protocol",
        "Any",
    ];
    if KEYWORDS.contains(&name) {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}

fn render_root(root_decls: &BTreeMap<String, String>) -> String {
    let mut out = format!(
        "// Generated by BAML. DO NOT EDIT.\n\
         import BamlBridge\nimport Foundation\n\n\
         /// Root namespace of the generated BAML SDK. Touching\n\
         /// `_initialized` (every generated entry point does) loads the\n\
         /// inlined bytecode into the native runtime exactly once.\n\
         public enum Baml {{\n\
         \t/// Canonical BAML product version this SDK was generated by.\n\
         \t/// `register_bridge` requires it to exactly match the linked\n\
         \t/// native library's version.\n\
         \tpublic static let sdkVersion = \"{version}\"\n\n\
         \tstatic let _initialized: Bool = {{\n\
         \t\tBamlRuntime.shared.initialize(\n\
         \t\t\tbytecode: _BamlInlined.bytecode,\n\
         \t\t\tsdkVersion: sdkVersion,\n\
         \t\t\tembeddedBamlToml: _BamlInlined.embeddedBamlToml\n\
         \t\t)\n\
         \t\treturn true\n\
         \t}}()\n",
        version = baml_version::CANONICAL_VERSION,
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
fn render_inlined_baml(baml_bytecode: &[u8], embedded_baml_toml: Option<&str>) -> String {
    // Fixed-width lines inside the literal for editor/diff friendliness.
    const CHUNK: usize = 96;
    let b64 = base64::engine::general_purpose::STANDARD.encode(baml_bytecode);
    let mut out = String::from(
        "// Generated by BAML. DO NOT EDIT.\n\
         import Foundation\n\n\
         enum _BamlInlined {\n    static let bytecodeBase64: String = \"\"\"\n",
    );
    for chunk in b64.as_bytes().chunks(CHUNK) {
        out.push_str("        ");
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ascii"));
        out.push('\n');
    }
    out.push_str("        \"\"\"\n");
    let manifest = embedded_baml_toml
        .map(|manifest| format!("Optional.some({manifest:?})"))
        .unwrap_or_else(|| "Optional.none".to_string());
    let _ = writeln!(out, "    static let embeddedBamlToml: String? = {manifest}");
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

        assert!(files[&PathBuf::from("BamlRoot.swift")].contains("public enum Baml"));
    }

    #[test]
    fn companion_bindings_do_not_collide_with_authored_swift_names() {
        let allocated = allocate_callable_names([
            "extract_spec",
            "extract@spec",
            "extract_stream_async",
            "extract@stream",
        ]);

        assert_eq!(allocated["extract_spec"], "extract_spec");
        assert_eq!(allocated["extract_stream_async"], "extract_stream_async");
        assert_eq!(allocated["extract@spec"], "extract_spec_2");
        assert_eq!(allocated["extract@stream"], "extract_stream_2");
    }

    fn int() -> Ty {
        Ty::Int {
            attr: baml_base::TyAttr::EMPTY,
        }
    }
    fn float() -> Ty {
        Ty::Float {
            attr: baml_base::TyAttr::EMPTY,
        }
    }
    fn string() -> Ty {
        Ty::String {
            attr: baml_base::TyAttr::EMPTY,
        }
    }
    fn null() -> Ty {
        Ty::Null {
            attr: baml_base::TyAttr::EMPTY,
        }
    }
    fn list(inner: Ty) -> Ty {
        Ty::List(Box::new(inner), baml_base::TyAttr::EMPTY)
    }
    fn map(key: Ty, value: Ty) -> Ty {
        Ty::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: baml_base::TyAttr::EMPTY,
        }
    }
    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members, baml_base::TyAttr::EMPTY)
    }
    fn literal(value: baml_base::Literal) -> Ty {
        Ty::Literal(
            value,
            baml_codegen_types::Freshness::Regular,
            baml_base::TyAttr::EMPTY,
        )
    }

    #[test]
    fn translate_ty_primitive_subset() {
        let ctx = TranslateCtx {
            supported_classes: BTreeSet::new(),
            supported_enums: BTreeSet::new(),
            supported_aliases: BTreeSet::new(),
            nullable_aliases: BTreeSet::new(),
        };
        let t = |ty: &Ty| translate_ty(ty, &ctx);
        assert_eq!(t(&int()).as_deref(), Some("Swift.Int"));
        assert_eq!(t(&float()).as_deref(), Some("Swift.Double"));
        assert_eq!(t(&list(int())).as_deref(), Some("[Swift.Int]"));
        assert_eq!(
            t(&map(string(), list(int()))).as_deref(),
            Some("[Swift.String: [Swift.Int]]")
        );
        // string?[] → [String?]
        assert_eq!(
            t(&list(union(vec![string(), null()]))).as_deref(),
            Some("[Swift.String?]")
        );
        // (int | string)[] — family reference, inline, no registry.
        assert_eq!(
            t(&list(union(vec![int(), string()]))).as_deref(),
            Some("[BamlUnion2<Swift.Int, Swift.String>]")
        );

        let stream_name = Name::new(
            baml_base::Name::new("ai"),
            vec![baml_base::Name::new("stream")],
            baml_base::Name::new("Stream"),
        );
        let stream = Ty::Class(
            stream_name,
            vec![string(), string()],
            baml_base::TyAttr::EMPTY,
        );
        assert_eq!(
            t(&stream).as_deref(),
            Some("BamlStream<Swift.String, Swift.String>")
        );
        // Same shape is the same type everywhere (structural identity).
        assert_eq!(
            t(&union(vec![int(), string(), null()])).as_deref(),
            Some("BamlUnion2<Swift.Int, Swift.String>?")
        );
        // int | int → dedup + singleton collapse.
        assert_eq!(t(&union(vec![int(), int()])).as_deref(), Some("Swift.Int"));
        // Literal-only unions collapse to their base type — no raw enums.
        assert_eq!(
            t(&union(vec![
                literal(baml_base::Literal::String("draft".into())),
                literal(baml_base::Literal::String("sent".into())),
            ]))
            .as_deref(),
            Some("Swift.String")
        );
        // map with non-string key — not yet
        assert_eq!(t(&map(int(), int())), None);
    }
}
