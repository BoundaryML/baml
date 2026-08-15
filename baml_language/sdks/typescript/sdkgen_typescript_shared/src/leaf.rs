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

use baml_base::qualified_name::AI_STREAM_STREAM;
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
    /// Set when a generic class emits a `$types` field, which references the
    /// runtime `BamlType` token type.
    uses_baml_type: bool,
}

impl RenderState {
    fn merge(&mut self, t: &TranslatedType) {
        for p in &t.imports {
            self.imports.insert(p.clone());
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

/// Raw BAML type-parameter name → the TypeScript binder identifier allocated
/// for it in one generic scope. Mirrors the Python SDK's `TypeVarMap`.
pub(crate) type TypeVarMap = BTreeMap<String, String>;

/// Resolve one raw type-parameter name to the identifier its declaration
/// allocated. The scope map wins; the stateless [`safe_decl_name`] is the
/// fallback for scopes that bind no type parameters (type-alias bodies,
/// non-generic classes and callables), where no twin can exist.
pub(crate) fn emitted_binder(raw: &str, map: Option<&TypeVarMap>) -> String {
    map.and_then(|m| m.get(raw).cloned())
        .unwrap_or_else(|| safe_decl_name(raw))
}

/// Allocate the emitted binder identifier for every type parameter of ONE
/// generic scope, against a reservation set, and return the scope's full
/// raw→emitted map (the enclosing scope's entries included).
///
/// This is the TypeScript counterpart of the Python SDK's
/// `leaf::allocate_leaf_type_vars`, narrowed to TypeScript's scoping rules. The
/// Python allocator reserves LEAF-globally because a Python `TypeVar` is a
/// module-level assignment (`T = typing.TypeVar("T")`), so two scopes really do
/// share one binding. A TypeScript type parameter is scoped to its own
/// declaration, so the allocation unit here is the scope, not the leaf. The
/// collision-resolution rule is otherwise identical.
///
/// - `raw_params` are this scope's OWN type parameters, in declaration order.
/// - `outer` is the enclosing scope's map (a generic class, for an instance
///   method) or `None`. A static method re-declares the class parameters as its
///   own (TS2302), so it passes them in `raw_params` with no `outer`.
/// - `module_names` are the leaf's module-scope bindings a binder could shadow.
///
/// Guarantees, matching the Python allocator's:
/// - **A non-reserved raw name maps to itself, unconditionally.** The
///   reservation set is only ever consulted for a raw name that is a JavaScript
///   reserved word, and bumping only appends `_`. Every keyword-free schema
///   therefore renders byte-identically to the stateless escape it replaces.
/// - **A reserved raw name bumps** (`package`→`package_`→`package__`…) past
///   (a) every raw type-parameter name in this scope and its enclosing scope,
///   (b) every binder already allocated in this scope chain, and (c) the leaf's
///   module-scope declaration names and immediate child-namespace names.
/// - The map is keyed by RAW name, so a `{package, package_}` twin can never
///   collapse onto one identifier, and `translate_ty` resolves each use site
///   through the same map.
///
/// **Deliberately not reserved.** Runtime import names (`defineFunction`,
/// `BamlCallContext`, `_BamlHandle`, `_TYPE_MAP`, `__ns_*`, `__baml_*`) are
/// unreachable by construction: bumping only appends `_` to a reserved word, so
/// an allocated binder is never `_`-leading and never equals one of them. The
/// cross-leaf `import type * as <segment>` aliases are NOT reserved: they are
/// routing-sanitized module path segments, they are not known until the leaf's
/// bodies have been rendered, and shadowing one inside a type-parameter list is
/// a resolution change rather than a parse error. That bound is stated here
/// rather than papered over.
/// An inner scope's NON-reserved parameter is likewise not reserved against.
/// `Box<package>` allocates `package_`, and an instance method that declares a
/// parameter literally named `package_` re-binds that identifier for the whole
/// method. Shadowing a type parameter is legal TypeScript and compiles clean,
/// so this is a name-resolution change of the same category as the import
/// aliases above, not a parse error. Widening the bump to cover it would
/// destroy the unconditional non-reserved-maps-to-itself guarantee, so it is
/// stated rather than fixed.
fn allocate_binders(
    raw_params: &[String],
    outer: Option<&TypeVarMap>,
    module_names: &BTreeSet<String>,
) -> TypeVarMap {
    let mut map: TypeVarMap = outer.cloned().unwrap_or_default();
    if raw_params.is_empty() {
        return map;
    }
    let mut reserved: BTreeSet<String> = BTreeSet::new();
    // (a) every raw type-parameter name in this scope — so `package` cannot bump
    //     onto a sibling `package_` that maps to itself.
    reserved.extend(raw_params.iter().cloned());
    // (b) the enclosing scope's raw names and the binders already allocated for
    //     them. This is a BUMP target only: it stops a reserved-word inner
    //     parameter from bumping onto an identifier the enclosing scope is
    //     already using, which would silently retarget that scope's references.
    //     It does NOT stop a non-reserved inner parameter from re-binding an
    //     outer name: that branch maps the raw name to itself unconditionally
    //     and never reads this set. That case is ordinary TypeScript shadowing,
    //     legal and compile-clean, and is noted with the other unreserved
    //     surfaces above.
    for (raw, emitted) in &map {
        reserved.insert(raw.clone());
        reserved.insert(emitted.clone());
    }
    // (c) the leaf's module-scope declaration and child-namespace names.
    reserved.extend(module_names.iter().cloned());

    for raw in raw_params {
        if !is_js_reserved(raw) {
            map.insert(raw.clone(), raw.clone());
        } else {
            let mut candidate = format!("{raw}_");
            while is_js_reserved(&candidate) || reserved.contains(&candidate) {
                candidate.push('_');
            }
            reserved.insert(candidate.clone());
            map.insert(raw.clone(), candidate);
        }
    }
    map
}

/// The leaf's module-scope bindings a type-parameter binder could shadow: every
/// emitted declaration name in the body, plus every immediate child-namespace
/// name re-exported by this `index.ts`.
fn module_scope_names(body: &LeafBody, kids: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = kids.clone();
    for (sym, _) in &body.symbols {
        out.insert(symbol_decl_name(sym).to_string());
    }
    out
}

fn symbol_decl_name(sym: &EmittedSymbol) -> &str {
    match sym {
        EmittedSymbol::Class(c) => &c.name,
        EmittedSymbol::Enum(e) => &e.name,
        EmittedSymbol::TypeAlias(a) => &a.name,
        EmittedSymbol::Function(f) => &f.name,
    }
}

/// `<T, U>` generic-parameter list, or empty. A type-parameter name is a
/// binding identifier, so it cannot be a reserved word; `map` carries the
/// identifier allocated for each raw name by [`allocate_binders`], and
/// `translate_ty` resolves every `Ty::TypeVar` use site through the same map.
/// The raw spellings still reach the runtime through the `$generic` array and
/// the `typeParams` factory argument, which are string literals.
fn generic_decl(params: &[String], map: &TypeVarMap) -> String {
    if params.is_empty() {
        String::new()
    } else {
        let escaped: Vec<String> = params
            .iter()
            .map(|p| emitted_binder(p.as_str(), Some(map)))
            .collect();
        format!("<{}>", escaped.join(", "))
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

/// A DECLARATION name binds an identifier in module scope, so unlike an enum
/// member or a class property (both `IdentifierName`, where reserved words are
/// legal) it cannot be a reserved word. `export enum import {}` is TS1359, and
/// because it is a parse error it kills the whole generated file rather than
/// just that one symbol. Append `_`, the same rule [`safe_param_name`] uses and
/// the same rule the Python SDK's `escape_python_keyword` uses, so a BAML
/// `enum import` renders as `export enum import_ {}`.
///
/// The escape is deliberately STATELESS. A class / enum / type-alias name has
/// to be reproducible from the bare BAML `Name` alone at a distance, because
/// `translate_ty::render_name_ref` re-derives the reference from the IR at
/// every cross-reference rather than reading the emitted declaration back.
///
/// One bound follows from that statelessness: a leaf declaring BOTH a reserved
/// word and its `_`-suffixed twin (`import` alongside `import_`) collapses them
/// onto a single identifier, which is not a regression because at base that same
/// leaf emitted `export class import {}`, a TS1359 parse error that already
/// killed the whole generated file.
///
/// Only the TypeScript identifier moves. Wire identity travels on separate
/// channels and is untouched: `defineFunction` is handed `baml_fqn`,
/// `_typemap.ts` is keyed on `TypeScriptClass::source`, and enum member values
/// are emitted verbatim, so dispatch and marshalling still target `import`.
pub(crate) fn safe_decl_name(name: &str) -> String {
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
    generic_map: &TypeVarMap,
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
        params.push(format!("$opts?: {{ {} }} | undefined", fields.join("; ")));
    }
    let ret = if is_async {
        format!("Promise<{ret_expr}>")
    } else {
        ret_expr.to_string()
    };
    format!(
        "{}({}) => {ret}",
        generic_decl(generics, generic_map),
        params.join(", ")
    )
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
        type_var_map: None,
    };
    let mut state = RenderState::default();
    let callable_child_aliases = body.callable_child_aliases(kids);
    // Module-scope bindings a generic binder must not shadow. Computed once per
    // leaf and only ever consulted for a reserved-word type parameter.
    let module_names = module_scope_names(body, kids);

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
            &module_names,
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
    module_names: &BTreeSet<String>,
    runtime_package: &str,
) {
    match sym {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = runtime_owned_reexport_name(c) {
                render_media_reexport_ts(out, &c.name, rust_name, runtime_package);
            } else {
                render_class_ts(out, c, ctx, state, module_names);
            }
        }
        EmittedSymbol::Enum(e) => render_enum(out, e),
        EmittedSymbol::TypeAlias(a) => render_type_alias(out, a, ctx, state),
        EmittedSymbol::Function(f) => render_function_ts(
            out,
            f,
            ctx,
            state,
            module_names,
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
    module_names: &BTreeSet<String>,
) {
    write_class_doc(out, c);
    // The class's type parameters are one generic scope: allocate their binders
    // once, then render the `<…>` list, every field type, and every instance
    // method signature through the same map.
    let class_map = std::rc::Rc::new(allocate_binders(&c.generic_params, None, module_names));
    let class_ctx = ctx.with_type_vars(&class_map);
    let generics = generic_decl(&c.generic_params, &class_map);

    // Translate each property type once; reuse for field + constructor.
    let props: Vec<(&str, TranslatedType)> = c
        .properties
        .iter()
        .map(|p| {
            let t = translate_ty(&p.ty, &class_ctx);
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
            .map(|p| format!("{p}?: BamlType"))
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
        render_method_binding_ts(
            out,
            m,
            &c.generic_params,
            &class_map,
            ctx,
            state,
            module_names,
        );
    }
    for m in &c.instance_methods {
        render_method_binding_ts(
            out,
            m,
            &c.generic_params,
            &class_map,
            ctx,
            state,
            module_names,
        );
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
    class_map: &TypeVarMap,
    ctx: &TranslateCtx,
    state: &mut RenderState,
    module_names: &BTreeSet<String>,
) {
    write_doc_with_raises(out, m.docstring.as_deref(), &m.raises_names);
    let is_async = m.mode == SyncAsync::Async;
    let sig_generics = method_sig_generics(m, class_generics);
    // A static method re-declares the class parameters as its own (TS2302), so
    // its scope is the flat `sig_generics` list with no enclosing map. An
    // instance method binds only its own parameters on top of the class scope,
    // so the class map is the outer scope and its binders are reserved against.
    // `sig_generics` is exactly the scope's own parameter list in both cases.
    let outer = match m.kind {
        MethodKind::Static => None,
        MethodKind::Instance => Some(class_map),
    };
    let sig_map = std::rc::Rc::new(allocate_binders(&sig_generics, outer, module_names));
    let method_ctx = ctx.with_type_vars(&sig_map);
    let (names, tys, defaults, ret) = binding_surface(m, &method_ctx, state);
    let sig = fn_type_sig(
        &sig_generics,
        &sig_map,
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
    module_names: &BTreeSet<String>,
    child_namespace_alias: Option<&str>,
) {
    write_doc_with_raises(out, f.docstring.as_deref(), &f.raises_names);
    state.uses_define_function = true;
    // A free function's type parameters are one generic scope of their own.
    let fn_map = std::rc::Rc::new(allocate_binders(&f.generic_params, None, module_names));
    let fn_ctx = ctx.with_type_vars(&fn_map);
    let tys: Vec<TranslatedType> = f
        .arg_tys
        .iter()
        .map(|t| {
            let tt = translate_ty(t, &fn_ctx);
            state.merge(&tt);
            tt
        })
        .collect();
    let ret = translate_ty(&f.return_ty, &fn_ctx);
    state.merge(&ret);
    let names: Vec<&str> = f.param_names.iter().map(String::as_str).collect();
    let is_async = f.mode == SyncAsync::Async;
    let sig = fn_type_sig(
        &f.generic_params,
        &fn_map,
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
    fn safe_decl_name_escapes_only_reserved_words() {
        assert_eq!(safe_decl_name("import"), "import_");
        assert_eq!(safe_decl_name("class"), "class_");
        assert_eq!(safe_decl_name("interface"), "interface_");
        assert_eq!(safe_decl_name("Resume"), "Resume");
        // Companion suffixes survive: `$` is a legal TS identifier character.
        assert_eq!(safe_decl_name("Resume$stream"), "Resume$stream");
        // Python keywords that are not reserved in TypeScript stay put, so the
        // shared `reserved_keywords` fixture keeps per-language spellings
        // rather than converging on one escaped set.
        assert_eq!(safe_decl_name("None"), "None");
        assert_eq!(safe_decl_name("pass"), "pass");
        assert_eq!(safe_decl_name("lambda"), "lambda");
    }

    #[test]
    fn generic_decl_escapes_reserved_type_parameters() {
        let decl = |params: &[&str]| {
            let raw: Vec<String> = params.iter().map(ToString::to_string).collect();
            let map = allocate_binders(&raw, None, &BTreeSet::new());
            generic_decl(&raw, &map)
        };
        assert_eq!(decl(&[]), "");
        assert_eq!(decl(&["T"]), "<T>");
        assert_eq!(decl(&["package", "T"]), "<package_, T>");
    }

    /// The `{package, package_}` twin: the stateless escape maps BOTH raw names
    /// onto `package_`, so the binder list would read `<package_, package_>` —
    /// TS2300 duplicate identifier, which kills the whole generated file. The
    /// allocator reserves the sibling raw name, so `package` bumps past it.
    #[test]
    fn reserved_and_underscore_twin_get_distinct_binders() {
        let raw = vec!["package".to_string(), "package_".to_string()];
        // Control: the stateless escape collapses the twin.
        assert_eq!(safe_decl_name("package"), safe_decl_name("package_"));

        let map = allocate_binders(&raw, None, &BTreeSet::new());
        assert_eq!(map.get("package").map(String::as_str), Some("package__"));
        assert_eq!(map.get("package_").map(String::as_str), Some("package_"));
        assert_eq!(generic_decl(&raw, &map), "<package__, package_>");
    }

    /// Declaration order does not matter: whichever reserved-word parameter is
    /// allocated first still bumps past the sibling that maps to itself.
    #[test]
    fn twin_binders_are_distinct_in_either_declaration_order() {
        let raw = vec!["package_".to_string(), "package".to_string()];
        let map = allocate_binders(&raw, None, &BTreeSet::new());
        assert_eq!(generic_decl(&raw, &map), "<package_, package__>");
    }

    /// A binder never lands on a module-scope declaration name in the same leaf.
    #[test]
    fn binder_does_not_shadow_a_module_scope_declaration() {
        let module_names: BTreeSet<String> = ["package_".to_string()].into_iter().collect();
        let raw = vec!["package".to_string()];
        let map = allocate_binders(&raw, None, &module_names);
        assert_eq!(generic_decl(&raw, &map), "<package__>");
    }

    /// Keyword-free schemas are untouched: a non-reserved raw name maps to
    /// itself unconditionally, so the reservation set is never consulted and
    /// output stays byte-identical to the stateless escape it replaces.
    #[test]
    fn non_reserved_binders_are_never_bumped() {
        let module_names: BTreeSet<String> =
            ["T".to_string(), "U".to_string()].into_iter().collect();
        let raw = vec!["T".to_string(), "U".to_string()];
        let map = allocate_binders(&raw, None, &module_names);
        assert_eq!(generic_decl(&raw, &map), "<T, U>");
    }

    /// End-to-end through `render_index_ts`: a generic class declaring both
    /// `package` and `package_` renders distinct binders, and every use site
    /// (field types, the constructor init object) resolves to the binder its
    /// declaration allocated rather than re-deriving a colliding escape.
    #[test]
    fn generic_class_with_reserved_twin_renders_distinct_binders() {
        let type_var = |n: &str| {
            Ty::TypeVar(
                baml_codegen_types::ParamTy::new(0, BaseName::new(n)),
                baml_base::TyAttr::EMPTY,
            )
        };
        let c = TypeScriptClass {
            name: "Pair".to_string(),
            source: name("user", &["lorem"], "Pair"),
            generic_params: vec!["package".to_string(), "package_".to_string()],
            docstring: None,
            properties: vec![
                crate::emit::class::TypeScriptClassProperty {
                    name: "first".to_string(),
                    ty: type_var("package"),
                    docstring: None,
                },
                crate::emit::class::TypeScriptClassProperty {
                    name: "second".to_string(),
                    ty: type_var("package_"),
                    docstring: None,
                },
            ],
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        };
        let b = body(&["lorem"], vec![EmittedSymbol::Class(c)]);
        let ts = render_index_ts(&b, &BTreeSet::new(), false, TEST_RUNTIME_PACKAGE);

        assert!(
            ts.contains("export class Pair<package__, package_> {"),
            "binders collided or were not allocated:\n{ts}"
        );
        assert!(
            ts.contains("first!: package__;"),
            "`package` use site did not resolve to its binder:\n{ts}"
        );
        assert!(
            ts.contains("second!: package_;"),
            "`package_` use site did not resolve to its binder:\n{ts}"
        );
        // The wire channels stay RAW — the encoder positions `$types` bindings
        // by the BAML spelling, not the TypeScript identifier.
        assert!(
            ts.contains("static readonly $generic = [\"package\", \"package_\"] as const;"),
            "raw generic names must survive on the wire channel:\n{ts}"
        );
    }

    /// Instance-method scope, the `Some(class_map)` arm of the `outer` match in
    /// `render_method_binding_ts`. A method that re-declares a reserved-word
    /// parameter must bump clear of BOTH the class's raw parameter names and the
    /// binders the class already allocated for them. Landing on `package_` would
    /// silently retarget every class-level reference that resolves through that
    /// binder, which is a wrong-type error the compiler cannot catch.
    #[test]
    fn instance_method_binder_bumps_past_the_outer_class_scope() {
        let module_names: BTreeSet<String> = ["Pair".to_string()].into_iter().collect();
        let class_raw = vec!["package".to_string(), "package_".to_string()];
        let class_map = allocate_binders(&class_raw, None, &module_names);
        assert_eq!(
            class_map.get("package").map(String::as_str),
            Some("package__")
        );
        assert_eq!(
            class_map.get("package_").map(String::as_str),
            Some("package_")
        );

        // The instance arm: the method's OWN parameters only, with the class map
        // as the enclosing scope.
        let method_raw = vec!["package".to_string()];
        let sig_map = allocate_binders(&method_raw, Some(&class_map), &module_names);

        // KILL ASSERTION. Without the outer-scope reservation loop the candidate
        // `package_` is unreserved, so the method binder lands exactly on the
        // class's own `package_` binder. Reserving the outer raw names AND the
        // outer binders is what pushes it to `package___`.
        assert_eq!(
            sig_map.get("package").map(String::as_str),
            Some("package___"),
            "method binder did not bump clear of the outer scope: {sig_map:?}"
        );

        // Stated the other way: clear of every outer raw name and every outer
        // binder, not just the one this fixture happens to collide with.
        let allocated = sig_map.get("package").cloned().unwrap();
        for (raw, emitted) in &class_map {
            assert_ne!(
                &allocated, raw,
                "method binder collided with an outer raw name"
            );
            assert_ne!(
                &allocated, emitted,
                "method binder collided with an outer binder"
            );
        }

        // The enclosing scope's entries survive into the method map, so a
        // class-level reference inside the method body still resolves.
        assert_eq!(
            sig_map.get("package_").map(String::as_str),
            Some("package_")
        );
        assert_eq!(generic_decl(&method_raw, &sig_map), "<package___>");
    }

    /// Static-method scope, the `None` arm of the same match. A static member
    /// cannot reference the class's type parameters (TS2302), so
    /// `method_sig_generics` flattens the class params and the method's own into
    /// ONE list that is allocated with no enclosing scope. The flat list must
    /// still allocate distinct binders, and re-declaring the class params must
    /// reproduce the class's binders exactly.
    #[test]
    fn static_method_scope_allocates_the_flattened_list_with_no_outer() {
        let module_names: BTreeSet<String> = ["Box".to_string()].into_iter().collect();
        let class_raw = vec!["package".to_string(), "package_".to_string()];
        let class_map = allocate_binders(&class_raw, None, &module_names);

        let m = TypeScriptMethodBinding {
            name: "build".to_string(),
            baml_fqn: "user.lorem.Box.build".to_string(),
            mode: SyncAsync::Sync,
            kind: MethodKind::Static,
            required_args: Vec::new(),
            optional_args: Vec::new(),
            return_ty: Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            },
            generic_params: vec!["T".to_string()],
            docstring: None,
            raises_names: Vec::new(),
        };

        let sig_generics = method_sig_generics(&m, &class_raw);
        assert_eq!(
            sig_generics,
            vec![
                "package".to_string(),
                "package_".to_string(),
                "T".to_string()
            ],
            "static signature must re-declare the class params ahead of its own"
        );

        let sig_map = allocate_binders(&sig_generics, None, &module_names);
        assert_eq!(
            sig_map.get("package").map(String::as_str),
            Some("package__")
        );
        assert_eq!(
            sig_map.get("package_").map(String::as_str),
            Some("package_")
        );
        assert_eq!(sig_map.get("T").map(String::as_str), Some("T"));
        // Three raw names, three distinct binders.
        let distinct: BTreeSet<&String> = sig_map.values().collect();
        assert_eq!(distinct.len(), sig_map.len());
        assert_eq!(
            generic_decl(&sig_generics, &sig_map),
            "<package__, package_, T>"
        );
        // Re-declaring the class params reproduces the class binders exactly.
        // That is what passing no enclosing scope buys.
        assert_eq!(sig_map.get("package"), class_map.get("package"));
        assert_eq!(sig_map.get("package_"), class_map.get("package_"));

        // KILL ASSERTION, as a control on the arm this branch deliberately does
        // not take. Allocating the same flat list WITH the class map as an
        // enclosing scope reserves the outer binders, so `package` bumps one
        // further to `package___` and the static signature drifts off the class
        // declaration. If the outer-scope reservation loop were removed, this
        // allocation would collapse back onto the no-outer result and both
        // assertions below would fail.
        let with_outer = allocate_binders(&sig_generics, Some(&class_map), &module_names);
        assert_eq!(
            with_outer.get("package").map(String::as_str),
            Some("package___"),
            "outer-scope reservation did not apply: {with_outer:?}"
        );
        assert_ne!(with_outer.get("package"), sig_map.get("package"));
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
                        vec![],
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
                        vec![],
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
                        vec![],
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
