//! Per-leaf symbol bundle and leaf renderer. One `LeafBody` per
//! populated leaf file, pre-sorted at build time so the renderer is a
//! straight walk — no ordering logic at render time.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use baml_codegen_types::Ty;

use crate::{
    emit::{
        EmittedSymbol, SortKey,
        function::{PyFunction, SyncAsync},
        method::{MethodKind, PyMethodBinding},
    },
    py_string,
    routing::{LeafPath, route_class_ref},
    translate_ty::{TranslateCtx, translate_ty},
};

/// All symbols that land in one leaf's body, in final render order.
/// Each entry keeps its `SortKey` so the renderer can group function
/// fan-out siblings tightly (they share their parent's sort key)
/// while separating unrelated top-level definitions.
pub(crate) struct LeafBody {
    pub(crate) leaf: LeafPath,
    pub(crate) symbols: Vec<(EmittedSymbol, SortKey)>,
}

impl LeafBody {
    /// Public `py_name`s in render order — the contents of `__all__`.
    /// Filters underscore-prefixed names defensively.
    pub(crate) fn all_names(&self) -> Vec<&str> {
        self.symbols
            .iter()
            .map(|(s, _)| s.py_name())
            .filter(|n| !n.starts_with('_'))
            .collect()
    }

    pub(crate) fn stdlib_imports(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self
            .symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Enum(_)))
        {
            out.push("enum");
        }
        // Class field types and type aliases may use `typing.Optional`,
        // `typing.List`, `typing.TypeAlias`, etc. — be generous and
        // import `typing` whenever a class or alias is present. Media
        // re-export classes render as a one-line import only — they
        // don't need `typing` for their own body.
        if self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => media_reexport_rust_name(c).is_none(),
            EmittedSymbol::TypeAlias(_) => true,
            _ => false,
        }) {
            out.push("typing");
        }
        out
    }

    pub(crate) fn needs_pydantic(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => media_reexport_rust_name(c).is_none(),
            _ => false,
        })
    }

    pub(crate) fn needs_define_function(&self) -> bool {
        self.symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Function(_)))
    }

    pub(crate) fn needs_define_static_method(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => {
                media_reexport_rust_name(c).is_none() && !c.static_methods.is_empty()
            }
            _ => false,
        })
    }

    pub(crate) fn needs_define_instance_method(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => {
                media_reexport_rust_name(c).is_none() && !c.instance_methods.is_empty()
            }
            _ => false,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// First segments of every cross-leaf routed module path referenced
    /// by this leaf's `.py`, deduped and sorted.
    ///
    /// Function/method types don't actually render annotations in the
    /// `.py` (factory bindings only), but they're walked anyway so the
    /// import block is identical across `.py` and `.pyi`. The
    /// `TYPE_CHECKING` guard makes the extras free at runtime.
    pub(crate) fn cross_leaf_first_segments_py(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) if media_reexport_rust_name(c).is_some() => {}
                EmittedSymbol::Class(c) => {
                    for prop in &c.properties {
                        collect_cross_leaf(&prop.ty, current, &mut set);
                    }
                    for m in &c.static_methods {
                        for ty in &m.arg_tys {
                            collect_cross_leaf(ty, current, &mut set);
                        }
                        collect_cross_leaf(&m.return_ty, current, &mut set);
                    }
                    for m in &c.instance_methods {
                        for ty in &m.arg_tys {
                            collect_cross_leaf(ty, current, &mut set);
                        }
                        collect_cross_leaf(&m.return_ty, current, &mut set);
                    }
                }
                EmittedSymbol::Function(f) => {
                    for ty in &f.arg_tys {
                        collect_cross_leaf(ty, current, &mut set);
                    }
                    collect_cross_leaf(&f.return_ty, current, &mut set);
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_cross_leaf(&a.resolves_to, current, &mut set);
                }
                EmittedSymbol::Enum(_) => {}
            }
        }
        set.into_iter().collect()
    }

    /// First segments for the `.pyi` companion. Class fields are not
    /// mirrored into `.pyi` (only methods get typed signatures), so
    /// they don't contribute.
    pub(crate) fn cross_leaf_first_segments_pyi(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) if media_reexport_rust_name(c).is_some() => {}
                EmittedSymbol::Class(c) => {
                    for m in &c.static_methods {
                        for ty in &m.arg_tys {
                            collect_cross_leaf(ty, current, &mut set);
                        }
                        collect_cross_leaf(&m.return_ty, current, &mut set);
                    }
                    for m in &c.instance_methods {
                        for ty in &m.arg_tys {
                            collect_cross_leaf(ty, current, &mut set);
                        }
                        collect_cross_leaf(&m.return_ty, current, &mut set);
                    }
                }
                EmittedSymbol::Function(f) => {
                    for ty in &f.arg_tys {
                        collect_cross_leaf(ty, current, &mut set);
                    }
                    collect_cross_leaf(&f.return_ty, current, &mut set);
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_cross_leaf(&a.resolves_to, current, &mut set);
                }
                EmittedSymbol::Enum(_) => {}
            }
        }
        set.into_iter().collect()
    }

    /// Whether this leaf's `.pyi` needs `import typing`. Any rendered
    /// signature pulls it in (`typing.TypeAlias`, `typing.Optional`,
    /// etc.). Property-only classes don't (fields aren't mirrored into
    /// `.pyi`), but generic classes do (`typing.Generic[T]` base).
    pub(crate) fn needs_typing_pyi(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Function(_) | EmittedSymbol::TypeAlias(_) => true,
            EmittedSymbol::Class(c) => {
                media_reexport_rust_name(c).is_none()
                    && (!c.static_methods.is_empty()
                        || !c.instance_methods.is_empty()
                        || !c.generic_params.is_empty())
            }
            EmittedSymbol::Enum(_) => false,
        })
    }

    /// Union of `TypeVar` identifiers declared by any generic
    /// definition routed here. Alphabetic so emission is deterministic.
    pub(crate) fn generic_typevars(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    for n in &c.generic_params {
                        set.insert(n.clone());
                    }
                    for m in c.static_methods.iter().chain(&c.instance_methods) {
                        for n in &m.generic_params {
                            set.insert(n.clone());
                        }
                    }
                }
                EmittedSymbol::Function(f) => {
                    for n in &f.generic_params {
                        set.insert(n.clone());
                    }
                }
                EmittedSymbol::Enum(_) | EmittedSymbol::TypeAlias(_) => {}
            }
        }
        set.into_iter().collect()
    }
}

/// Add the first routed segment of every `Name`-bearing reference
/// whose routed leaf differs from `current`. Empty-routed-leaf
/// references (root-leaf from a non-root leaf) emit no import — the
/// translator renders those as bare names without prefix.
fn collect_cross_leaf(ty: &Ty, current: &LeafPath, out: &mut BTreeSet<String>) {
    match ty {
        Ty::Class(name, args) => {
            let routed = route_class_ref(name);
            if routed != *current && !routed.segments.is_empty() {
                out.insert(routed.segments[0].clone());
            }
            for a in args {
                collect_cross_leaf(a, current, out);
            }
        }
        Ty::Enum(name) | Ty::TypeAlias(name) => {
            let routed = route_class_ref(name);
            if routed != *current && !routed.segments.is_empty() {
                out.insert(routed.segments[0].clone());
            }
        }
        Ty::Optional(inner) | Ty::List(inner) => collect_cross_leaf(inner, current, out),
        Ty::Map { key, value } => {
            collect_cross_leaf(key, current, out);
            collect_cross_leaf(value, current, out);
        }
        Ty::Union(items) => {
            for item in items {
                collect_cross_leaf(item, current, out);
            }
        }
        Ty::Callable { params, ret } => {
            for p in params {
                collect_cross_leaf(p, current, out);
            }
            collect_cross_leaf(ret, current, out);
        }
        // `Ty::Media(_)` is rendered by `translate_ty` as the literal
        // dotted form `baml.media.Image` etc. — no `Name` involved, but
        // the resulting annotation references the `baml` first-segment,
        // so the leaf needs `baml` imported.
        Ty::Media(_) => {
            out.insert("baml".to_string());
        }
        Ty::Int
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Literal(_)
        | Ty::Uint8Array
        | Ty::TypeVar(_)
        | Ty::BuiltinUnknown
        | Ty::Unit
        | Ty::BamlOptions => {}
    }
}

pub(crate) fn group_and_sort(
    triples: Vec<(LeafPath, EmittedSymbol, SortKey)>,
) -> BTreeMap<LeafPath, LeafBody> {
    let mut buckets: BTreeMap<LeafPath, Vec<(EmittedSymbol, SortKey)>> = BTreeMap::new();
    for (leaf, sym, key) in triples {
        buckets.entry(leaf).or_default().push((sym, key));
    }

    // Stable sort preserves intra-parent function fan-out order (base
    // sync, base async, companions each sync/async).
    //
    // Tertiary tie-breaker: when sort keys collide (PPIR assigns
    // synthetic `$stream` symbols `TextRange::default()` so they share
    // span 0), emit type aliases last — the alias's RHS may reference
    // stream classes in the same leaf, and the class must be defined
    // before the alias's RHS evaluates.
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

/// Non-generic: `pydantic.BaseModel`.
/// Generic: `pydantic.BaseModel, typing.Generic[T, …]`.
fn render_class_bases(generic_params: &[String]) -> String {
    if generic_params.is_empty() {
        "pydantic.BaseModel".to_string()
    } else {
        format!(
            "pydantic.BaseModel, typing.Generic[{}]",
            generic_params.join(", ")
        )
    }
}

/// If `c` is one of the four `baml.media.{Image,Video,Audio,Pdf}` stdlib
/// classes, return the corresponding Rust `PyO3` class name (`BamlImage`,
/// `BamlVideo`, …). 15b §lines 14-19 specify these as re-exports of
/// `PyO3` types holding `Arc<MediaValue>` directly; the regular Pydantic
/// shell is suppressed for them. Hardcoded list rather than an IR
/// flag — out of scope per 15d.
fn media_reexport_rust_name(c: &crate::emit::class::PyClass) -> Option<&'static str> {
    match c.source.to_string().as_str() {
        "baml.media.Image" => Some("BamlImage"),
        "baml.media.Video" => Some("BamlVideo"),
        "baml.media.Audio" => Some("BamlAudio"),
        "baml.media.Pdf" => Some("BamlPdf"),
        _ => None,
    }
}

fn is_media_reexport(s: &EmittedSymbol) -> bool {
    match s {
        EmittedSymbol::Class(c) => media_reexport_rust_name(c).is_some(),
        _ => false,
    }
}

// Within a method block, sync/async/companion fan-out of one source
// method renders contiguously (`tight_to_prev`); distinct source
// methods get a blank line between them.
#[derive(askama::Template)]
#[template(
    source = r#"class {{ py_name }}({{ bases }}):
    model_config = pydantic.ConfigDict(extra="forbid")
{%- for prop in properties %}
    {{ prop.name }}: {{ prop.ty_py }}
{%- endfor %}
{%- if !static_methods.is_empty() %}

{%- for m in static_methods %}
{%- if !loop.first && !m.tight_to_prev %}

{%- endif %}
{{ m.line }}
{%- endfor %}
{%- endif %}
{%- if !instance_methods.is_empty() %}

{%- for m in instance_methods %}
{%- if !loop.first && !m.tight_to_prev %}

{%- endif %}
{{ m.line }}
{%- endfor %}
{%- endif %}"#,
    ext = "py.j2",
    escape = "none"
)]
struct ClassBodyPy {
    py_name: String,
    bases: String,
    properties: Vec<ClassPropertyView>,
    static_methods: Vec<MethodLineView>,
    instance_methods: Vec<MethodLineView>,
}

struct ClassPropertyView {
    name: String,
    ty_py: String,
}

struct MethodLineView {
    line: String,
    tight_to_prev: bool,
}

// BAML forbids empty enums at parse time, so the `pass` branch below
// is purely defensive — emitted only if the IR somehow produces one.
#[derive(askama::Template)]
#[template(
    source = r#"class {{ py_name }}(str, enum.Enum):
{%- if variants.is_empty() %}
    pass
{%- else %}
{%- for v in variants %}
    {{ v.ident }} = {{ v.value }}
{%- endfor %}
{%- endif %}"#,
    ext = "py.j2",
    escape = "none"
)]
struct EnumBodyPy {
    py_name: String,
    variants: Vec<EnumVariantView>,
}

struct EnumVariantView {
    ident: String,
    value: String,
}

#[derive(askama::Template)]
#[template(
    source = "{{ py_name }}: typing.TypeAlias = {{ rhs }}",
    ext = "py.j2",
    escape = "none"
)]
struct TypeAliasPy {
    py_name: String,
    rhs: String,
}

/// `tight_to_prev` is true when this method shares its
/// `source_method_root` with the previous one (sync/async/companion
/// fan-out). The first method also gets `true` — the template emits
/// the leading blank line unconditionally.
fn build_method_line_views(methods: &[PyMethodBinding]) -> Vec<MethodLineView> {
    let mut out = Vec::with_capacity(methods.len());
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = source_method_root(&m.baml_fqn);
        let tight_to_prev = match prev_root {
            None => true,
            Some(p) => p == root,
        };
        out.push(MethodLineView {
            line: render_method_binding(m),
            tight_to_prev,
        });
        prev_root = Some(root);
    }
    out
}

/// Render one symbol into its `.py` source block, including trailing `\n`.
fn render_symbol(s: &EmittedSymbol, leaf: &LeafPath) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = media_reexport_rust_name(c) {
                return format!(
                    "from baml.baml_core.baml_py import {rust_name} as {py_name}\n",
                    py_name = c.py_name,
                );
            }
            let properties = c
                .properties
                .iter()
                .map(|prop| ClassPropertyView {
                    name: prop.name.clone(),
                    ty_py: translate_ty(&prop.ty, &ctx),
                })
                .collect();
            let mut out = ClassBodyPy {
                py_name: c.py_name.clone(),
                bases: render_class_bases(&c.generic_params),
                properties,
                static_methods: build_method_line_views(&c.static_methods),
                instance_methods: build_method_line_views(&c.instance_methods),
            }
            .render()
            .expect("class_body template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::Enum(e) => {
            let variants = e
                .variants
                .iter()
                .map(|v| EnumVariantView {
                    ident: v.ident.clone(),
                    value: py_string(&v.value),
                })
                .collect();
            let mut out = EnumBodyPy {
                py_name: e.py_name.clone(),
                variants,
            }
            .render()
            .expect("enum_body template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::TypeAlias(a) => {
            let rhs = translate_ty(&a.resolves_to, &ctx);
            let rhs = if a.recursive {
                // Wrap the entire RHS in single quotes so Pydantic
                // defers resolution to `model_rebuild` time.
                format!("'{rhs}'")
            } else {
                rhs
            };
            let mut out = TypeAliasPy {
                py_name: a.py_name.clone(),
                rhs,
            }
            .render()
            .expect("type_alias template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::Function(f) => {
            let mut out = render_factory_binding(f);
            out.push('\n');
            out
        }
    }
}

/// FQN prefix up to the first `$`. Sync/async pairs and companions
/// of one source method all share this prefix (companion FQN is
/// `<parent_fqn>$<suffix>`).
fn source_method_root(fqn: &str) -> &str {
    match fqn.find('$') {
        Some(i) => &fqn[..i],
        None => fqn,
    }
}

/// One factory-binding line. Sync and async siblings emit independently
/// but pad to align columns:
///
/// ```text
/// foo       = _define_function("<fqn>", "sync",  [<params>])
/// foo_async = _define_function("<fqn>", "async", [<params>])
/// ```
fn render_factory_binding(f: &crate::emit::function::PyFunction) -> String {
    // 6 = len("_async"): sync LHS aligns with async sibling's `=`
    // without inspecting the sibling. Mode literal padded so
    // `[<params>]` starts at the same column for both modes.
    let (lhs_pad, mode_str) = match f.mode {
        SyncAsync::Sync => ("      ", "\"sync\", "),
        SyncAsync::Async => ("", "\"async\","),
    };
    let params = render_param_list(&f.param_names);
    format!(
        "{name}{lhs_pad} = _define_function({fqn}, {mode_str} {params})",
        name = f.py_name,
        fqn = py_string(&f.baml_fqn),
    )
}

/// One method-binding line, indented for a class body. Mirrors
/// `render_factory_binding` modulo the factory alias and the
/// `staticmethod(...)` wrap on statics.
fn render_method_binding(m: &PyMethodBinding) -> String {
    let (lhs_pad, mode_str) = match m.mode {
        SyncAsync::Sync => ("      ", "\"sync\", "),
        SyncAsync::Async => ("", "\"async\","),
    };
    let params = render_param_list(&m.param_names);
    let factory = match m.kind {
        MethodKind::Static => "_define_static_method",
        MethodKind::Instance => "_define_instance_method",
    };
    let inner = format!(
        "{factory}({fqn}, {mode_str} {params})",
        fqn = py_string(&m.baml_fqn),
    );
    // `staticmethod(...)` wrap stops Python's descriptor protocol from
    // injecting the class as positional arg 0. Instance methods use
    // the descriptor protocol and don't need a wrap.
    let rhs = match m.kind {
        MethodKind::Static => format!("staticmethod({inner})"),
        MethodKind::Instance => inner,
    };
    format!("    {name}{lhs_pad} = {rhs}", name = m.py_name)
}

fn render_param_list(names: &[String]) -> String {
    if names.is_empty() {
        return "[]".to_string();
    }
    let mut s = String::from("[");
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&py_string(n));
    }
    s.push(']');
    s
}

/// Leaf body: imports + symbol bodies + `__all__`. Empty string when
/// the leaf has no symbols.
///
/// ```text
/// [blank]
/// import enum     (if any PyEnum)
/// import typing   (if any PyClass or PyTypeAlias)
/// import pydantic (if any PyClass)
/// [blank × 2]
/// <symbol body>
/// [blank × 2 between top-level groups; 0 between function fan-out siblings]
/// <symbol body>
/// [blank × 2]
/// __all__ = [
///     "X",
///     ...
/// ]
/// ```
pub(crate) fn render_leaf_body(body: &LeafBody) -> String {
    if body.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    let mut stdlibs = body.stdlib_imports();
    let cross_leaf_segments = body.cross_leaf_first_segments_py();
    // The cross-leaf block uses `typing.TYPE_CHECKING`, so even a
    // function-only leaf with a cross-leaf parameter still needs
    // `typing`. Append (not insert) — "typing" sorts after "enum".
    if !cross_leaf_segments.is_empty() && !stdlibs.contains(&"typing") {
        stdlibs.push("typing");
    }
    let needs_pydantic = body.needs_pydantic();
    let needs_factory = body.needs_define_function();
    let has_stdlib_block = !stdlibs.is_empty() || needs_pydantic;
    if has_stdlib_block {
        out.push('\n');
        for lib in &stdlibs {
            writeln!(out, "import {lib}").unwrap();
        }
        if needs_pydantic {
            out.push_str("import pydantic\n");
        }
    }
    // Cross-leaf imports go between the stdlib block and the factory
    // imports. `baml` is lifted to a runtime import — Pydantic v2
    // resolves field annotations like `baml.media.Pdf` against the
    // module's runtime globals at model-construction time, so the
    // `TYPE_CHECKING` guard isn't enough. `baml/*` is stdlib (only
    // ever imports from `baml.baml_core`) and never references user
    // leaves, so the runtime import can't cycle.
    //
    // All other first-segments stay under `if typing.TYPE_CHECKING:`
    // so recursive cross-leaf references (leaf A → leaf B → leaf A)
    // don't create an import cycle. `from __future__ import annotations`
    // (in every header) makes annotations resolve lazily as strings.
    //
    // Dot count = depth + 1: anchors at the `baml_sdk/` root
    // regardless of the installed package name.
    let dots = ".".repeat(body.leaf.segments.len() + 1);
    let (runtime_segments, type_checking_segments): (Vec<&String>, Vec<&String>) =
        cross_leaf_segments
            .iter()
            .partition(|seg| seg.as_str() == "baml");
    if !runtime_segments.is_empty() {
        out.push('\n');
        for seg in &runtime_segments {
            writeln!(out, "from {dots} import {seg}").unwrap();
        }
    }
    if !type_checking_segments.is_empty() {
        out.push('\n');
        out.push_str("if typing.TYPE_CHECKING:\n");
        for seg in &type_checking_segments {
            writeln!(out, "    from {dots} import {seg}").unwrap();
        }
    }
    // Factory imports use absolute paths (`baml.baml_core` is a
    // separate installed package, not reachable from this SDK tree)
    // with a `_` alias to keep them private to the module.
    let needs_static_method = body.needs_define_static_method();
    let needs_instance_method = body.needs_define_instance_method();
    let mut factories: Vec<&'static str> = Vec::new();
    if needs_factory {
        factories.push("define_function");
    }
    if needs_instance_method {
        factories.push("define_instance_method");
    }
    if needs_static_method {
        factories.push("define_static_method");
    }
    // The push order above is already alphabetic: `define_function` <
    // `define_instance_method` < `define_static_method`.
    if !factories.is_empty() {
        out.push('\n');
        if factories.len() == 1 {
            let original = factories[0];
            writeln!(out, "from baml.baml_core import {original} as _{original}").unwrap();
        } else {
            out.push_str("from baml.baml_core import (\n");
            for original in &factories {
                writeln!(out, "    {original} as _{original},").unwrap();
            }
            out.push_str(")\n");
        }
    }

    let typevars = body.generic_typevars();
    if !typevars.is_empty() {
        out.push_str("\n\n");
        for tv in &typevars {
            writeln!(out, "{tv} = typing.TypeVar(\"{tv}\")").unwrap();
        }
    }

    out.push_str("\n\n");

    let mut prev: Option<(&SortKey, &EmittedSymbol)> = None;
    for (sym, key) in &body.symbols {
        let body_text = render_symbol(sym, &body.leaf);
        if body_text.is_empty() {
            continue;
        }
        match prev {
            None => {}
            // Function fan-out siblings share their parent's sort key
            // and render contiguously.
            Some((p, prev_sym))
                if p == key
                    && matches!(prev_sym, EmittedSymbol::Function(_))
                    && matches!(sym, EmittedSymbol::Function(_)) => {}
            // Adjacent media re-export imports collapse into a single
            // import block with no blank between them.
            Some((_, prev_sym)) if is_media_reexport(prev_sym) && is_media_reexport(sym) => {}
            Some(_) => out.push_str("\n\n"),
        }
        out.push_str(&body_text);
        prev = Some((key, sym));
    }

    let names = body.all_names();
    if !names.is_empty() {
        out.push_str("\n\n");
        out.push_str("__all__ = [\n");
        for n in names {
            writeln!(out, "    \"{n}\",").unwrap();
        }
        out.push_str("]\n");
    }

    out
}

#[derive(askama::Template)]
#[template(
    source = r#"{%- if static_methods.is_empty() && instance_methods.is_empty() -%}
class {{ py_name }}({{ bases }}): ...
{%- else -%}
class {{ py_name }}({{ bases }}):
{%- for m in static_methods %}
{%- if !loop.first && !m.tight_to_prev %}

{%- endif %}
{{ m.block }}
{%- endfor %}
{%- if !static_methods.is_empty() && !instance_methods.is_empty() %}

{%- endif %}
{%- for m in instance_methods %}
{%- if !loop.first && !m.tight_to_prev %}

{%- endif %}
{{ m.block }}
{%- endfor %}
{%- endif %}"#,
    ext = "py.j2",
    escape = "none"
)]
struct ClassBodyPyi {
    py_name: String,
    bases: String,
    static_methods: Vec<MethodBlockView>,
    instance_methods: Vec<MethodBlockView>,
}

struct MethodBlockView {
    block: String,
    tight_to_prev: bool,
}

#[derive(askama::Template)]
#[template(
    source = "class {{ py_name }}(str, enum.Enum): ...",
    ext = "py.j2",
    escape = "none"
)]
struct EnumBodyPyi {
    py_name: String,
}

/// One method's `.pyi` signature block: a single `def` line for
/// instance methods, prefixed by `@staticmethod` for statics.
fn render_method_block_pyi(m: &PyMethodBinding, ctx: &TranslateCtx) -> String {
    let async_kw = if matches!(m.mode, SyncAsync::Async) {
        "async "
    } else {
        ""
    };
    let typed_params = render_method_params_pyi(m, ctx);
    let ret_py = translate_ty(&m.return_ty, ctx);
    let signature = format!(
        "    {async_kw}def {name}({typed_params}) -> {ret_py}: ...",
        name = m.py_name
    );
    match m.kind {
        MethodKind::Static => format!("    @staticmethod\n{signature}"),
        MethodKind::Instance => signature,
    }
}

/// `.pyi` counterpart of `build_method_line_views`.
fn build_method_block_views(
    methods: &[PyMethodBinding],
    ctx: &TranslateCtx,
) -> Vec<MethodBlockView> {
    let mut out = Vec::with_capacity(methods.len());
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = source_method_root(&m.baml_fqn);
        let tight_to_prev = match prev_root {
            None => true,
            Some(p) => p == root,
        };
        out.push(MethodBlockView {
            block: render_method_block_pyi(m, ctx),
            tight_to_prev,
        });
        prev_root = Some(root);
    }
    out
}

/// Render one symbol into its `.pyi` source block. Classes and enums
/// render name-only with `...`; type aliases mirror the `.py` shape;
/// functions render as typed `def`/`async def` signatures.
fn render_symbol_pyi(s: &EmittedSymbol, leaf: &LeafPath) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
            if let Some(rust_name) = media_reexport_rust_name(c) {
                return format!(
                    "from baml.baml_core.baml_py import {rust_name} as {py_name}\n",
                    py_name = c.py_name,
                );
            }
            let mut out = ClassBodyPyi {
                py_name: c.py_name.clone(),
                bases: render_class_bases(&c.generic_params),
                static_methods: build_method_block_views(&c.static_methods, &ctx),
                instance_methods: build_method_block_views(&c.instance_methods, &ctx),
            }
            .render()
            .expect("class_body.pyi template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::Enum(e) => {
            let mut out = EnumBodyPyi {
                py_name: e.py_name.clone(),
            }
            .render()
            .expect("enum_body.pyi template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::TypeAlias(a) => {
            // Type alias is identical between `.py` and `.pyi`.
            let rhs = translate_ty(&a.resolves_to, &ctx);
            let rhs = if a.recursive { format!("'{rhs}'") } else { rhs };
            let mut out = TypeAliasPy {
                py_name: a.py_name.clone(),
                rhs,
            }
            .render()
            .expect("type_alias template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::Function(f) => {
            let mut out = render_function_signature_pyi(f, &ctx);
            out.push('\n');
            out
        }
    }
}

/// `name: ty, …` for static methods; `self, name: ty, …` for instance
/// methods (`self` takes no annotation).
fn render_method_params_pyi(m: &PyMethodBinding, ctx: &TranslateCtx) -> String {
    match m.kind {
        MethodKind::Static => render_typed_params(&m.param_names, &m.arg_tys, ctx),
        MethodKind::Instance => {
            let mut s = String::from("self");
            for (n, t) in m.param_names.iter().skip(1).zip(m.arg_tys.iter()) {
                s.push_str(", ");
                s.push_str(n);
                s.push_str(": ");
                s.push_str(&translate_ty(t, ctx));
            }
            s
        }
    }
}

fn render_function_signature_pyi(f: &PyFunction, ctx: &TranslateCtx) -> String {
    let async_kw = if matches!(f.mode, SyncAsync::Async) {
        "async "
    } else {
        ""
    };
    let typed_params = render_typed_params(&f.param_names, &f.arg_tys, ctx);
    let ret_py = translate_ty(&f.return_ty, ctx);
    format!(
        "{async_kw}def {name}({typed_params}) -> {ret_py}: ...",
        name = f.py_name
    )
}

fn render_typed_params(names: &[String], tys: &[Ty], ctx: &TranslateCtx) -> String {
    let mut s = String::new();
    for (i, (n, t)) in names.iter().zip(tys.iter()).enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(n);
        s.push_str(": ");
        s.push_str(&translate_ty(t, ctx));
    }
    s
}

/// Mirrors `render_leaf_body` with these differences: no
/// `baml.baml_core` factory imports; `typing` is needed whenever a
/// signature is present (`needs_typing_pyi`); `enum` and `pydantic`
/// follow the `.py` rule.
pub(crate) fn render_leaf_body_pyi(body: &LeafBody) -> String {
    if body.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    let needs_enum = body
        .symbols
        .iter()
        .any(|(s, _)| matches!(s, EmittedSymbol::Enum(_)));
    let cross_leaf_segments = body.cross_leaf_first_segments_pyi();
    // The cross-leaf block uses `typing.TYPE_CHECKING`, so `typing`
    // must be in scope even if no signature would pull it in.
    let needs_typing = body.needs_typing_pyi() || !cross_leaf_segments.is_empty();
    let needs_pydantic = body.needs_pydantic();
    let has_stdlib_block = needs_enum || needs_typing || needs_pydantic;
    if has_stdlib_block {
        out.push('\n');
        if needs_enum {
            out.push_str("import enum\n");
        }
        if needs_typing {
            out.push_str("import typing\n");
        }
        if needs_pydantic {
            out.push_str("import pydantic\n");
        }
    }

    // Same cross-leaf block as `.py`. The guard is a no-op in `.pyi`
    // but kept for diffability.
    if !cross_leaf_segments.is_empty() {
        out.push('\n');
        out.push_str("if typing.TYPE_CHECKING:\n");
        let dots = ".".repeat(body.leaf.segments.len() + 1);
        for seg in &cross_leaf_segments {
            writeln!(out, "    from {dots} import {seg}").unwrap();
        }
    }

    // The `.pyi` re-declares TypeVars because stubs don't import from
    // sibling `.py` files.
    let typevars = body.generic_typevars();
    if !typevars.is_empty() {
        out.push_str("\n\n");
        for tv in &typevars {
            writeln!(out, "{tv} = typing.TypeVar(\"{tv}\")").unwrap();
        }
    }

    out.push_str("\n\n");

    let mut prev: Option<(&SortKey, &EmittedSymbol)> = None;
    for (sym, key) in &body.symbols {
        let body_text = render_symbol_pyi(sym, &body.leaf);
        if body_text.is_empty() {
            continue;
        }
        match prev {
            None => {}
            // Function fan-out siblings share their parent's sort key
            // and render contiguously.
            Some((p, prev_sym))
                if p == key
                    && matches!(prev_sym, EmittedSymbol::Function(_))
                    && matches!(sym, EmittedSymbol::Function(_)) => {}
            // Adjacent media re-export imports collapse into a single
            // import block (mirrors `.py`).
            Some((_, prev_sym)) if is_media_reexport(prev_sym) && is_media_reexport(sym) => {}
            Some(_) => out.push_str("\n\n"),
        }
        out.push_str(&body_text);
        prev = Some((key, sym));
    }

    let names = body.all_names();
    if !names.is_empty() {
        out.push_str("\n\n");
        out.push_str("__all__ = [\n");
        for n in names {
            writeln!(out, "    \"{n}\",").unwrap();
        }
        out.push_str("]\n");
    }

    out
}
