//! Shared per-leaf body grouping and TypeScript rendering.
//!
//! `group_and_sort` buckets the emitted symbols by leaf and orders them
//! within each leaf. `render_index_ts` emits the full `index.ts` for a
//! directory: runtime/cross-leaf imports, child-namespace re-exports, and
//! real TS bodies for every top — classes, enums, type aliases, and
//! `defineFunction(...)` / `defineInstanceFunction(...)` bindings. The five
//! runtime-owned stdlib types re-export from the configured runtime package instead
//! of getting a generated body.
//!
//! Codegen emits only `index.ts` — no sibling `index.d.ts`. The generated
//! `.ts` is fully typed (real `export class`, typed `as` casts on every
//! `defineFunction` binding), so a separate declaration file is redundant;
//! `tsc` and editors read types straight from the `.ts`.
//!
//! Output shapes follow `00a-example-ts-codegen-type-shapes.md`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use baml_base::qualified_name::{AI_FUNCTION_SPEC, AI_STREAM_STREAM};
use baml_codegen_types::FunctionArgumentDefault;

use crate::{
    emit::{
        EmittedSymbol, SortKey,
        class::TypeScriptClass,
        enum_::TypeScriptEnum,
        function::{SyncAsync, TypeScriptFunction},
        method::{MethodKind, TypeScriptMethodBinding},
        type_alias::TypeScriptTypeAlias,
    },
    routing::LeafPath,
    translate_ty::{TranslateCtx, TranslatedType, translate_ty},
};

/// All symbols that land in one leaf's body, in final render order.
pub(crate) struct LeafBody {
    pub(crate) leaf: LeafPath,
    pub(crate) symbols: Vec<(EmittedSymbol, SortKey)>,
}

impl LeafBody {
    fn callable_child_aliases(&self, kids: &BTreeSet<String>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (sym, _) in &self.symbols {
            let EmittedSymbol::Function(f) = sym else {
                continue;
            };
            if f.mode == SyncAsync::Sync && kids.contains(&f.name) {
                out.insert(f.name.clone(), child_namespace_alias(&f.name));
            }
        }
        out
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

/// Authoritative engine-FQN → runtime-export mapping for classes whose
/// JavaScript identity is owned by the bridge package rather than codegen.
const RUNTIME_OWNED_CLASS_REEXPORTS: &[(&str, &str)] = &[
    ("baml.media.Image", "BamlImage"),
    ("baml.media.Audio", "BamlAudio"),
    ("baml.media.Video", "BamlVideo"),
    ("baml.media.Pdf", "BamlPdf"),
    (AI_STREAM_STREAM, "BamlStream"),
    (AI_FUNCTION_SPEC, "BamlFunctionSpec"),
    ("ai.Prompt", "BamlPrompt"),
    ("reflect.Type", "reflectType"),
];

fn runtime_owned_reexport_name(c: &TypeScriptClass) -> Option<&'static str> {
    let source = c.source.to_string();
    RUNTIME_OWNED_CLASS_REEXPORTS
        .iter()
        .find_map(|(fqn, runtime_name)| (*fqn == source).then_some(*runtime_name))
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
    // Restricted binding identifiers in strict mode. TypeScript modules are
    // always strict, so these are illegal as parameters and top-level consts
    // even though they are not ordinary ECMAScript keywords.
    "arguments",
    "eval",
];

fn is_js_reserved(name: &str) -> bool {
    JS_RESERVED.contains(&name)
}

/// State accumulated while rendering a leaf's symbol bodies, used to build
/// the file's import preamble.
// Each flag tracks a distinct runtime import the leaf may need; they're
// independent presence bits, not a state enum, so the bool-count lint
// (`struct_excessive_bools`) doesn't apply cleanly here.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
struct RenderState {
    /// Cross-leaf references, as routed `LeafPath`s (root-relative).
    imports: BTreeSet<LeafPath>,
    uses_define_function: bool,
    uses_define_instance: bool,
    /// Set when any rendered type expression references the runtime opaque
    /// handle token `_BamlHandle` (`Ty::RustType`).
    uses_baml_handle: bool,
    /// Set when a generic class/call emits `$types`, which accepts both opaque
    /// reflected definitions and statically-known host tokens.
    uses_baml_type: bool,
    /// Set when a BAML `type` appears in a public signature.
    uses_baml_type_handle: bool,
}

impl RenderState {
    fn merge(&mut self, t: &TranslatedType) {
        for p in &t.imports {
            self.imports.insert(p.clone());
        }
        if t.expr.contains("BamlType") {
            self.uses_baml_type_handle = true;
        }
    }
}

fn write_doc_with_raises(out: &mut String, doc: Option<&str>, raises_names: &[String]) {
    let has_doc = doc.is_some_and(|d| !d.trim().is_empty());
    if !has_doc && raises_names.is_empty() {
        return;
    }

    out.push_str("/**\n");
    if let Some(d) = doc {
        for line in d.lines() {
            let _ = writeln!(out, " * {line}");
        }
    }
    for name in raises_names {
        let _ = writeln!(out, " * @throws {name}");
    }
    out.push_str(" */\n");
}

fn write_class_doc(out: &mut String, c: &TypeScriptClass) {
    let documented_fields = c.properties.iter().any(|p| p.docstring.is_some());
    let has_doc = c.docstring.as_deref().is_some_and(|d| !d.trim().is_empty());
    if !has_doc && !documented_fields {
        return;
    }

    out.push_str("/**\n");
    if let Some(doc) = c.docstring.as_deref() {
        for line in doc.lines() {
            let _ = writeln!(out, " * {line}");
        }
    }
    if documented_fields {
        if has_doc {
            out.push_str(" *\n");
        }
        out.push_str(" * Attributes:\n");
        for prop in &c.properties {
            match prop.docstring.as_deref() {
                Some(doc) if !doc.trim().is_empty() => {
                    let mut lines = doc.lines();
                    if let Some(first) = lines.next() {
                        let _ = writeln!(out, " *   {}: {}", prop.name, first);
                    }
                    for line in lines {
                        let _ = writeln!(out, " *     {line}");
                    }
                }
                _ => {
                    let _ = writeln!(out, " *   {}", prop.name);
                }
            }
        }
    }
    out.push_str(" */\n");
}

fn write_enum_doc(out: &mut String, e: &TypeScriptEnum) {
    let documented_variants = e.variants.iter().any(|v| v.docstring.is_some());
    let has_doc = e.docstring.as_deref().is_some_and(|d| !d.trim().is_empty());
    if !has_doc && !documented_variants {
        return;
    }

    out.push_str("/**\n");
    if let Some(doc) = e.docstring.as_deref() {
        for line in doc.lines() {
            let _ = writeln!(out, " * {line}");
        }
    }
    if documented_variants {
        if has_doc {
            out.push_str(" *\n");
        }
        out.push_str(" * Members:\n");
        for variant in &e.variants {
            match variant.docstring.as_deref() {
                Some(doc) if !doc.trim().is_empty() => {
                    let mut lines = doc.lines();
                    if let Some(first) = lines.next() {
                        let _ = writeln!(out, " *   {}: {}", variant.ident, first);
                    }
                    for line in lines {
                        let _ = writeln!(out, " *     {line}");
                    }
                }
                _ => {
                    let _ = writeln!(out, " *   {}", variant.ident);
                }
            }
        }
    }
    out.push_str(" */\n");
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
pub(crate) fn safe_param_name(name: &str) -> String {
    if is_js_reserved(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

pub(crate) fn safe_required_param_names(
    names: &[&str],
    reserve_options_param: bool,
) -> Vec<String> {
    let mut used = BTreeSet::new();
    if reserve_options_param {
        used.insert("$opts".to_string());
    }
    names
        .iter()
        .map(|name| {
            let mut candidate = safe_param_name(name);
            while !used.insert(candidate.clone()) {
                candidate.push('_');
            }
            candidate
        })
        .collect()
}

pub(crate) fn option_field_name(name: &str) -> String {
    if is_ts_property_identifier(name) {
        name.to_string()
    } else {
        crate::ts_string(name)
    }
}

fn is_ts_property_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c == '$' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c == '$' || c.is_ascii_alphanumeric())
}

/// Build the surface function-type `<G>(a: A, b: B) => R` (or `Promise<R>`
/// for async), given the function's own generic params, parallel
/// `names`/`tys`, and a return type. `generics` are the callable's OWN type
/// vars; a class type var is already in scope on the enclosing class.
fn fn_type_sig(
    generics: &[String],
    type_binding_params: &[String],
    names: &[&str],
    tys: &[TranslatedType],
    defaults: &[Option<FunctionArgumentDefault>],
    ret_expr: &str,
    is_async: bool,
) -> String {
    let required = required_positional_count(defaults);
    let required_names = safe_required_param_names(&names[..required], required < names.len());
    let mut params: Vec<String> = required_names
        .iter()
        .zip(tys.iter())
        .map(|(name, ty)| format!("{name}: {}", ty.expr))
        .collect();
    {
        let mut fields = Vec::new();
        for (name, ty) in names.iter().zip(tys.iter()).skip(required) {
            fields.push(format!(
                "{}?: {} | undefined",
                option_field_name(name),
                ty.expr
            ));
        }
        fields.push("$ctx?: BamlCallContext | undefined".to_string());
        if !type_binding_params.is_empty() {
            let bindings = type_binding_params
                .iter()
                .map(|name| format!("{name}?: BamlType | BamlTypeToken"))
                .collect::<Vec<_>>()
                .join("; ");
            fields.push(format!("$types?: {{ {bindings} }} | undefined"));
        }
        params.push(format!("$opts?: {{ {} }} | undefined", fields.join("; ")));
    }
    let ret = if is_async {
        format!("Promise<{ret_expr}>")
    } else {
        ret_expr.to_string()
    };
    format!("{}({}) => {ret}", generic_decl(generics), params.join(", "))
}

// ── Public entry point ──

/// Render the full `index.ts` for a directory.
pub(crate) fn render_index_ts(
    body: &LeafBody,
    kids: &BTreeSet<String>,
    is_root: bool,
    runtime_package: &str,
) -> String {
    let ctx = TranslateCtx {
        current_leaf: body.leaf.clone(),
    };
    let mut state = RenderState::default();
    let callable_child_aliases = body.callable_child_aliases(kids);

    // Render symbol bodies first so the import preamble can be computed.
    let mut body_str = String::new();
    let mut prev: Option<&SortKey> = None;
    for (sym, key) in &body.symbols {
        if prev.is_some() {
            body_str.push('\n');
        }
        render_symbol_ts(
            &mut body_str,
            sym,
            &ctx,
            &mut state,
            &callable_child_aliases,
            runtime_package,
        );
        prev = Some(key);
    }

    state.uses_baml_handle = body_str.contains("_BamlHandle");
    let mut out = String::new();
    write_preamble_ts(
        &mut out,
        &state,
        body,
        kids,
        &callable_child_aliases,
        is_root,
        runtime_package,
    );
    if !body_str.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&body_str);
    }
    out
}

// ── Preamble ──

/// Emit type-only `import type * as <seg0> from "<rel>"` lines for each
/// distinct top-level namespace referenced cross-leaf. Reserved-word
/// segments never reach here in practice (they hold functions, not the
/// classes/enums/aliases that get cross-referenced).
fn cross_leaf_imports(state: &RenderState, leaf: &LeafPath) -> String {
    use crate::translate_ty::ROOT_ALIAS;
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
        let rel = leaf_module_specifier(leaf, &LeafPath { segments: vec![] });
        let _ = writeln!(out, "import type * as {ROOT_ALIAS} from \"{rel}\";");
    }
    for seg0 in seg0s {
        let rel = leaf_module_specifier(
            leaf,
            &LeafPath {
                segments: vec![seg0.to_string()],
            },
        );
        let _ = writeln!(out, "import type * as {seg0} from \"{rel}\";");
    }
    out
}

fn leaf_module_specifier(from: &LeafPath, to: &LeafPath) -> String {
    let up = "../".repeat(from.segments.len());
    if to.segments.is_empty() {
        if up.is_empty() {
            "./index.js".to_string()
        } else {
            format!("{up}index.js")
        }
    } else {
        let down = to.segments.join("/");
        if up.is_empty() {
            format!("./{down}/index.js")
        } else {
            format!("{up}{down}/index.js")
        }
    }
}

/// Child-namespace re-exports. `export * as <kid>` works for nearly every
/// segment (including `void`), but a reserved word like `default` is not a
/// legal `export * as` alias — bind a mangled local and re-export under the
/// reserved name (legal as an export name).
fn child_namespace_alias(kid: &str) -> String {
    format!("__ns_{kid}")
}

fn write_child_reexports(
    out: &mut String,
    kids: &BTreeSet<String>,
    callable_child_aliases: &BTreeMap<String, String>,
) {
    for kid in kids {
        let child_path = format!("./{kid}/index.js");
        if let Some(local) = callable_child_aliases.get(kid) {
            let _ = writeln!(out, "import * as {local} from \"{child_path}\";");
        } else if is_js_reserved(kid) {
            let local = format!("__ns_{kid}");
            let _ = writeln!(out, "import * as {local} from \"{child_path}\";");
            let _ = writeln!(out, "export {{ {local} as {kid} }};");
        } else {
            let _ = writeln!(out, "export * as {kid} from \"{child_path}\";");
        }
    }
}

fn runtime_import_line(state: &RenderState, extra: &[&str], runtime_package: &str) -> String {
    let mut names: Vec<&str> = Vec::new();
    names.extend_from_slice(extra);
    if state.uses_define_function {
        names.push("defineFunction");
    }
    if state.uses_define_instance {
        names.push("defineInstanceFunction");
    }
    if state.uses_define_function || state.uses_define_instance {
        names.push("type BamlCallContext");
    }
    // Type-only import (inline `type` modifier) for the generic `$types` field
    // token. Sorted alongside the value imports; TS accepts a mixed
    // value/type-only named import.
    if state.uses_baml_type {
        names.push("type BamlType");
        names.push("type BamlTypeToken");
    } else if state.uses_baml_type_handle {
        names.push("type BamlType");
    }
    if names.is_empty() {
        return String::new();
    }
    names.sort_unstable();
    format!(
        "import {{ {} }} from \"{runtime_package}\";\n",
        names.join(", ")
    )
}

fn write_preamble_ts(
    out: &mut String,
    state: &RenderState,
    body: &LeafBody,
    kids: &BTreeSet<String>,
    callable_child_aliases: &BTreeMap<String, String>,
    is_root: bool,
    runtime_package: &str,
) {
    if state.uses_baml_handle {
        let _ = writeln!(
            out,
            "import type {{ BamlHandle as _BamlHandle }} from \"{runtime_package}\";"
        );
    }
    if is_root {
        out.push_str(&runtime_import_line(
            state,
            &["initializeRuntimeFromBytecode", "setTypeMap"],
            runtime_package,
        ));
        out.push_str("import * as _inlinedbaml from \"./_inlinedbaml.js\";\n");
        out.push_str("import { _TYPE_MAP } from \"./_typemap.js\";\n");
        out.push_str(&cross_leaf_imports(state, &body.leaf));
        out.push('\n');
        out.push_str(
            "initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE, _inlinedbaml.BAML_TOML);\n",
        );
        out.push_str("setTypeMap(_TYPE_MAP);\n");
        if !kids.is_empty() {
            out.push('\n');
            write_child_reexports(out, kids, callable_child_aliases);
        }
    } else {
        out.push_str(&runtime_import_line(state, &[], runtime_package));
        out.push_str(&cross_leaf_imports(state, &body.leaf));
        write_child_reexports(out, kids, callable_child_aliases);
    }
}

// ── Per-symbol rendering ──

fn render_symbol_ts(
    out: &mut String,
    sym: &EmittedSymbol,
    ctx: &TranslateCtx,
    state: &mut RenderState,
    callable_child_aliases: &BTreeMap<String, String>,
    runtime_package: &str,
) {
    match sym {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = runtime_owned_reexport_name(c) {
                render_media_reexport_ts(out, &c.name, rust_name, runtime_package);
            } else {
                render_class_ts(out, c, ctx, state);
            }
        }
        EmittedSymbol::Enum(e) => render_enum(out, e),
        EmittedSymbol::TypeAlias(a) => render_type_alias(out, a, ctx, state),
        EmittedSymbol::Function(f) => render_function_ts(
            out,
            f,
            ctx,
            state,
            callable_child_aliases.get(&f.name).map(String::as_str),
        ),
    }
}

fn render_media_reexport_ts(out: &mut String, local: &str, rust_name: &str, runtime_package: &str) {
    // Import-then-export (rather than a bare `export { … } from`) so the
    // aliased name is also a usable LOCAL binding: other symbols in the same
    // leaf (e.g. `baml.llm` functions returning `Stream<…>`) reference it. A
    // bare re-export would only create an export, not a local binding. The
    // class binding is both a value (constructors, `instanceof`) and a type,
    // so no separate `export type` is needed (that would conflict, TS2484).
    let _ = writeln!(
        out,
        "import {{ {rust_name} as {local} }} from \"{runtime_package}\";"
    );
    let _ = writeln!(out, "export {{ {local} }};");
}

fn render_enum(out: &mut String, e: &TypeScriptEnum) {
    write_enum_doc(out, e);
    let _ = writeln!(out, "export enum {} {{", e.name);
    for v in &e.variants {
        let _ = writeln!(out, "  {} = {},", v.ident, crate::ts_string(&v.value));
    }
    out.push_str("}\n");
}

fn render_type_alias(
    out: &mut String,
    a: &TypeScriptTypeAlias,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    let rhs = translate_ty(&a.resolves_to, ctx);
    state.merge(&rhs);
    // TS resolves recursive aliases natively; same shape for both.
    let _ = writeln!(out, "export type {} = {};", a.name, rhs.expr);
}

fn render_class_ts(
    out: &mut String,
    c: &TypeScriptClass,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    write_class_doc(out, c);
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

    // A generic class carries its concrete TypeVar bindings in an optional
    // `$types` field — the value-level type channel the inbound encoder reads to
    // build `class_ty` (TS erases generics, so the metadata Python recovers from
    // Pydantic must be spelled explicitly here). It is optional: an absent
    // binding lowers to the unknown/top type at encode time.
    let is_generic = !c.generic_params.is_empty();
    let types_field = is_generic.then(|| {
        let fields = c
            .generic_params
            .iter()
            .map(|p| format!("{p}?: BamlType | BamlTypeToken"))
            .collect::<Vec<_>>()
            .join("; ");
        format!("{{ {fields} }}")
    });

    let _ = writeln!(out, "export class {}{generics} {{", c.name);
    for (name, t) in &props {
        // `!` definite-assignment assertion: fields are populated via the
        // constructor's `Object.assign`, which tsc's flow analysis can't see.
        let _ = writeln!(out, "  {name}!: {};", t.expr);
    }
    if let Some(types_ty) = &types_field {
        state.uses_baml_type = true;
        let _ = writeln!(out, "  $types?: {types_ty};");
    }

    // Constructor.
    if props.is_empty() && types_field.is_none() {
        out.push_str("  constructor(init: {}) {\n    Object.assign(this, init);\n  }\n");
    } else {
        out.push_str("  constructor(init: {\n");
        for (name, t) in &props {
            let _ = writeln!(out, "    {name}: {};", t.expr);
        }
        if let Some(types_ty) = &types_field {
            let _ = writeln!(out, "    $types?: {types_ty};");
        }
        out.push_str("  }) {\n    Object.assign(this, init);\n  }\n");
    }

    // Static `$generic`: the TypeVar names in declaration order, read back by
    // the inbound encoder to position the `$types` bindings as `class_ty` args.
    if is_generic {
        let params = c
            .generic_params
            .iter()
            .map(|p| crate::ts_string(p))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "  static readonly $generic = [{params}] as const;");
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
fn method_sig_generics(m: &TypeScriptMethodBinding, class_generics: &[String]) -> Vec<String> {
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
    m: &'a TypeScriptMethodBinding,
    ctx: &TranslateCtx,
    state: &mut RenderState,
) -> (
    Vec<&'a str>,
    Vec<TranslatedType>,
    Vec<Option<FunctionArgumentDefault>>,
    TranslatedType,
) {
    let surface_names: Vec<&str> = m
        .required_args
        .iter()
        .map(|arg| arg.name.as_str())
        .chain(m.optional_args.iter().map(|arg| arg.name.as_str()))
        .collect();

    let mut tys: Vec<TranslatedType> = m
        .required_args
        .iter()
        .map(|arg| {
            let tt = translate_ty(&arg.ty, ctx);
            state.merge(&tt);
            tt
        })
        .collect();
    tys.extend(m.optional_args.iter().map(|arg| {
        let tt = translate_ty(&arg.ty, ctx);
        state.merge(&tt);
        tt
    }));

    let ret = translate_ty(&m.return_ty, ctx);
    state.merge(&ret);

    let defaults = vec![None; m.required_args.len()]
        .into_iter()
        .chain(m.optional_args.iter().map(|arg| Some(arg.default.clone())))
        .collect();

    (surface_names, tys, defaults, ret)
}

fn render_method_binding_ts(
    out: &mut String,
    m: &TypeScriptMethodBinding,
    class_generics: &[String],
    ctx: &TranslateCtx,
    state: &mut RenderState,
) {
    write_doc_with_raises(out, m.docstring.as_deref(), &m.raises_names);
    let (names, tys, defaults, ret) = binding_surface(m, ctx, state);
    let is_async = m.mode == SyncAsync::Async;
    let sig_generics = method_sig_generics(m, class_generics);
    if !m.generic_params.is_empty() {
        state.uses_baml_type = true;
    }
    let sig = fn_type_sig(
        &sig_generics,
        &m.generic_params,
        &names,
        &tys,
        &defaults,
        &ret.expr,
        is_async,
    );
    let required_params = m.runtime_required_names();
    let optional_params = m.optional_names();
    let required_params_lit = param_names_literal(&required_params);
    let optional_arg = optional_param_names_arg(&optional_params);
    // A static method binds only its own `<...>` params (a generic static never
    // re-binds the class params — the compiler forbids that ambiguity); an
    // instance method also binds the enclosing class's params, recovered from
    // the `self` receiver. Mirrors the Python SDK's `class_type_params` rule.
    let class_type_params: &[String] = match m.kind {
        MethodKind::Static => &[],
        MethodKind::Instance => class_generics,
    };
    let tail = factory_tail(&optional_arg, &m.generic_params, class_type_params);
    match m.kind {
        MethodKind::Static => {
            state.uses_define_function = true;
            let _ = writeln!(
                out,
                "  static {} = defineFunction(\"{}\", \"{}\", {required_params_lit}{tail}) as {sig};",
                m.name,
                m.baml_fqn,
                mode_str(m.mode),
            );
        }
        MethodKind::Instance => {
            state.uses_define_instance = true;
            let _ = writeln!(
                out,
                "  {} = defineInstanceFunction(\"{}\", \"{}\", {required_params_lit}{tail}).bind(this) as {sig};",
                m.name,
                m.baml_fqn,
                mode_str(m.mode),
            );
        }
    }
}

fn render_function_ts(
    out: &mut String,
    f: &TypeScriptFunction,
    ctx: &TranslateCtx,
    state: &mut RenderState,
    child_namespace_alias: Option<&str>,
) {
    write_doc_with_raises(out, f.docstring.as_deref(), &f.raises_names);
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
    if !f.generic_params.is_empty() {
        state.uses_baml_type = true;
    }
    let sig = fn_type_sig(
        &f.generic_params,
        &f.generic_params,
        &names,
        &tys,
        &f.arg_defaults,
        &ret.expr,
        is_async,
    );
    let (required_params, optional_params) = split_param_names(&f.param_names, &f.arg_defaults, 0);
    let required_params_lit = param_names_literal(&required_params);
    let optional_arg = optional_param_names_arg(&optional_params);
    // Free functions bind only their own `<...>` params (no generic receiver).
    let tail = factory_tail(&optional_arg, &f.generic_params, &[]);
    let mut factory = format!(
        "defineFunction(\"{}\", \"{}\", {required_params_lit}{tail}) as {sig}",
        f.baml_fqn,
        mode_str(f.mode),
    );
    if let Some(alias) = child_namespace_alias {
        factory = format!("Object.assign({factory}, {alias})");
    }
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

fn optional_param_names_arg(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!(", {}", param_names_literal(names))
    }
}

/// The `{ typeParams, classTypeParams }` literal passed to the runtime factory
/// to turn on host-side `TypeVar` binding. `type_params` are the callee's own
/// `<...>` params (bound via the caller's `$types` option); `class_type_params`
/// are the enclosing generic class's params (bound from the `self` receiver).
/// `None` when the callee binds nothing (the non-generic fast path). Mirrors
/// the Python SDK's `render_generic_kwargs`.
fn generics_object_literal(type_params: &[String], class_type_params: &[String]) -> Option<String> {
    if type_params.is_empty() && class_type_params.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !type_params.is_empty() {
        parts.push(format!("typeParams: {}", param_names_literal(type_params)));
    }
    if !class_type_params.is_empty() {
        parts.push(format!(
            "classTypeParams: {}",
            param_names_literal(class_type_params)
        ));
    }
    Some(format!("{{ {} }}", parts.join(", ")))
}

/// The trailing factory arguments after the required-param-names list: the
/// optional-param-names list (if any) followed by the generics object (if the
/// callee is generic). When a callee is generic but has no optional params, the
/// optional slot is filled with `undefined` so the generics object lands in the
/// correct positional slot.
fn factory_tail(
    optional_arg: &str,
    type_params: &[String],
    class_type_params: &[String],
) -> String {
    match generics_object_literal(type_params, class_type_params) {
        None => optional_arg.to_string(),
        Some(generics) => {
            let optional = if optional_arg.is_empty() {
                ", undefined"
            } else {
                optional_arg
            };
            format!("{optional}, {generics}")
        }
    }
}

fn required_positional_count(defaults: &[Option<FunctionArgumentDefault>]) -> usize {
    defaults
        .iter()
        .take_while(|default| default.is_none())
        .count()
}

fn split_param_names(
    names: &[String],
    arg_defaults: &[Option<FunctionArgumentDefault>],
    receiver_count: usize,
) -> (Vec<String>, Vec<String>) {
    let required = receiver_count + required_positional_count(arg_defaults);
    (names[..required].to_vec(), names[required..].to_vec())
}

#[cfg(test)]
mod tests {
    use baml_base::{Literal, Name as BaseName};
    use baml_codegen_types::{DefaultLiteral, FunctionArgumentDefault, Name, Ty};

    use super::*;

    const TEST_RUNTIME_PACKAGE: &str = "@boundaryml/baml-bridge";

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
        EmittedSymbol::Class(TypeScriptClass {
            name: n.to_string(),
            source,
            generic_params: Vec::new(),
            docstring: None,
            properties: props
                .into_iter()
                .map(|(pn, ty)| crate::emit::class::TypeScriptClassProperty {
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
        EmittedSymbol::Enum(TypeScriptEnum {
            name: n.to_string(),
            source,
            variants: variants
                .iter()
                .map(|v| crate::emit::enum_::TypeScriptEnumVariant {
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
        let param_names: Vec<String> = params.iter().map(|(n, _)| n.to_string()).collect();
        let arg_tys: Vec<Ty> = params.into_iter().map(|(_, t)| t).collect();
        EmittedSymbol::Function(TypeScriptFunction {
            name: n.to_string(),
            baml_fqn: fqn.to_string(),
            mode,
            param_names,
            arg_defaults: vec![None; arg_tys.len()],
            arg_tys,
            return_ty: ret,
            generic_params: Vec::new(),
            docstring: None,
            raises_names: Vec::new(),
        })
    }

    fn func_sym_with_defaults(
        n: &str,
        fqn: &str,
        mode: SyncAsync,
        params: Vec<(&str, Ty, Option<FunctionArgumentDefault>)>,
        ret: Ty,
    ) -> EmittedSymbol {
        EmittedSymbol::Function(TypeScriptFunction {
            name: n.to_string(),
            baml_fqn: fqn.to_string(),
            mode,
            param_names: params.iter().map(|(n, _, _)| n.to_string()).collect(),
            arg_tys: params.iter().map(|(_, t, _)| t.clone()).collect(),
            arg_defaults: params.into_iter().map(|(_, _, d)| d).collect(),
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
                vec![
                    (
                        "name",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ),
                    (
                        "age",
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ),
                ],
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains("export class Resume {"));
        assert!(ts.contains("name!: string;"));
        assert!(ts.contains("age!: number;"));
        assert!(ts.contains("Object.assign(this, init);"));
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
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains("export enum Sentiment {"));
        assert!(ts.contains("HAPPY = \"HAPPY\","));
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
                    vec![(
                        "text",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    )],
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ),
                func_sym(
                    "extract_async",
                    "user.lorem.extract",
                    SyncAsync::Async,
                    vec![(
                        "text",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    )],
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ),
            ],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains(
            "import { defineFunction, type BamlCallContext } from \"@boundaryml/baml-bridge\";"
        ));
        assert!(ts.contains("export const extract = defineFunction(\"user.lorem.extract\", \"sync\", [\"text\"]) as (text: string, $opts?: { $ctx?: BamlCallContext | undefined } | undefined) => number;"));
        assert!(ts.contains("export const extract_async = defineFunction(\"user.lorem.extract\", \"async\", [\"text\"]) as (text: string, $opts?: { $ctx?: BamlCallContext | undefined } | undefined) => Promise<number>;"));
    }

    #[test]
    fn optional_opts_fields_preserve_reserved_baml_names() {
        let b = body(
            &["lorem"],
            vec![func_sym_with_defaults(
                "extract",
                "user.lorem.extract",
                SyncAsync::Sync,
                vec![
                    (
                        "arg0",
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        None,
                    ),
                    (
                        "default",
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                            Literal::Int(1),
                        ))),
                    ),
                    (
                        "not-valid",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                            Literal::String("x".to_string()),
                        ))),
                    ),
                ],
                Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains(
            "as (arg0: number, $opts?: { default?: number | undefined; \"not-valid\"?: string | undefined; $ctx?: BamlCallContext | undefined } | undefined) => number;"
        ));
        assert!(!ts.contains("default_?:"));
    }

    #[test]
    fn strict_mode_and_projected_parameter_collisions_keep_wire_names() {
        let b = body(
            &["lorem"],
            vec![func_sym_with_defaults(
                "extract",
                "user.lorem.extract",
                SyncAsync::Sync,
                vec![
                    (
                        "arguments",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        None,
                    ),
                    (
                        "arguments_",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        None,
                    ),
                    (
                        "$opts",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        None,
                    ),
                    (
                        "eval",
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        Some(FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(
                            Literal::String("x".to_string()),
                        ))),
                    ),
                ],
                Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains(
            "defineFunction(\"user.lorem.extract\", \"sync\", [\"arguments\", \"arguments_\", \"$opts\"], [\"eval\"])"
        ));
        assert!(ts.contains(
            "as (arguments_: string, arguments__: string, $opts_: string, $opts?: { eval?: string | undefined; $ctx?: BamlCallContext | undefined } | undefined) => string;"
        ));
    }

    #[test]
    fn cross_leaf_field_imports_seg0() {
        let b = body(
            &["consumer"],
            vec![class_sym(
                "Holder",
                name("user", &["consumer"], "Holder"),
                vec![(
                    "r",
                    Ty::Class(
                        name("user", &["lorem"], "Resume"),
                        Box::new([]),
                        baml_base::TyAttr::EMPTY,
                    ),
                )],
            )],
        );
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains("import type * as lorem from \"../lorem/index.js\";"));
        assert!(ts.contains("r!: lorem.Resume;"));
    }

    #[test]
    fn runtime_owned_reexports_use_the_configured_package_and_only_exact_bases() {
        let mut symbols = Vec::new();
        for (fqn, _) in RUNTIME_OWNED_CLASS_REEXPORTS {
            let parts = fqn.split('.').collect::<Vec<_>>();
            let class_name = parts[parts.len() - 1];
            symbols.push(class_sym(
                class_name,
                name(parts[0], &parts[1..parts.len() - 1], class_name),
                vec![],
            ));
        }
        symbols.push(class_sym(
            "Image$stream",
            name("baml", &["media"], "Image$stream"),
            vec![],
        ));
        symbols.push(class_sym(
            "UserImage",
            name("user", &["media"], "UserImage"),
            vec![],
        ));
        let b = body(&["baml", "media"], symbols);

        for runtime_package in ["@boundaryml/baml-bridge", "@boundaryml/baml-bridge-web"] {
            let ts = render_index_ts(&b, &BTreeSet::new(), false, runtime_package);
            for (fqn, runtime_name) in RUNTIME_OWNED_CLASS_REEXPORTS {
                let local_name = fqn.rsplit('.').next().unwrap();
                assert!(ts.contains(&format!(
                    "import {{ {runtime_name} as {local_name} }} from \"{runtime_package}\";"
                )));
                assert!(ts.contains(&format!("export {{ {local_name} }};")));
            }
            assert!(ts.contains("export class Image$stream {"));
            assert!(ts.contains("export class UserImage {"));
            assert!(!ts.contains("export type Image"));
        }
    }

    #[test]
    fn container_reexports_children() {
        let b = body(&["vendor"], vec![]);
        let mut kids = BTreeSet::new();
        kids.insert("aws".to_string());
        let ts = render_index_ts(&b, &kids, false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains("export * as aws from \"./aws/index.js\";"));
        assert!(!ts.contains("export const"));
    }

    #[test]
    fn callable_child_collision_composes_function_with_namespace() {
        let b = body(
            &["vendor", "boundary"],
            vec![
                func_sym(
                    "id",
                    "boundary.id",
                    SyncAsync::Sync,
                    vec![],
                    Ty::Class(
                        name("boundary", &[], "LocalId"),
                        Box::new([]),
                        baml_base::TyAttr::EMPTY,
                    ),
                ),
                func_sym(
                    "id_async",
                    "boundary.id",
                    SyncAsync::Async,
                    vec![],
                    Ty::Class(
                        name("boundary", &[], "LocalId"),
                        Box::new([]),
                        baml_base::TyAttr::EMPTY,
                    ),
                ),
                class_sym("LocalId", name("boundary", &[], "LocalId"), vec![]),
            ],
        );
        let mut kids = BTreeSet::new();
        kids.insert("id".to_string());
        let ts = render_index_ts(&b, &kids, false, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains("import * as __ns_id from \"./id/index.js\";"));
        assert!(!ts.contains("export * as id from \"./id/index.js\";"));
        assert!(ts.contains(
            "export const id = Object.assign(defineFunction(\"boundary.id\", \"sync\", []) as ($opts?: { $ctx?: BamlCallContext | undefined } | undefined) => LocalId, __ns_id);"
        ));
        assert!(
            ts.contains(
                "export const id_async = defineFunction(\"boundary.id\", \"async\", []) as ($opts?: { $ctx?: BamlCallContext | undefined } | undefined) => Promise<LocalId>;"
            )
        );
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
                Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
            )],
        );
        let mut kids = BTreeSet::new();
        kids.insert("lorem".to_string());
        let ts = render_index_ts(&b, &kids, true, TEST_RUNTIME_PACKAGE);
        assert!(ts.contains(
            "initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE, _inlinedbaml.BAML_TOML);"
        ));
        assert!(ts.contains("setTypeMap(_TYPE_MAP);"));
        assert!(ts.contains("export * as lorem from \"./lorem/index.js\";"));
        assert!(ts.contains("export const make_foo = defineFunction("));
        assert!(ts.contains("import { defineFunction, initializeRuntimeFromBytecode, setTypeMap, type BamlCallContext } from \"@boundaryml/baml-bridge\";"));
    }
}
