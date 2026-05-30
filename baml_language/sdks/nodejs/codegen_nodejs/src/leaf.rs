//! Per-leaf body grouping and Phase-2 placeholder rendering.
//!
//! `group_and_sort` buckets the emitted symbols by leaf and orders them
//! within each leaf (source span, then kind tie-break, then recursive
//! aliases hoisted first) — a faithful port of
//! `codegen_python/src/leaf.rs`. `render_leaf_body{,_dts}` emit only
//! placeholder lines in Phase 2; Phase 4 replaces them with real
//! `export class` / `defineFunction(...)` bodies.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{
    emit::{EmittedSymbol, SortKey, class::NodeClass, function::SyncAsync},
    routing::LeafPath,
};

/// All symbols that land in one leaf's body, in final render order.
/// Each entry keeps its `SortKey` so the renderer can group function
/// fan-out siblings tightly while separating unrelated definitions.
pub(crate) struct LeafBody {
    #[allow(dead_code)]
    pub(crate) leaf: LeafPath,
    pub(crate) symbols: Vec<(EmittedSymbol, SortKey)>,
}

pub(crate) fn group_and_sort(
    triples: Vec<(LeafPath, EmittedSymbol, SortKey)>,
) -> BTreeMap<LeafPath, LeafBody> {
    let mut buckets: BTreeMap<LeafPath, Vec<(EmittedSymbol, SortKey)>> = BTreeMap::new();
    for (leaf, sym, key) in triples {
        buckets.entry(leaf).or_default().push((sym, key));
    }

    let mut out: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
    for (leaf, mut pairs) in buckets {
        // Primary: source (file, span). Tertiary tie-break: type aliases
        // last so a forward reference to a same-leaf class resolves.
        pairs.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| symbol_kind_ord(&a.0).cmp(&symbol_kind_ord(&b.0)))
        });
        // Stable hoist: recursive aliases to the very front of the leaf
        // so a self-reference resolves after the alias declaration.
        pairs.sort_by_key(|(sym, _)| match sym {
            EmittedSymbol::TypeAlias(a) if a.recursive => 0u8,
            _ => 1,
        });
        out.insert(
            leaf.clone(),
            LeafBody {
                leaf,
                symbols: pairs,
            },
        );
    }
    out
}

fn symbol_kind_ord(sym: &EmittedSymbol) -> u8 {
    match sym {
        EmittedSymbol::TypeAlias(_) => 1,
        _ => 0,
    }
}

/// If `c` is one of the five runtime-owned stdlib types that codegen
/// re-exports (rather than emitting a generated body), return the
/// runtime export name (`BamlImage`, etc.). These resolve to the
/// runtime class identity in `@boundaryml/baml-core`; a generated
/// structural class would not round-trip (see `00a-spec` "Stdlib
/// Re-Exports").
fn media_reexport_node_name(c: &NodeClass) -> Option<&'static str> {
    match c.source.to_string().as_str() {
        "baml.media.Image" => Some("BamlImage"),
        "baml.media.Video" => Some("BamlVideo"),
        "baml.media.Audio" => Some("BamlAudio"),
        "baml.media.Pdf" => Some("BamlPdf"),
        "baml.llm.Stream" => Some("BamlStream"),
        _ => None,
    }
}

fn is_media_reexport(s: &EmittedSymbol) -> bool {
    match s {
        EmittedSymbol::Class(c) => media_reexport_node_name(c).is_some(),
        _ => false,
    }
}

fn mode_str(mode: SyncAsync) -> &'static str {
    match mode {
        SyncAsync::Sync => "sync",
        SyncAsync::Async => "async",
    }
}

/// Render a leaf body's runtime `.ts`. `emit_placeholder_import` is
/// `false` for the root leaf (whose preamble already imports
/// `BAML_PLACEHOLDER`) and `true` for every other leaf.
pub(crate) fn render_leaf_body(body: &LeafBody, emit_placeholder_import: bool) -> String {
    render_body(body, false, emit_placeholder_import)
}

/// Render a leaf body's type-only `.d.ts`. Never imports
/// `BAML_PLACEHOLDER` (type-position placeholders are `: any;`).
pub(crate) fn render_leaf_body_dts(body: &LeafBody) -> String {
    render_body(body, true, false)
}

fn render_body(body: &LeafBody, dts: bool, emit_placeholder_import: bool) -> String {
    let needs_import =
        emit_placeholder_import && body.symbols.iter().any(|(s, _)| !is_media_reexport(s));

    let mut out = String::new();
    if needs_import {
        out.push_str("import { BAML_PLACEHOLDER } from \"@boundaryml/baml-core\";\n");
    }

    let mut prev: Option<&SortKey> = None;
    for (sym, key) in &body.symbols {
        let new_group = prev.is_none_or(|p| p != key);
        if new_group && (prev.is_some() || needs_import) {
            out.push('\n');
        }
        render_symbol(&mut out, sym, dts);
        prev = Some(key);
    }

    out
}

fn render_symbol(out: &mut String, sym: &EmittedSymbol, dts: bool) {
    match sym {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = media_reexport_node_name(c) {
                let _ = writeln!(out, "// class {}", c.name);
                let _ = writeln!(
                    out,
                    "export {{ {rust_name} as {} }} from \"@boundaryml/baml-core\";",
                    c.name
                );
            } else {
                let _ = writeln!(out, "// class {}", c.name);
                write_placeholder(out, &c.name, dts);
            }
        }
        EmittedSymbol::Enum(e) => {
            let _ = writeln!(out, "// enum {}", e.name);
            write_placeholder(out, &e.name, dts);
        }
        EmittedSymbol::TypeAlias(a) => {
            let _ = writeln!(out, "// type {}", a.name);
            write_placeholder(out, &a.name, dts);
        }
        EmittedSymbol::Function(f) => {
            let _ = writeln!(out, "// function {} ({})", f.baml_fqn, mode_str(f.mode));
            write_placeholder(out, &f.name, dts);
        }
    }
}

/// ECMAScript reserved words that cannot be used as a `const`/binding
/// identifier in module (always strict-mode) code. They ARE valid as
/// export names (`export { x as new }`) and namespace aliases
/// (`export * as void`), so only the local binding needs mangling. A BAML
/// symbol named after one (e.g. the stdlib `baml.glob.new`) would otherwise
/// emit `export const new`, a syntax error. Python doesn't hit this — `new`
/// is not a Python keyword.
const JS_RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "await",
];

fn is_js_reserved(name: &str) -> bool {
    JS_RESERVED.contains(&name)
}

fn write_placeholder(out: &mut String, name: &str, dts: bool) {
    if is_js_reserved(name) {
        // `export const new` is a syntax error; bind a mangled local and
        // re-export it under the reserved name (legal as an export name).
        let local = format!("__baml_{name}");
        if dts {
            let _ = writeln!(out, "const {local}: any;");
        } else {
            let _ = writeln!(out, "const {local}: any = BAML_PLACEHOLDER;");
        }
        let _ = writeln!(out, "export {{ {local} as {name} }};");
    } else if dts {
        let _ = writeln!(out, "export const {name}: any;");
    } else {
        let _ = writeln!(out, "export const {name}: any = BAML_PLACEHOLDER;");
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::Name;

    use super::*;
    use crate::emit::{
        EmittedSymbol,
        class::NodeClass,
        enum_::NodeEnum,
        function::{NodeFunction, SyncAsync},
    };

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn leaf(segs: &[&str]) -> LeafPath {
        LeafPath {
            segments: segs.iter().copied().map(String::from).collect(),
        }
    }

    fn body(syms: Vec<(EmittedSymbol, SortKey)>) -> LeafBody {
        LeafBody {
            leaf: leaf(&["lorem"]),
            symbols: syms,
        }
    }

    fn class(n: &str, source: Name) -> EmittedSymbol {
        EmittedSymbol::Class(NodeClass {
            name: n.to_string(),
            source,
            generic_params: Vec::new(),
            docstring: None,
            properties: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
    }

    fn func(n: &str, fqn: &str, mode: SyncAsync) -> EmittedSymbol {
        EmittedSymbol::Function(NodeFunction {
            name: n.to_string(),
            baml_fqn: fqn.to_string(),
            mode,
            param_names: Vec::new(),
            arg_defaults: Vec::new(),
            arg_tys: Vec::new(),
            return_ty: baml_codegen_types::Ty::Int,
            generic_params: Vec::new(),
            docstring: None,
            raises_names: Vec::new(),
        })
    }

    #[test]
    fn class_body_renders() {
        let b = body(vec![(
            class("Resume", name("user", &["lorem"], "Resume")),
            ("a.baml".to_string(), 0),
        )]);
        let out = render_leaf_body(&b, true);
        assert!(out.contains("// class Resume\nexport const Resume: any = BAML_PLACEHOLDER;\n"));
        assert!(out.starts_with("import { BAML_PLACEHOLDER } from \"@boundaryml/baml-core\";\n"));
    }

    #[test]
    fn enum_body_renders() {
        let e = EmittedSymbol::Enum(NodeEnum {
            name: "Sentiment".to_string(),
            source: name("user", &["ipsum"], "Sentiment"),
            variants: Vec::new(),
            docstring: None,
        });
        let b = body(vec![(e, ("a.baml".to_string(), 0))]);
        let out = render_leaf_body(&b, true);
        assert!(
            out.contains("// enum Sentiment\nexport const Sentiment: any = BAML_PLACEHOLDER;\n")
        );
    }

    #[test]
    fn function_fans_out_sync_and_async_contiguously() {
        let b = body(vec![
            (
                func(
                    "extract_resume",
                    "user.lorem.extract_resume",
                    SyncAsync::Sync,
                ),
                ("a.baml".to_string(), 5),
            ),
            (
                func(
                    "extract_resume_async",
                    "user.lorem.extract_resume",
                    SyncAsync::Async,
                ),
                ("a.baml".to_string(), 5),
            ),
        ]);
        let out = render_leaf_body(&b, true);
        assert!(out.contains(
            "// function user.lorem.extract_resume (sync)\nexport const extract_resume: any = BAML_PLACEHOLDER;\n// function user.lorem.extract_resume (async)\nexport const extract_resume_async: any = BAML_PLACEHOLDER;\n"
        ));
    }

    #[test]
    fn dts_uses_type_position_placeholder() {
        let b = body(vec![(
            class("Resume", name("user", &["lorem"], "Resume")),
            ("a.baml".to_string(), 0),
        )]);
        let out = render_leaf_body_dts(&b);
        assert!(out.contains("// class Resume\nexport const Resume: any;\n"));
        assert!(!out.contains("BAML_PLACEHOLDER"));
    }

    #[test]
    fn media_class_renders_reexport_not_placeholder() {
        let b = body(vec![(
            class("Image", name("baml", &["media"], "Image")),
            ("a.baml".to_string(), 0),
        )]);
        let out = render_leaf_body(&b, true);
        assert!(out.contains("export { BamlImage as Image } from \"@boundaryml/baml-core\";"));
        assert!(!out.contains("BAML_PLACEHOLDER"));
    }
}
