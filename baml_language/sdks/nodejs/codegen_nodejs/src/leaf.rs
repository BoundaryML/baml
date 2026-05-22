//! Per-leaf symbol bundle and Phase 4 renderer. Emits real TypeScript
//! bodies (classes, enums, type aliases, function bindings) plus the
//! cross-leaf imports needed to resolve them.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use baml_codegen_types::Ty;

use crate::{
    emit::{
        EmittedSymbol, SortKey,
        class::NodeClass,
        enum_::NodeEnum,
        function::{NodeFunction, SyncAsync},
        method::{MethodKind, NodeMethodBinding},
        type_alias::NodeTypeAlias,
    },
    routing::LeafPath,
    translate_ty::{TranslateCtx, translate_ty},
    ts_string,
};

pub(crate) struct LeafBody {
    pub(crate) leaf: LeafPath,
    pub(crate) symbols: Vec<(EmittedSymbol, SortKey)>,
}

impl LeafBody {
    pub(crate) fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// True iff this leaf has any function / static-method / instance-method
    /// binding — the trigger for `import { defineFunction } from …`.
    fn needs_define_function(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Function(_) => true,
            EmittedSymbol::Class(c) => {
                !c.static_methods.is_empty() || !c.instance_methods.is_empty()
            }
            _ => false,
        })
    }
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
        pairs.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| symbol_kind_ord(&a.0).cmp(&symbol_kind_ord(&b.0)))
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

/// Accumulator passed through every per-symbol renderer. Holds both
/// namespace-leaf imports (`imports`) and bare root-name imports
/// (`root_names`) so the renderer can stay imperative without juggling
/// two mutable references per call.
#[derive(Default)]
struct ImportSink {
    imports: BTreeSet<LeafPath>,
    root_names: BTreeSet<String>,
}

impl ImportSink {
    fn merge_from(&mut self, t: &crate::translate_ty::TranslatedType) {
        self.imports.extend(t.imports.iter().cloned());
        self.root_names.extend(t.root_names.iter().cloned());
    }
}

/// Translate every type expression that the leaf will render, collect
/// the imports needed, and group by [`LeafPath`].
struct RenderedBody {
    /// Per-symbol rendered chunks (already include the `JSDoc` + the symbol body).
    chunks: Vec<String>,
    sink: ImportSink,
}

fn render_body(body: &LeafBody, is_dts: bool) -> RenderedBody {
    let ctx = TranslateCtx {
        current_leaf: body.leaf.clone(),
    };
    let mut chunks: Vec<String> = Vec::new();
    let mut sink = ImportSink::default();
    for (sym, _) in &body.symbols {
        let mut out = String::new();
        match sym {
            EmittedSymbol::TypeAlias(a) => {
                render_type_alias(&mut out, a, &ctx, &mut sink);
            }
            EmittedSymbol::Enum(e) => {
                render_enum(&mut out, e, is_dts);
            }
            EmittedSymbol::Class(c) => {
                render_class(&mut out, c, &ctx, &mut sink, is_dts);
            }
            EmittedSymbol::Function(f) => {
                render_function(&mut out, f, &ctx, &mut sink, is_dts);
            }
        }
        chunks.push(out);
    }
    RenderedBody { chunks, sink }
}

/// Render the runtime `index.ts` body for a leaf.
pub(crate) fn render_leaf_body(body: &LeafBody) -> String {
    render_leaf(body, /*is_dts=*/ false)
}

/// Render the type-position `index.d.ts` body for a leaf.
pub(crate) fn render_leaf_body_dts(body: &LeafBody) -> String {
    render_leaf(body, /*is_dts=*/ true)
}

fn render_leaf(body: &LeafBody, is_dts: bool) -> String {
    if body.is_empty() {
        return String::new();
    }
    let rendered = render_body(body, is_dts);
    let chunks = rendered.chunks;

    let mut out = String::new();
    write_import_block(&mut out, &body.leaf, &rendered.sink);
    if !is_dts && body.needs_define_function() {
        out.push_str("import { defineFunction } from \"@boundaryml/baml-node\";\n");
    }
    if body_uses_rust_handle(&chunks) {
        // Always type-only — the runtime handle class is referenced only
        // as a field type, never as a value, in leaf bodies.
        out.push_str("import type { BamlHandle as _BamlHandle } from \"@boundaryml/baml-node\";\n");
    }
    if !out.is_empty() {
        out.push('\n');
    }
    for chunk in chunks {
        out.push_str(&chunk);
    }
    out
}

fn body_uses_rust_handle(chunks: &[String]) -> bool {
    chunks.iter().any(|c| c.contains("_BamlHandle"))
}

/// Emit the cross-leaf import block.
///
/// Each cross-leaf reference resolves through the *top-level* segment
/// of its routed leaf, mirroring the Python codegen's
/// `from <root_dots> import <top>` line. The use site keeps the full
/// dotted form (`symbol_collisions.fizz.buzz.foo.Bar`) so the resulting
/// `.ts` reads like the BAML source.
///
/// All cross-leaf uses inside a leaf body sit in type positions — class
/// field types, `as (…) => R` casts, constructor signatures — so the
/// imports are always `import type * as`. Erasing them at runtime sidesteps
/// the circular-load cycle that would otherwise occur when a leaf inside
/// `<top>/…/` imports `<top>` itself (whose `index.ts` re-exports the
/// current leaf).
fn write_import_block(out: &mut String, current: &LeafPath, sink: &ImportSink) {
    let mut top_segments: BTreeSet<&String> = BTreeSet::new();
    for imp in &sink.imports {
        if let Some(first) = imp.segments.first() {
            top_segments.insert(first);
        }
    }
    for top in &top_segments {
        let target = LeafPath {
            segments: vec![(*top).clone()],
        };
        let rel = relative_import_path(current, &target);
        let _ = writeln!(out, "import type * as {top} from \"{rel}\";");
    }
    if !sink.root_names.is_empty() && !current.segments.is_empty() {
        let rel = relative_import_path(
            current,
            &LeafPath {
                segments: Vec::new(),
            },
        );
        let names: Vec<&String> = sink.root_names.iter().collect();
        let _ = writeln!(
            out,
            "import type {{ {} }} from \"{rel}\";",
            names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// Compute the relative TS import path from `current` to `target` such that
/// `import * as foo from <rel>` resolves under `CommonJS` module resolution.
///
/// Both `current` and `target` are leaf segments under `baml_sdk/`; both
/// resolve to `<dir>/index.ts`. Examples:
///
/// - `current=["lorem"]`, `target=["ipsum"]` → `"../ipsum"`
/// - `current=["lorem"]`, `target=["stream_types","lorem"]` → `"../stream_types/lorem"`
/// - `current=[]`, `target=["lorem"]` → `"./lorem"`
/// - `current=["a","b"]`, `target=["a","b","c"]` → `"./c"`
fn relative_import_path(current: &LeafPath, target: &LeafPath) -> String {
    let cur = &current.segments;
    let tgt = &target.segments;
    let mut common = 0usize;
    while common < cur.len() && common < tgt.len() && cur[common] == tgt[common] {
        common += 1;
    }
    let ups = cur.len() - common;
    let remainder = &tgt[common..];
    if ups == 0 {
        if remainder.is_empty() {
            "./".to_string()
        } else {
            format!("./{}", remainder.join("/"))
        }
    } else {
        let mut s = String::new();
        for _ in 0..ups {
            s.push_str("../");
        }
        s.push_str(&remainder.join("/"));
        // Strip trailing slash if remainder was empty.
        s.trim_end_matches('/').to_string()
    }
}

// ── Per-kind renderers ──────────────────────────────────────────────────

fn render_type_alias(
    out: &mut String,
    a: &NodeTypeAlias,
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
) {
    let rhs = translate_ty(&a.resolves_to, ctx);
    sink.merge_from(&rhs);
    let _ = writeln!(out, "export type {} = {};", a.name, rhs.expr);
}

fn render_enum(out: &mut String, e: &NodeEnum, is_dts: bool) {
    write_doc_comment(out, e.docstring.as_deref(), 0);
    let keyword = if is_dts {
        "export declare enum"
    } else {
        "export enum"
    };
    let _ = writeln!(out, "{keyword} {} {{", e.name);
    for v in &e.variants {
        write_doc_comment(out, v.docstring.as_deref(), 4);
        let _ = writeln!(out, "    {} = {},", v.ident, ts_string(&v.value));
    }
    out.push_str("}\n");
}

fn render_class(
    out: &mut String,
    c: &NodeClass,
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
    is_dts: bool,
) {
    write_class_doc(out, c);
    let keyword = if is_dts {
        "export declare class"
    } else {
        "export class"
    };
    let generics = if c.generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", c.generic_params.join(", "))
    };
    let _ = writeln!(out, "{keyword} {}{} {{", c.name, generics);

    // Fields
    let mut field_types: Vec<(String, String)> = Vec::new();
    for p in &c.properties {
        let t = translate_ty(&p.ty, ctx);
        sink.merge_from(&t);
        write_doc_comment(out, p.docstring.as_deref(), 4);
        let suffix = if is_dts { "" } else { "!" };
        let _ = writeln!(out, "    {}{}: {};", p.name, suffix, t.expr);
        field_types.push((p.name.clone(), t.expr));
    }

    // Constructor
    if !c.properties.is_empty() || c.static_methods.is_empty() && c.instance_methods.is_empty() {
        let init_obj = field_types
            .iter()
            .map(|(n, t)| format!("{n}: {t}"))
            .collect::<Vec<_>>()
            .join("; ");
        if is_dts {
            let _ = writeln!(out, "    constructor(init: {{ {init_obj} }});");
        } else {
            out.push('\n');
            let _ = writeln!(
                out,
                "    constructor(init: {{ {init_obj} }}) {{ Object.assign(this, init); }}"
            );
        }
    }

    // Static methods
    if !c.static_methods.is_empty() {
        out.push('\n');
    }
    for m in &c.static_methods {
        render_method_in_class(out, c, m, ctx, sink, is_dts);
    }

    // Instance methods
    if !c.instance_methods.is_empty() {
        out.push('\n');
    }
    for m in &c.instance_methods {
        render_method_in_class(out, c, m, ctx, sink, is_dts);
    }

    out.push_str("}\n");
}

fn render_method_in_class(
    out: &mut String,
    parent: &NodeClass,
    m: &NodeMethodBinding,
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
    is_dts: bool,
) {
    let signature = render_method_signature(m, &parent.generic_params, ctx, sink);
    let mode = sync_async_str(m.mode);
    let factory_params = render_param_array(&m.param_names);
    match (m.kind, is_dts) {
        (MethodKind::Static, true) => {
            let _ = writeln!(out, "    static {}: {};", m.name, signature.typed);
        }
        (MethodKind::Static, false) => {
            let _ = writeln!(
                out,
                "    static {} = defineFunction({}, \"{}\", {}) as {};",
                m.name,
                ts_string(&m.baml_fqn),
                mode,
                factory_params,
                signature.typed,
            );
        }
        (MethodKind::Instance, true) => {
            let _ = writeln!(
                out,
                "    {}{}({}): {};",
                m.name, signature.generics, signature.public_params, signature.public_return,
            );
        }
        (MethodKind::Instance, false) => {
            // Public shim forwards through the private static factory.
            // `this` is positional arg 0, matching `paramNames[0] === "self"`.
            let arg_call = if signature.public_arg_names.is_empty() {
                "this".to_string()
            } else {
                format!("this, {}", signature.public_arg_names.join(", "))
            };
            let _ = writeln!(
                out,
                "    {name}{generics}({params}): {ret} {{ return ({cls}._{name} as unknown as (...args: unknown[]) => unknown)({arg_call}) as {ret}; }}",
                name = m.name,
                generics = signature.generics,
                params = signature.public_params,
                ret = signature.public_return,
                cls = parent.name,
                arg_call = arg_call,
            );
            let _ = writeln!(
                out,
                "    private static _{} = defineFunction({}, \"{}\", {});",
                m.name,
                ts_string(&m.baml_fqn),
                mode,
                factory_params,
            );
        }
    }
}

fn render_function(
    out: &mut String,
    f: &NodeFunction,
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
    is_dts: bool,
) {
    let signature = render_function_signature(f, ctx, sink);
    if is_dts {
        let _ = writeln!(out, "export declare const {}: {};", f.name, signature.typed);
    } else {
        let mode = sync_async_str(f.mode);
        let factory_params = render_param_array(&f.param_names);
        let _ = writeln!(
            out,
            "export const {} = defineFunction({}, \"{}\", {}) as {};",
            f.name,
            ts_string(&f.baml_fqn),
            mode,
            factory_params,
            signature.typed,
        );
    }
}

/// Built typed signatures for a free function or static-method binding.
/// `typed` is the full `(arg: T, …) => R` (or `=> Promise<R>` for async)
/// used at the `as` assertion site and in the `.d.ts` line.
struct FunctionSignature {
    typed: String,
}

fn render_function_signature(
    f: &NodeFunction,
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
) -> FunctionSignature {
    let params = render_typed_params(&f.param_names, &f.arg_tys, ctx, sink);
    let ret = translate_ty(&f.return_ty, ctx);
    sink.merge_from(&ret);
    let ret_str = match f.mode {
        SyncAsync::Sync => ret.expr,
        SyncAsync::Async => format!("Promise<{}>", ret.expr),
    };
    let generics = render_generic_clause(&f.generic_params);
    FunctionSignature {
        typed: format!("{generics}({params}) => {ret_str}"),
    }
}

/// Components of a method signature. `typed` is the `static`-form
/// `<G>(arg: T) => R` line used by static methods; `public_params` and
/// `public_return` drive the instance-method `name<G>(p: T, …): R`
/// shape; `public_arg_names` lists the post-`self` arg identifiers
/// (after reserved-word sanitization) for the instance-method shim body;
/// `generics` is the `<G, …>` clause inserted between the method name
/// and its parameter list.
struct MethodSignature {
    typed: String,
    public_params: String,
    public_return: String,
    public_arg_names: Vec<String>,
    generics: String,
}

fn render_method_signature(
    m: &NodeMethodBinding,
    parent_generic_params: &[String],
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
) -> MethodSignature {
    // Public signature names: skip the leading `"self"` for instance
    // methods. Static methods carry their full param list.
    let public_names_raw: Vec<String> = match m.kind {
        MethodKind::Static => m.param_names.clone(),
        MethodKind::Instance => m.param_names.iter().skip(1).cloned().collect(),
    };
    let public_names: Vec<String> = public_names_raw
        .iter()
        .map(|n| sanitize_param_ident(n))
        .collect();
    let public_params = render_typed_params(&public_names, &m.arg_tys, ctx, sink);
    let ret = translate_ty(&m.return_ty, ctx);
    sink.merge_from(&ret);
    let public_return = match m.mode {
        SyncAsync::Sync => ret.expr.clone(),
        SyncAsync::Async => format!("Promise<{}>", ret.expr),
    };
    // Static methods can't reference the class's type parameters at the
    // class body's static slot (TS2302). Re-declare the parent class's
    // generics on the static signature itself so each call instantiates
    // them fresh.
    let mut generic_params: Vec<String> = match m.kind {
        MethodKind::Static => parent_generic_params.to_vec(),
        MethodKind::Instance => Vec::new(),
    };
    for g in &m.generic_params {
        if !generic_params.contains(g) {
            generic_params.push(g.clone());
        }
    }
    let generics = render_generic_clause(&generic_params);
    let typed = format!("{generics}({public_params}) => {public_return}");
    MethodSignature {
        typed,
        public_params,
        public_return,
        public_arg_names: public_names,
        generics,
    }
}

fn render_typed_params(
    names: &[String],
    tys: &[Ty],
    ctx: &TranslateCtx,
    sink: &mut ImportSink,
) -> String {
    debug_assert_eq!(names.len(), tys.len());
    let mut parts = Vec::with_capacity(names.len());
    for (n, t) in names.iter().zip(tys.iter()) {
        let translated = translate_ty(t, ctx);
        sink.merge_from(&translated);
        parts.push(format!("{}: {}", sanitize_param_ident(n), translated.expr));
    }
    parts.join(", ")
}

fn render_generic_clause(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

/// Sanitize a parameter identifier for use as a TS function/method
/// parameter binding. JS reserved words (`default`, `class`, …) can't
/// appear as binding identifiers; append a trailing underscore. The
/// kwargs key on the wire (which the factory's `paramNames` carries)
/// is left untouched.
fn sanitize_param_ident(name: &str) -> String {
    if is_reserved_param_word(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

fn is_reserved_param_word(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

fn render_param_array(names: &[String]) -> String {
    if names.is_empty() {
        return "[]".to_string();
    }
    let mut out = String::from("[");
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&ts_string(n));
    }
    out.push(']');
    out
}

fn sync_async_str(m: SyncAsync) -> &'static str {
    match m {
        SyncAsync::Sync => "sync",
        SyncAsync::Async => "async",
    }
}

fn write_class_doc(out: &mut String, c: &NodeClass) {
    let has_summary = c.docstring.as_deref().is_some_and(|s| !s.is_empty());
    let has_field_docs = c.properties.iter().any(|p| p.docstring.is_some());
    if !has_summary && !has_field_docs {
        return;
    }
    out.push_str("/**\n");
    if let Some(s) = c.docstring.as_deref() {
        for line in s.lines() {
            let _ = writeln!(out, " * {line}");
        }
    }
    if has_field_docs {
        if has_summary {
            out.push_str(" *\n");
        }
        for p in &c.properties {
            if let Some(d) = &p.docstring {
                let _ = writeln!(out, " * @property {} - {}", p.name, d);
            }
        }
    }
    out.push_str(" */\n");
}

fn write_doc_comment(out: &mut String, doc: Option<&str>, indent: usize) {
    let Some(s) = doc else { return };
    if s.is_empty() {
        return;
    }
    let pad = " ".repeat(indent);
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() == 1 {
        let _ = writeln!(out, "{pad}/** {} */", lines[0]);
    } else {
        let _ = writeln!(out, "{pad}/**");
        for line in lines {
            let _ = writeln!(out, "{pad} * {line}");
        }
        let _ = writeln!(out, "{pad} */");
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{Name, Ty};

    use super::*;
    use crate::emit::{class::NodeClassProperty, enum_::NodeEnumVariant};

    fn cg_name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn body_for(leaf_segs: &[&str], sym: EmittedSymbol) -> LeafBody {
        LeafBody {
            leaf: LeafPath {
                segments: leaf_segs.iter().map(|&s| s.to_string()).collect(),
            },
            symbols: vec![(sym, ("x.baml".to_string(), 0))],
        }
    }

    #[test]
    fn type_alias_emits_real_alias() {
        let n = cg_name("user", &["lorem"], "MyList");
        let b = body_for(
            &["lorem"],
            EmittedSymbol::TypeAlias(NodeTypeAlias {
                name: "MyList".to_string(),
                source: n,
                resolves_to: Ty::List(Box::new(Ty::Int)),
                recursive: false,
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(ts.contains("export type MyList = Array<number>;\n"), "{ts}");
        let dts = render_leaf_body_dts(&b);
        assert!(
            dts.contains("export type MyList = Array<number>;\n"),
            "{dts}"
        );
    }

    #[test]
    fn enum_emits_string_enum() {
        let n = cg_name("user", &["lorem"], "Sentiment");
        let b = body_for(
            &["lorem"],
            EmittedSymbol::Enum(NodeEnum {
                name: "Sentiment".to_string(),
                source: n,
                variants: vec![
                    NodeEnumVariant {
                        ident: "POSITIVE".to_string(),
                        value: "POSITIVE".to_string(),
                        docstring: None,
                    },
                    NodeEnumVariant {
                        ident: "NEGATIVE".to_string(),
                        value: "NEGATIVE".to_string(),
                        docstring: None,
                    },
                ],
                docstring: None,
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(ts.contains("export enum Sentiment {"), "{ts}");
        assert!(ts.contains("    POSITIVE = \"POSITIVE\",\n"), "{ts}");
        let dts = render_leaf_body_dts(&b);
        assert!(dts.contains("export declare enum Sentiment {"), "{dts}");
    }

    #[test]
    fn class_with_fields_and_constructor() {
        let n = cg_name("user", &["lorem"], "Resume");
        let b = body_for(
            &["lorem"],
            EmittedSymbol::Class(NodeClass {
                name: "Resume".to_string(),
                source: n,
                generic_params: vec![],
                docstring: None,
                properties: vec![
                    NodeClassProperty {
                        name: "name".to_string(),
                        ty: Ty::String,
                        docstring: None,
                    },
                    NodeClassProperty {
                        name: "tags".to_string(),
                        ty: Ty::List(Box::new(Ty::String)),
                        docstring: None,
                    },
                ],
                static_methods: vec![],
                instance_methods: vec![],
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(ts.contains("export class Resume {\n"), "{ts}");
        assert!(ts.contains("    name!: string;\n"), "{ts}");
        assert!(ts.contains("    tags!: Array<string>;\n"), "{ts}");
        assert!(
            ts.contains(
                "constructor(init: { name: string; tags: Array<string> }) { Object.assign(this, init); }"
            ),
            "{ts}"
        );

        let dts = render_leaf_body_dts(&b);
        assert!(dts.contains("export declare class Resume {\n"), "{dts}");
        assert!(dts.contains("    name: string;\n"), "{dts}");
        assert!(dts.contains("    constructor(init: { "), "{dts}");
    }

    #[test]
    fn function_emits_define_function_with_typed_signature() {
        let b = body_for(
            &["lorem"],
            EmittedSymbol::Function(NodeFunction {
                name: "extract_resume".to_string(),
                baml_fqn: "user.lorem.extract_resume".to_string(),
                mode: SyncAsync::Sync,
                param_names: vec!["text".to_string()],
                arg_defaults: vec![None],
                arg_tys: vec![Ty::String],
                return_ty: Ty::Int,
                generic_params: vec![],
                docstring: None,
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(ts.contains("import { defineFunction } from"), "{ts}");
        // Param names + typed assertion: regression guard for the
        // empty-param-array bug.
        assert!(
            ts.contains(
                "export const extract_resume = defineFunction(\"user.lorem.extract_resume\", \"sync\", [\"text\"]) as (text: string) => number;"
            ),
            "{ts}"
        );
        let dts = render_leaf_body_dts(&b);
        assert!(
            dts.contains("export declare const extract_resume: (text: string) => number;"),
            "{dts}"
        );
    }

    #[test]
    fn function_async_wraps_return_in_promise() {
        let b = body_for(
            &["lorem"],
            EmittedSymbol::Function(NodeFunction {
                name: "extract_resume_async".to_string(),
                baml_fqn: "user.lorem.extract_resume".to_string(),
                mode: SyncAsync::Async,
                param_names: vec!["text".to_string()],
                arg_defaults: vec![None],
                arg_tys: vec![Ty::String],
                return_ty: Ty::Int,
                generic_params: vec![],
                docstring: None,
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(
            ts.contains(
                "export const extract_resume_async = defineFunction(\"user.lorem.extract_resume\", \"async\", [\"text\"]) as (text: string) => Promise<number>;"
            ),
            "{ts}"
        );
        let dts = render_leaf_body_dts(&b);
        assert!(
            dts.contains(
                "export declare const extract_resume_async: (text: string) => Promise<number>;"
            ),
            "{dts}"
        );
    }

    #[test]
    fn function_with_union_param_emits_full_union_type() {
        // The bug-symptom case from ns_unions: function signatures had
        // empty param lists and no type assertion.
        let b = body_for(
            &["unions"],
            EmittedSymbol::Function(NodeFunction {
                name: "RoundTripNullToEnd".to_string(),
                baml_fqn: "user.unions.RoundTripNullToEnd".to_string(),
                mode: SyncAsync::Sync,
                param_names: vec!["u".to_string()],
                arg_defaults: vec![None],
                arg_tys: vec![Ty::Union(vec![Ty::Int, Ty::String, Ty::Null])],
                return_ty: Ty::Union(vec![Ty::Int, Ty::String, Ty::Null]),
                generic_params: vec![],
                docstring: None,
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(
            ts.contains(
                "export const RoundTripNullToEnd = defineFunction(\"user.unions.RoundTripNullToEnd\", \"sync\", [\"u\"]) as (u: number | string | null) => number | string | null;"
            ),
            "{ts}"
        );
        let dts = render_leaf_body_dts(&b);
        assert!(
            dts.contains(
                "export declare const RoundTripNullToEnd: (u: number | string | null) => number | string | null;"
            ),
            "{dts}"
        );
    }

    #[test]
    fn static_method_emits_typed_factory() {
        use crate::emit::method::{MethodKind, NodeMethodBinding};
        let cls_name = cg_name("user", &["lorem"], "Pdf");
        let b = body_for(
            &["lorem"],
            EmittedSymbol::Class(NodeClass {
                name: "Pdf".to_string(),
                source: cls_name,
                generic_params: vec![],
                docstring: None,
                properties: vec![],
                static_methods: vec![NodeMethodBinding {
                    name: "from_url".to_string(),
                    baml_fqn: "user.lorem.Pdf.from_url".to_string(),
                    mode: SyncAsync::Sync,
                    param_names: vec!["url".to_string()],
                    arg_defaults: vec![None],
                    kind: MethodKind::Static,
                    arg_tys: vec![Ty::String],
                    return_ty: Ty::Class(cg_name("user", &["lorem"], "Pdf"), vec![]),
                    generic_params: vec![],
                    docstring: None,
                }],
                instance_methods: vec![],
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(
            ts.contains(
                "static from_url = defineFunction(\"user.lorem.Pdf.from_url\", \"sync\", [\"url\"]) as (url: string) => Pdf;"
            ),
            "{ts}"
        );
        let dts = render_leaf_body_dts(&b);
        assert!(
            dts.contains("static from_url: (url: string) => Pdf;"),
            "{dts}"
        );
    }

    #[test]
    fn instance_method_emits_typed_public_shim_and_private_factory() {
        use crate::emit::method::{MethodKind, NodeMethodBinding};
        let cls_name = cg_name("user", &["lorem"], "File");
        let b = body_for(
            &["lorem"],
            EmittedSymbol::Class(NodeClass {
                name: "File".to_string(),
                source: cls_name,
                generic_params: vec![],
                docstring: None,
                properties: vec![],
                static_methods: vec![],
                instance_methods: vec![NodeMethodBinding {
                    name: "tag".to_string(),
                    baml_fqn: "user.lorem.File.tag".to_string(),
                    mode: SyncAsync::Sync,
                    param_names: vec!["self".to_string(), "label".to_string()],
                    arg_defaults: vec![None],
                    kind: MethodKind::Instance,
                    arg_tys: vec![Ty::String],
                    return_ty: Ty::Bool,
                    generic_params: vec![],
                    docstring: None,
                }],
            }),
        );
        let ts = render_leaf_body(&b);
        assert!(
            ts.contains(
                "tag(label: string): boolean { return (File._tag as unknown as (...args: unknown[]) => unknown)(this, label) as boolean; }"
            ),
            "{ts}"
        );
        assert!(
            ts.contains(
                "private static _tag = defineFunction(\"user.lorem.File.tag\", \"sync\", [\"self\", \"label\"]);"
            ),
            "{ts}"
        );
        let dts = render_leaf_body_dts(&b);
        assert!(dts.contains("tag(label: string): boolean;"), "{dts}");
        // No private factory in the .d.ts surface.
        assert!(!dts.contains("private static _tag"), "{dts}");
    }

    #[test]
    fn function_with_cross_leaf_class_return_emits_import() {
        let resume_name = cg_name("user", &["lorem"], "Resume");
        let b = body_for(
            &["unions"],
            EmittedSymbol::Function(NodeFunction {
                name: "ProcessResume".to_string(),
                baml_fqn: "user.unions.ProcessResume".to_string(),
                mode: SyncAsync::Sync,
                param_names: vec!["r".to_string()],
                arg_defaults: vec![None],
                arg_tys: vec![Ty::Class(resume_name.clone(), vec![])],
                return_ty: Ty::Class(resume_name, vec![]),
                generic_params: vec![],
                docstring: None,
            }),
        );
        let ts = render_leaf_body(&b);
        // Cross-leaf class refs must be imported so the typed signature
        // resolves: previous behaviour emitted untyped (`any`) bindings
        // and silently swallowed the missing import.
        assert!(
            ts.contains("import type * as lorem from \"../lorem\";"),
            "{ts}"
        );
        assert!(
            ts.contains(
                "export const ProcessResume = defineFunction(\"user.unions.ProcessResume\", \"sync\", [\"r\"]) as (r: lorem.Resume) => lorem.Resume;"
            ),
            "{ts}"
        );
        let dts = render_leaf_body_dts(&b);
        assert!(
            dts.contains("import type * as lorem from \"../lorem\";"),
            "{dts}"
        );
        assert!(
            dts.contains("export declare const ProcessResume: (r: lorem.Resume) => lorem.Resume;"),
            "{dts}"
        );
    }

    #[test]
    fn cross_leaf_import_emitted() {
        // Class in `lorem` referenced via field type in `aliases_consumer`.
        // Cross-leaf imports are always type-only (every use sits in a
        // type position) so the same `import type * as …` line shows up
        // in both the runtime `.ts` and the slim `.d.ts`.
        let resume_name = cg_name("user", &["lorem"], "Resume");
        let consumer = body_for(
            &["aliases_consumer"],
            EmittedSymbol::Class(NodeClass {
                name: "Wrapper".to_string(),
                source: cg_name("user", &["aliases_consumer"], "Wrapper"),
                generic_params: vec![],
                docstring: None,
                properties: vec![NodeClassProperty {
                    name: "inner".to_string(),
                    ty: Ty::Class(resume_name, vec![]),
                    docstring: None,
                }],
                static_methods: vec![],
                instance_methods: vec![],
            }),
        );
        let ts = render_leaf_body(&consumer);
        assert!(
            ts.contains("import type * as lorem from \"../lorem\";"),
            "{ts}"
        );
        assert!(ts.contains("inner!: lorem.Resume;"), "{ts}");
        let dts = render_leaf_body_dts(&consumer);
        assert!(
            dts.contains("import type * as lorem from \"../lorem\";"),
            "{dts}"
        );
    }

    #[test]
    fn cross_leaf_multi_segment_uses_top_namespace() {
        // Regression: emitting `symbol_collisions.fizz.buzz.foo.Bar`
        // requires importing the *top-level* segment `symbol_collisions`
        // (not the underscore-flattened
        // `symbol_collisions_fizz_buzz_foo`). The full dotted form must
        // survive into the rendered source unchanged.
        let bar_deep = cg_name("user", &["symbol_collisions", "fizz", "buzz", "foo"], "Bar");
        let bar_mid = cg_name("user", &["symbol_collisions", "fizz", "foo"], "Bar");
        let bar_shallow = cg_name("user", &["symbol_collisions", "foo"], "Bar");
        let consumer = body_for(
            &["symbol_collisions", "lorem"],
            EmittedSymbol::Class(NodeClass {
                name: "Ipsum".to_string(),
                source: cg_name("user", &["symbol_collisions", "lorem"], "Ipsum"),
                generic_params: vec![],
                docstring: None,
                properties: vec![
                    NodeClassProperty {
                        name: "bar1".to_string(),
                        ty: Ty::Class(bar_shallow, vec![]),
                        docstring: None,
                    },
                    NodeClassProperty {
                        name: "bar2".to_string(),
                        ty: Ty::Class(bar_mid, vec![]),
                        docstring: None,
                    },
                    NodeClassProperty {
                        name: "bar3".to_string(),
                        ty: Ty::Class(bar_deep, vec![]),
                        docstring: None,
                    },
                ],
                static_methods: vec![],
                instance_methods: vec![],
            }),
        );
        let ts = render_leaf_body(&consumer);
        // From `symbol_collisions/lorem/index.ts` the import goes one
        // directory up to find `symbol_collisions/index.ts`.
        assert!(
            ts.contains("import type * as symbol_collisions from \"..\";"),
            "{ts}"
        );
        // Make sure the old per-target flattened alias is gone — if it
        // sneaks back in we'd be importing the wrong path.
        assert!(
            !ts.contains("symbol_collisions_fizz_buzz_foo"),
            "underscore-flattened alias leaked: {ts}"
        );
        assert!(
            !ts.contains("symbol_collisions_fizz_foo"),
            "underscore-flattened alias leaked: {ts}"
        );
        // And the use sites use the full dotted form.
        assert!(ts.contains("bar1!: symbol_collisions.foo.Bar;"), "{ts}");
        assert!(
            ts.contains("bar2!: symbol_collisions.fizz.foo.Bar;"),
            "{ts}"
        );
        assert!(
            ts.contains("bar3!: symbol_collisions.fizz.buzz.foo.Bar;"),
            "{ts}"
        );

        // Single import line covers all three references — dedup by top
        // segment.
        let import_lines: Vec<&str> = ts
            .lines()
            .filter(|l| l.contains("symbol_collisions") && l.starts_with("import"))
            .collect();
        assert_eq!(import_lines.len(), 1, "expected 1 import line: {ts}");

        let dts = render_leaf_body_dts(&consumer);
        assert!(
            dts.contains("import type * as symbol_collisions from \"..\";"),
            "{dts}"
        );
    }
}
