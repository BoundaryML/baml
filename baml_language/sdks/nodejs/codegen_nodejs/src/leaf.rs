//! Per-leaf body grouping and TypeScript rendering.
//!
//! `group_and_sort` buckets the emitted symbols by leaf and orders them
//! within each leaf. `render_index_ts` / `render_index_dts` emit the full
//! `index.ts` / `index.d.ts` for a directory: runtime/cross-leaf imports,
//! child-namespace re-exports, and real TS bodies for every top — classes,
//! enums, type aliases, and `defineFunction(...)` / `defineInstanceFunction(...)`
//! bindings. The five runtime-owned stdlib types re-export from
//! `@boundaryml/baml-core` instead of getting a generated body.
//!
//! Output shapes follow `00a-example-ts-codegen-type-shapes.md`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

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
    translate_ty::{TranslateCtx, TranslatedType, translate_ty},
};

const RUNTIME_PKG: &str = "@boundaryml/baml-core";

/// All symbols that land in one leaf's body, in final render order.
pub(crate) struct LeafBody {
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
        // Stable hoist: recursive aliases to the very front of the leaf.
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

/// If `c` is one of the five runtime-owned stdlib types, return the
/// runtime export name (`BamlImage`, etc.).
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

fn mode_str(mode: SyncAsync) -> &'static str {
    match mode {
        SyncAsync::Sync => "sync",
        SyncAsync::Async => "async",
    }
}

/// ECMAScript reserved words that cannot be a `const`/binding identifier.
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

/// State accumulated while rendering a leaf's symbol bodies, used to build
/// the file's import preamble.
#[derive(Default)]
struct RenderState {
    /// Cross-leaf references, as routed `LeafPath`s (root-relative).
    imports: BTreeSet<LeafPath>,
    uses_define_function: bool,
    uses_define_instance: bool,
    /// Set when any rendered type expression references the runtime opaque
    /// handle token `_BamlHandle` (`Ty::RustType`).
    uses_baml_handle: bool,
}

impl RenderState {
    fn merge(&mut self, t: &TranslatedType) {
        for p in &t.imports {
            self.imports.insert(p.clone());
        }
    }
}

/// Emit a `JSDoc` block for a top-level docstring, if present.
fn write_doc(out: &mut String, doc: Option<&str>) {
    if let Some(d) = doc {
        if d.trim().is_empty() {
            return;
        }
        out.push_str("/**\n");
        for line in d.lines() {
            let _ = writeln!(out, " * {line}");
        }
        out.push_str(" */\n");
    }
}

/// `<T, U>` generic-parameter list, or empty.
fn generic_decl(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

/// A function-type parameter name is cosmetic (it never affects call sites),
/// but it must be a legal identifier. Append `_` to reserved words so
/// `(default: V)` becomes `(default_: V)`. The real BAML name still travels in
/// the `defineFunction` `paramNames` array for marshalling.
fn safe_param_name(name: &str) -> String {
    if is_js_reserved(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// Build the surface function-type `<G>(a: A, b: B) => R` (or `Promise<R>`
/// for async), given the function's own generic params, parallel
/// `names`/`tys`, and a return type. `generics` are the callable's OWN type
/// vars; a class type var is already in scope on the enclosing class.
fn fn_type_sig(
    generics: &[String],
    names: &[&str],
    tys: &[TranslatedType],
    ret_expr: &str,
    is_async: bool,
) -> String {
    let params: Vec<String> = names
        .iter()
        .zip(tys.iter())
        .map(|(n, t)| format!("{}: {}", safe_param_name(n), t.expr))
        .collect();
    let ret = if is_async {
        format!("Promise<{ret_expr}>")
    } else {
        ret_expr.to_string()
    };
    format!("{}({}) => {ret}", generic_decl(generics), params.join(", "))
}

// ── Public entry points ──

/// Render the full `index.ts` for a directory.
pub(crate) fn render_index_ts(body: &LeafBody, kids: &BTreeSet<String>, is_root: bool) -> String {
    let ctx = TranslateCtx {
        current_leaf: body.leaf.clone(),
    };
    let mut state = RenderState::default();

    // Render symbol bodies first so the import preamble can be computed.
    let mut body_str = String::new();
    let mut prev: Option<&SortKey> = None;
    for (sym, key) in &body.symbols {
        if prev.is_some() {
            body_str.push('\n');
        }
        render_symbol_ts(&mut body_str, sym, &ctx, &mut state);
        prev = Some(key);
    }

    state.uses_baml_handle = body_str.contains("_BamlHandle");
    let mut out = String::new();
    write_preamble_ts(&mut out, &state, body, kids, is_root);
    if !body_str.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&body_str);
    }
    out
}

/// Render the full `index.d.ts` for a directory.
pub(crate) fn render_index_dts(body: &LeafBody, kids: &BTreeSet<String>, is_root: bool) -> String {
    let ctx = TranslateCtx {
        current_leaf: body.leaf.clone(),
    };
    let mut state = RenderState::default();

    let mut body_str = String::new();
    let mut prev: Option<&SortKey> = None;
    for (sym, key) in &body.symbols {
        if prev.is_some() {
            body_str.push('\n');
        }
        render_symbol_dts(&mut body_str, sym, &ctx, &mut state);
        prev = Some(key);
    }

    state.uses_baml_handle = body_str.contains("_BamlHandle");
    let mut out = String::new();
    write_preamble_dts(&mut out, &state, body, kids, is_root);
    if !body_str.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&body_str);
    }
    out
}

// ── Preambles ──

/// Emit type-only `import type * as <seg0> from "<rel>"` lines for each
/// distinct top-level namespace referenced cross-leaf. Reserved-word
/// segments never reach here in practice (they hold functions, not the
/// classes/enums/aliases that get cross-referenced).
fn cross_leaf_imports(state: &RenderState, leaf: &LeafPath) -> String {
    use crate::translate_ty::ROOT_ALIAS;
    let depth = leaf.segments.len();
    let mut seg0s: BTreeSet<&str> = BTreeSet::new();
    let mut needs_root = false;
    for routed in &state.imports {
        match routed.segments.first() {
            Some(first) => {
                seg0s.insert(first.as_str());
            }
            // Empty routed path = the package root (a root-namespace symbol
            // referenced from a non-root leaf).
            None => needs_root = true,
        }
    }
    let mut out = String::new();
    if needs_root {
        // Path back to the package root: `..`, `../..`, … by depth.
        let rel = vec![".."; depth].join("/");
        let _ = writeln!(out, "import type * as {ROOT_ALIAS} from \"{rel}\";");
    }
    for seg0 in seg0s {
        // Relative path from this leaf's directory up to the root, then
        // into the top-level namespace `seg0`.
        let rel = if depth == 0 {
            format!("./{seg0}")
        } else {
            format!("{}{seg0}", "../".repeat(depth))
        };
        let _ = writeln!(out, "import type * as {seg0} from \"{rel}\";");
    }
    out
}

/// Child-namespace re-exports. `export * as <kid>` works for nearly every
/// segment (including `void`), but a reserved word like `default` is not a
/// legal `export * as` alias — bind a mangled local and re-export under the
/// reserved name (legal as an export name).
fn write_child_reexports(out: &mut String, kids: &BTreeSet<String>) {
    for kid in kids {
        if is_js_reserved(kid) {
            let local = format!("__ns_{kid}");
            let _ = writeln!(out, "import * as {local} from \"./{kid}\";");
            let _ = writeln!(out, "export {{ {local} as {kid} }};");
        } else {
            let _ = writeln!(out, "export * as {kid} from \"./{kid}\";");
        }
    }
}

fn runtime_import_line(state: &RenderState, extra: &[&str]) -> String {
    let mut names: Vec<&str> = Vec::new();
    names.extend_from_slice(extra);
    if state.uses_define_function {
        names.push("defineFunction");
    }
    if state.uses_define_instance {
        names.push("defineInstanceFunction");
    }
    if names.is_empty() {
        return String::new();
    }
    names.sort_unstable();
    format!(
        "import {{ {} }} from \"{RUNTIME_PKG}\";\n",
        names.join(", ")
    )
}

fn write_preamble_ts(
    out: &mut String,
    state: &RenderState,
    body: &LeafBody,
    kids: &BTreeSet<String>,
    is_root: bool,
) {
    if state.uses_baml_handle {
        let _ = writeln!(
            out,
            "import type {{ BamlHandle as _BamlHandle }} from \"{RUNTIME_PKG}\";"
        );
    }
    if is_root {
        out.push_str(&runtime_import_line(
            state,
            &["initializeRuntime", "setTypeMap"],
        ));
        out.push_str("import * as _inlinedbaml from \"./_inlinedbaml\";\n");
        out.push_str("import { _TYPE_MAP } from \"./_typemap\";\n");
        out.push_str(&cross_leaf_imports(state, &body.leaf));
        out.push('\n');
        out.push_str("initializeRuntime(\"baml_src\", _inlinedbaml.FILES);\n");
        out.push_str("setTypeMap(_TYPE_MAP);\n");
        if !kids.is_empty() {
            out.push('\n');
            write_child_reexports(out, kids);
        }
    } else {
        out.push_str(&runtime_import_line(state, &[]));
        out.push_str(&cross_leaf_imports(state, &body.leaf));
        write_child_reexports(out, kids);
    }
}

fn write_preamble_dts(
    out: &mut String,
    state: &RenderState,
    body: &LeafBody,
    kids: &BTreeSet<String>,
    _is_root: bool,
) {
    // `.d.ts` never imports runtime helpers; only type-only cross-leaf
    // imports and child re-exports. The root and non-root shapes coincide.
    if state.uses_baml_handle {
        let _ = writeln!(
            out,
            "import type {{ BamlHandle as _BamlHandle }} from \"{RUNTIME_PKG}\";"
        );
    }
    out.push_str(&cross_leaf_imports(state, &body.leaf));
    write_child_reexports(out, kids);
}

// ── Per-symbol rendering (.ts) ──

fn render_symbol_ts(
    out: &mut String,
    sym: &EmittedSymbol,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    match sym {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = media_reexport_node_name(c) {
                render_media_reexport_ts(out, &c.name, rust_name);
            } else {
                render_class_ts(out, c, ctx, state);
            }
        }
        EmittedSymbol::Enum(e) => render_enum(out, e, /* declare */ false),
        EmittedSymbol::TypeAlias(a) => render_type_alias(out, a, ctx, state),
        EmittedSymbol::Function(f) => render_function_ts(out, f, ctx, state),
    }
}

fn render_media_reexport_ts(out: &mut String, local: &str, rust_name: &str) {
    // Import-then-export (rather than a bare `export { … } from`) so the
    // aliased name is also a usable LOCAL binding: other symbols in the same
    // leaf (e.g. `baml.llm` functions returning `Stream<…>`) reference it. A
    // bare re-export would only create an export, not a local binding. The
    // class binding is both a value (constructors, `instanceof`) and a type,
    // so no separate `export type` is needed (that would conflict, TS2484).
    let _ = writeln!(
        out,
        "import {{ {rust_name} as {local} }} from \"{RUNTIME_PKG}\";"
    );
    let _ = writeln!(out, "export {{ {local} }};");
}

fn render_enum(out: &mut String, e: &NodeEnum, declare: bool) {
    write_doc(out, e.docstring.as_deref());
    let kw = if declare {
        "export declare enum"
    } else {
        "export enum"
    };
    let _ = writeln!(out, "{kw} {} {{", e.name);
    for v in &e.variants {
        let _ = writeln!(out, "  {} = {},", v.ident, crate::ts_string(&v.value));
    }
    out.push_str("}\n");
}

fn render_type_alias(
    out: &mut String,
    a: &NodeTypeAlias,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    let rhs = translate_ty(&a.resolves_to, ctx);
    state.merge(&rhs);
    // TS resolves recursive aliases natively; same shape for both.
    let _ = writeln!(out, "export type {} = {};", a.name, rhs.expr);
}

fn render_class_ts(out: &mut String, c: &NodeClass, ctx: &TranslateCtx, state: &mut RenderState) {
    write_doc(out, c.docstring.as_deref());
    let generics = generic_decl(&c.generic_params);

    // Translate each property type once; reuse for field + constructor.
    let props: Vec<(&str, TranslatedType)> = c
        .properties
        .iter()
        .map(|p| {
            let t = translate_ty(&p.ty, ctx);
            state.merge(&t);
            (p.name.as_str(), t)
        })
        .collect();

    let _ = writeln!(out, "export class {}{generics} {{", c.name);
    for (name, t) in &props {
        // `!` definite-assignment assertion: fields are populated via the
        // constructor's `Object.assign`, which tsc's flow analysis can't see.
        let _ = writeln!(out, "  {name}!: {};", t.expr);
    }

    // Constructor.
    if props.is_empty() {
        out.push_str("  constructor(init: {}) {\n    Object.assign(this, init);\n  }\n");
    } else {
        out.push_str("  constructor(init: {\n");
        for (name, t) in &props {
            let _ = writeln!(out, "    {name}: {};", t.expr);
        }
        out.push_str("  }) {\n    Object.assign(this, init);\n  }\n");
    }

    // Static + instance method bindings, as class fields.
    for m in &c.static_methods {
        render_method_binding_ts(out, m, &c.generic_params, ctx, state);
    }
    for m in &c.instance_methods {
        render_method_binding_ts(out, m, &c.generic_params, ctx, state);
    }

    out.push_str("}\n");
}

/// The generic params a method's surface function-type should declare. A
/// STATIC member cannot reference the class's type parameters (TS2302), so a
/// static method on a generic class re-declares them as its own fresh params.
/// An instance method has the class params already in scope.
fn method_sig_generics(m: &NodeMethodBinding, class_generics: &[String]) -> Vec<String> {
    match m.kind {
        MethodKind::Static => {
            let mut g = class_generics.to_vec();
            g.extend(m.generic_params.iter().cloned());
            g
        }
        MethodKind::Instance => m.generic_params.clone(),
    }
}

/// Translate a binding's surface params (skipping the synthetic `self`
/// receiver for instance methods) and return type.
fn binding_surface<'a>(
    m: &'a NodeMethodBinding,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) -> (Vec<&'a str>, Vec<TranslatedType>, TranslatedType) {
    let surface_names: Vec<&str> = match m.kind {
        MethodKind::Static => m.param_names.iter().map(String::as_str).collect(),
        // `param_names[0]` is the synthetic `self`; drop it from the surface.
        MethodKind::Instance => m.param_names.iter().skip(1).map(String::as_str).collect(),
    };
    let tys: Vec<TranslatedType> = m
        .arg_tys
        .iter()
        .map(|t| {
            let tt = translate_ty(t, ctx);
            state.merge(&tt);
            tt
        })
        .collect();
    let ret = translate_ty(&m.return_ty, ctx);
    state.merge(&ret);
    (surface_names, tys, ret)
}

fn render_method_binding_ts(
    out: &mut String,
    m: &NodeMethodBinding,
    class_generics: &[String],
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    write_doc(out, m.docstring.as_deref());
    let (names, tys, ret) = binding_surface(m, ctx, state);
    let is_async = m.mode == SyncAsync::Async;
    let sig_generics = method_sig_generics(m, class_generics);
    let sig = fn_type_sig(&sig_generics, &names, &tys, &ret.expr, is_async);
    let params_lit = param_names_literal(&m.param_names);
    match m.kind {
        MethodKind::Static => {
            state.uses_define_function = true;
            let _ = writeln!(
                out,
                "  static {} = defineFunction(\"{}\", \"{}\", {params_lit}) as {sig};",
                m.name,
                m.baml_fqn,
                mode_str(m.mode),
            );
        }
        MethodKind::Instance => {
            state.uses_define_instance = true;
            let _ = writeln!(
                out,
                "  {} = defineInstanceFunction(\"{}\", \"{}\", {params_lit}).bind(this) as {sig};",
                m.name,
                m.baml_fqn,
                mode_str(m.mode),
            );
        }
    }
}

fn render_function_ts(
    out: &mut String,
    f: &NodeFunction,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    write_doc(out, f.docstring.as_deref());
    state.uses_define_function = true;
    let tys: Vec<TranslatedType> = f
        .arg_tys
        .iter()
        .map(|t| {
            let tt = translate_ty(t, ctx);
            state.merge(&tt);
            tt
        })
        .collect();
    let ret = translate_ty(&f.return_ty, ctx);
    state.merge(&ret);
    let names: Vec<&str> = f.param_names.iter().map(String::as_str).collect();
    let is_async = f.mode == SyncAsync::Async;
    let sig = fn_type_sig(&f.generic_params, &names, &tys, &ret.expr, is_async);
    let params_lit = param_names_literal(&f.param_names);
    let factory = format!(
        "defineFunction(\"{}\", \"{}\", {params_lit}) as {sig}",
        f.baml_fqn,
        mode_str(f.mode),
    );
    if is_js_reserved(&f.name) {
        // `export const new = …` is a syntax error; bind a mangled local
        // and re-export under the reserved name.
        let local = format!("__baml_{}", f.name);
        let _ = writeln!(out, "const {local} = {factory};");
        let _ = writeln!(out, "export {{ {local} as {} }};", f.name);
    } else {
        let _ = writeln!(out, "export const {} = {factory};", f.name);
    }
}

fn param_names_literal(names: &[String]) -> String {
    let parts: Vec<String> = names.iter().map(|n| crate::ts_string(n)).collect();
    format!("[{}]", parts.join(", "))
}

// ── Per-symbol rendering (.d.ts) ──

fn render_symbol_dts(
    out: &mut String,
    sym: &EmittedSymbol,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    match sym {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = media_reexport_node_name(c) {
                render_media_reexport_dts(out, &c.name, rust_name);
            } else {
                render_class_dts(out, c, ctx, state);
            }
        }
        EmittedSymbol::Enum(e) => render_enum(out, e, /* declare */ true),
        EmittedSymbol::TypeAlias(a) => render_type_alias(out, a, ctx, state),
        EmittedSymbol::Function(f) => render_function_dts(out, f, ctx, state),
    }
}

fn render_media_reexport_dts(out: &mut String, local: &str, rust_name: &str) {
    let _ = writeln!(
        out,
        "export declare const {local}: typeof import(\"{RUNTIME_PKG}\").{rust_name};"
    );
    if rust_name == "BamlStream" {
        let _ = writeln!(
            out,
            "export type {local}<TStream, TFinal> = import(\"{RUNTIME_PKG}\").{rust_name}<TStream, TFinal>;"
        );
    } else {
        let _ = writeln!(
            out,
            "export type {local} = import(\"{RUNTIME_PKG}\").{rust_name};"
        );
    }
}

fn render_class_dts(out: &mut String, c: &NodeClass, ctx: &TranslateCtx, state: &mut RenderState) {
    write_doc(out, c.docstring.as_deref());
    let generics = generic_decl(&c.generic_params);
    let props: Vec<(&str, TranslatedType)> = c
        .properties
        .iter()
        .map(|p| {
            let t = translate_ty(&p.ty, ctx);
            state.merge(&t);
            (p.name.as_str(), t)
        })
        .collect();

    let _ = writeln!(out, "export declare class {}{generics} {{", c.name);
    for (name, t) in &props {
        let _ = writeln!(out, "  {name}: {};", t.expr);
    }
    if props.is_empty() {
        out.push_str("  constructor(init: {});\n");
    } else {
        out.push_str("  constructor(init: {\n");
        for (name, t) in &props {
            let _ = writeln!(out, "    {name}: {};", t.expr);
        }
        out.push_str("  });\n");
    }
    for m in c.static_methods.iter().chain(c.instance_methods.iter()) {
        write_doc(out, m.docstring.as_deref());
        let (names, tys, ret) = binding_surface(m, ctx, state);
        let is_async = m.mode == SyncAsync::Async;
        let sig_generics = method_sig_generics(m, &c.generic_params);
        let sig = fn_type_sig(&sig_generics, &names, &tys, &ret.expr, is_async);
        let kw = if m.kind == MethodKind::Static {
            "static "
        } else {
            ""
        };
        let _ = writeln!(out, "  {kw}{}: {sig};", m.name);
    }
    out.push_str("}\n");
}

fn render_function_dts(
    out: &mut String,
    f: &NodeFunction,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    write_doc(out, f.docstring.as_deref());
    let tys: Vec<TranslatedType> = f
        .arg_tys
        .iter()
        .map(|t| {
            let tt = translate_ty(t, ctx);
            state.merge(&tt);
            tt
        })
        .collect();
    let ret = translate_ty(&f.return_ty, ctx);
    state.merge(&ret);
    let params: Vec<String> = f
        .param_names
        .iter()
        .zip(tys.iter())
        .map(|(n, t)| format!("{}: {}", safe_param_name(n), t.expr))
        .collect();
    let ret_expr = if f.mode == SyncAsync::Async {
        format!("Promise<{}>", ret.expr)
    } else {
        ret.expr.clone()
    };
    let generics = generic_decl(&f.generic_params);
    // A reserved-word function name can't be a `function` declaration name;
    // fall back to a `const` of the function type.
    if is_js_reserved(&f.name) {
        let local = format!("__baml_{}", f.name);
        let _ = writeln!(
            out,
            "declare const {local}: {generics}({}) => {ret_expr};\nexport {{ {local} as {} }};",
            params.join(", "),
            f.name,
        );
    } else {
        let _ = writeln!(
            out,
            "export declare function {}{generics}({}): {ret_expr};",
            f.name,
            params.join(", "),
        );
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{Name, Ty};

    use super::*;

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn leaf(segs: &[&str]) -> LeafPath {
        LeafPath {
            segments: segs.iter().map(ToString::to_string).collect(),
        }
    }

    fn body(segs: &[&str], syms: Vec<EmittedSymbol>) -> LeafBody {
        LeafBody {
            leaf: leaf(segs),
            symbols: syms.into_iter().map(|s| (s, (String::new(), 0))).collect(),
        }
    }

    fn class_sym(n: &str, source: Name, props: Vec<(&str, Ty)>) -> EmittedSymbol {
        EmittedSymbol::Class(NodeClass {
            name: n.to_string(),
            source,
            generic_params: Vec::new(),
            docstring: None,
            properties: props
                .into_iter()
                .map(|(pn, ty)| crate::emit::class::NodeClassProperty {
                    name: pn.to_string(),
                    ty,
                    docstring: None,
                })
                .collect(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
    }

    fn enum_sym(n: &str, source: Name, variants: &[&str]) -> EmittedSymbol {
        EmittedSymbol::Enum(NodeEnum {
            name: n.to_string(),
            source,
            variants: variants
                .iter()
                .map(|v| crate::emit::enum_::NodeEnumVariant {
                    ident: v.to_string(),
                    value: v.to_string(),
                    docstring: None,
                })
                .collect(),
            docstring: None,
        })
    }

    fn func_sym(
        n: &str,
        fqn: &str,
        mode: SyncAsync,
        params: Vec<(&str, Ty)>,
        ret: Ty,
    ) -> EmittedSymbol {
        EmittedSymbol::Function(NodeFunction {
            name: n.to_string(),
            baml_fqn: fqn.to_string(),
            mode,
            param_names: params.iter().map(|(n, _)| n.to_string()).collect(),
            arg_defaults: params.iter().map(|_| None).collect(),
            arg_tys: params.into_iter().map(|(_, t)| t).collect(),
            return_ty: ret,
            generic_params: Vec::new(),
            docstring: None,
            raises_names: Vec::new(),
        })
    }

    #[test]
    fn class_renders_real_body() {
        let b = body(
            &["lorem"],
            vec![class_sym(
                "Resume",
                name("user", &["lorem"], "Resume"),
                vec![("name", Ty::String), ("age", Ty::Int)],
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false);
        assert!(ts.contains("export class Resume {"));
        assert!(ts.contains("name!: string;"));
        assert!(ts.contains("age!: number;"));
        assert!(ts.contains("Object.assign(this, init);"));
        let dts = render_index_dts(&b, &BTreeSet::new(), false);
        assert!(dts.contains("export declare class Resume {"));
        assert!(dts.contains("name: string;"));
        assert!(dts.contains("constructor(init: {"));
        assert!(!dts.contains("Object.assign"));
    }

    #[test]
    fn enum_renders_runtime_enum() {
        let b = body(
            &["ipsum"],
            vec![enum_sym(
                "Sentiment",
                name("user", &["ipsum"], "Sentiment"),
                &["HAPPY", "SAD"],
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false);
        assert!(ts.contains("export enum Sentiment {"));
        assert!(ts.contains("HAPPY = \"HAPPY\","));
        let dts = render_index_dts(&b, &BTreeSet::new(), false);
        assert!(dts.contains("export declare enum Sentiment {"));
    }

    #[test]
    fn function_fans_out_define_function() {
        let b = body(
            &["lorem"],
            vec![
                func_sym(
                    "extract",
                    "user.lorem.extract",
                    SyncAsync::Sync,
                    vec![("text", Ty::String)],
                    Ty::Int,
                ),
                func_sym(
                    "extract_async",
                    "user.lorem.extract",
                    SyncAsync::Async,
                    vec![("text", Ty::String)],
                    Ty::Int,
                ),
            ],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false);
        assert!(ts.contains("import { defineFunction } from \"@boundaryml/baml-core\";"));
        assert!(ts.contains("export const extract = defineFunction(\"user.lorem.extract\", \"sync\", [\"text\"]) as (text: string) => number;"));
        assert!(ts.contains("export const extract_async = defineFunction(\"user.lorem.extract\", \"async\", [\"text\"]) as (text: string) => Promise<number>;"));
        let dts = render_index_dts(&b, &BTreeSet::new(), false);
        assert!(dts.contains("export declare function extract(text: string): number;"));
        assert!(
            dts.contains("export declare function extract_async(text: string): Promise<number>;")
        );
        assert!(!dts.contains("defineFunction"));
    }

    #[test]
    fn cross_leaf_field_imports_seg0() {
        let b = body(
            &["consumer"],
            vec![class_sym(
                "Holder",
                name("user", &["consumer"], "Holder"),
                vec![("r", Ty::Class(name("user", &["lorem"], "Resume"), vec![]))],
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false);
        assert!(ts.contains("import type * as lorem from \"../lorem\";"));
        assert!(ts.contains("r!: lorem.Resume;"));
    }

    #[test]
    fn media_reexport_full_shape() {
        let b = body(
            &["baml", "media"],
            vec![class_sym(
                "Image",
                name("baml", &["media"], "Image"),
                vec![],
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false);
        assert!(ts.contains("import { BamlImage as Image } from \"@boundaryml/baml-core\";"));
        assert!(ts.contains("export { Image };"));
        // The class binding already provides the type; no separate `export type`.
        assert!(!ts.contains("export type Image"));
        let dts = render_index_dts(&b, &BTreeSet::new(), false);
        assert!(dts.contains(
            "export declare const Image: typeof import(\"@boundaryml/baml-core\").BamlImage;"
        ));
    }

    #[test]
    fn container_reexports_children() {
        let b = body(&["vendor"], vec![]);
        let mut kids = BTreeSet::new();
        kids.insert("aws".to_string());
        let ts = render_index_ts(&b, &kids, false);
        assert!(ts.contains("export * as aws from \"./aws\";"));
        assert!(!ts.contains("export const"));
    }

    #[test]
    fn root_wires_runtime_and_reexports() {
        let b = body(
            &[],
            vec![func_sym(
                "make_foo",
                "user.make_foo",
                SyncAsync::Sync,
                vec![],
                Ty::Int,
            )],
        );
        let mut kids = BTreeSet::new();
        kids.insert("lorem".to_string());
        let ts = render_index_ts(&b, &kids, true);
        assert!(ts.contains("initializeRuntime(\"baml_src\", _inlinedbaml.FILES);"));
        assert!(ts.contains("setTypeMap(_TYPE_MAP);"));
        assert!(ts.contains("export * as lorem from \"./lorem\";"));
        assert!(ts.contains("export const make_foo = defineFunction("));
        assert!(ts.contains("import { defineFunction, initializeRuntime, setTypeMap } from \"@boundaryml/baml-core\";"));
    }
}
