//! Per-leaf symbol bundle and leaf renderer. One `LeafBody` per
//! populated leaf file, pre-sorted at build time so the renderer is a
//! straight walk — no ordering logic at render time.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use baml_base::Literal;
use baml_codegen_types::{DefaultLiteral, FunctionArgumentDefault, Ty};

use crate::{
    emit::{
        EmittedSymbol, SortKey,
        function::{PyFunction, SyncAsync},
        method::{MethodKind, PyMethodBinding},
    },
    py_string,
    routing::{LeafPath, route_class_ref},
    translate_ty::{SelfRef, TranslateCtx, translate_ty},
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
        // import `typing` whenever a class or alias is present.
        if self
            .symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Class(_) | EmittedSymbol::TypeAlias(_)))
        {
            out.push("typing");
        }
        // Recursive aliases (18c) render via
        // `typing_extensions.TypeAliasType`.
        if self.has_recursive_alias() {
            out.push("typing_extensions");
        }
        out
    }

    /// True when any type alias in this leaf is flagged recursive.
    /// Drives the `import typing_extensions` line for both `.py` and
    /// `.pyi` since recursive aliases render via
    /// `typing_extensions.TypeAliasType` (18c).
    pub(crate) fn has_recursive_alias(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::TypeAlias(a) => a.recursive,
            _ => false,
        })
    }

    pub(crate) fn needs_pydantic(&self) -> bool {
        self.symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Class(_)))
    }

    /// True when this leaf needs the `_define_function` factory import —
    /// free functions, static methods, and instance methods all route
    /// through the same `define_function` factory (the
    /// `staticmethod(...)` wrap on statics is handled at the call site,
    /// not the factory).
    pub(crate) fn needs_define_function(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Function(_) => true,
            EmittedSymbol::Class(c) => {
                !c.static_methods.is_empty() || !c.instance_methods.is_empty()
            }
            _ => false,
        })
    }

    /// True when any class field, function/method param, or return type
    /// in this leaf is `Ty::RustType` — i.e. needs the
    /// `from baml_core import BamlPyHandle as _BamlPyHandle` line.
    pub(crate) fn needs_baml_pyhandle(&self) -> bool {
        fn ty_uses_rust_type(ty: &Ty) -> bool {
            match ty {
                Ty::RustType => true,
                Ty::Optional(inner) | Ty::List(inner) => ty_uses_rust_type(inner),
                Ty::Map { key, value } => ty_uses_rust_type(key) || ty_uses_rust_type(value),
                Ty::Union(items) => items.iter().any(ty_uses_rust_type),
                Ty::Class(_, args) => args.iter().any(ty_uses_rust_type),
                Ty::Callable { params, ret } => {
                    params.iter().any(|param| ty_uses_rust_type(&param.ty))
                        || ty_uses_rust_type(ret)
                }
                _ => false,
            }
        }
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => {
                c.properties.iter().any(|p| ty_uses_rust_type(&p.ty))
                    || c.static_methods.iter().any(|m| {
                        m.arg_tys.iter().any(ty_uses_rust_type) || ty_uses_rust_type(&m.return_ty)
                    })
                    || c.instance_methods.iter().any(|m| {
                        m.arg_tys.iter().any(ty_uses_rust_type) || ty_uses_rust_type(&m.return_ty)
                    })
            }
            EmittedSymbol::Function(f) => {
                f.arg_tys.iter().any(ty_uses_rust_type) || ty_uses_rust_type(&f.return_ty)
            }
            EmittedSymbol::TypeAlias(a) => ty_uses_rust_type(&a.resolves_to),
            EmittedSymbol::Enum(_) => false,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Root imports referenced by this leaf, in two buckets. Every
    /// import resolves through the SDK root (`baml_sdk/`); we never
    /// import a different namespace's submodule directly.
    ///
    /// Concretely, from a leaf at `baml_sdk/a/b/`, a reference to a
    /// type at `baml_sdk/c/d/` is brought in by importing `c` through
    /// the root (`from <root_dots> import c`); the translator then
    /// emits the dotted access `c.d.Symbol` and `c/__init__.py`'s own
    /// `from . import d` line takes care of loading `c.d`. References
    /// to root-namespace types (`baml_sdk/Foo`) are imported by name
    /// (`from <root_dots> import Foo`) and emitted bare.
    ///
    /// - `segments`: first segments of routed module paths (e.g.
    ///   `lorem`, `util`) — imported as `from <root_dots> import <seg>`.
    /// - `root_names`: bare names of types that live at the root leaf
    ///   itself and are referenced from a non-root leaf — imported as
    ///   `from <root_dots> import <name>`.
    ///
    /// Function/method types don't actually render annotations in the
    /// `.py` (factory bindings only), but they're walked anyway so the
    /// import block is identical across `.py` and `.pyi`. The
    /// `TYPE_CHECKING` guard makes the extras free at runtime.
    pub(crate) fn root_imports_py(&self) -> RootImports {
        let mut acc = RootImportSets::default();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    for prop in &c.properties {
                        collect_root_imports(&prop.ty, current, &mut acc);
                    }
                    for m in &c.static_methods {
                        for ty in &m.arg_tys {
                            collect_root_imports(ty, current, &mut acc);
                        }
                        collect_root_imports(&m.return_ty, current, &mut acc);
                    }
                    for m in &c.instance_methods {
                        for ty in &m.arg_tys {
                            collect_root_imports(ty, current, &mut acc);
                        }
                        collect_root_imports(&m.return_ty, current, &mut acc);
                    }
                }
                EmittedSymbol::Function(f) => {
                    for ty in &f.arg_tys {
                        collect_root_imports(ty, current, &mut acc);
                    }
                    collect_root_imports(&f.return_ty, current, &mut acc);
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_root_imports(&a.resolves_to, current, &mut acc);
                }
                EmittedSymbol::Enum(_) => {}
            }
        }
        acc.into_imports()
    }

    /// Field-edge cross-leaf refs, expressed as relative-anchored
    /// imports (25b2 Phase 4). These are reachable from positions
    /// that evaluate as Python expressions at module load time —
    /// Pydantic field annotations and type-alias RHS values — so
    /// they need unconditional imports once the eager root cascade
    /// is gone.
    ///
    /// The relative-anchored form (`from .. import <segment>` rather
    /// than `from <root_dots> import <first_segment>`) navigates only
    /// through fully-initialized intermediates: an intra-subtree ref
    /// like `stream_types/baml/llm` → `stream_types/baml/http` lands
    /// as `from .. import http`, avoiding the partial-attribute
    /// `AttributeError` that going through the SDK root would trigger
    /// during subpackage init.
    pub(crate) fn field_edge_rel_imports_py(&self) -> Vec<RelImport> {
        let mut acc = RootImportSets::default();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    for prop in &c.properties {
                        collect_root_imports(&prop.ty, current, &mut acc);
                    }
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_root_imports(&a.resolves_to, current, &mut acc);
                }
                EmittedSymbol::Function(_) | EmittedSymbol::Enum(_) => {}
            }
        }
        acc.into_rel()
    }

    /// All cross-leaf refs (classes, enums, aliases, function and
    /// method signatures), expressed as relative-anchored imports.
    /// Mirrors `root_imports_py`'s walk shape but returns the
    /// `RelImport` form used by the post-25b2 render path.
    pub(crate) fn all_rel_imports_py(&self) -> Vec<RelImport> {
        let mut acc = RootImportSets::default();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    for prop in &c.properties {
                        collect_root_imports(&prop.ty, current, &mut acc);
                    }
                    for m in &c.static_methods {
                        for ty in &m.arg_tys {
                            collect_root_imports(ty, current, &mut acc);
                        }
                        collect_root_imports(&m.return_ty, current, &mut acc);
                    }
                    for m in &c.instance_methods {
                        for ty in &m.arg_tys {
                            collect_root_imports(ty, current, &mut acc);
                        }
                        collect_root_imports(&m.return_ty, current, &mut acc);
                    }
                }
                EmittedSymbol::Function(f) => {
                    for ty in &f.arg_tys {
                        collect_root_imports(ty, current, &mut acc);
                    }
                    collect_root_imports(&f.return_ty, current, &mut acc);
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_root_imports(&a.resolves_to, current, &mut acc);
                }
                EmittedSymbol::Enum(_) => {}
            }
        }
        acc.into_rel()
    }

    /// Whether this leaf's `.pyi` needs `import typing`. Any rendered
    /// signature, type alias, or class pulls it in — class field types
    /// may resolve to `typing.Optional[…]` / `typing.List[…]` / etc.,
    /// and the generic base is `typing.Generic[…]`. Mirrors the `.py`
    /// "be generous" rule (`stdlib_imports`).
    pub(crate) fn needs_typing_pyi(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(_) | EmittedSymbol::Function(_) | EmittedSymbol::TypeAlias(_) => {
                true
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

/// Imports a leaf reaches for through the SDK root, finalized into
/// deterministic sorted vectors. Returned by
/// `LeafBody::root_imports_*`. Every import in this struct uses the
/// relative-path form `from <root_dots> import <name>` — we never
/// import a different namespace's submodule directly; everything goes
/// through the root.
pub(crate) struct RootImports {
    /// First segments of routed module paths used by this leaf —
    /// `from <root_dots> import <seg>`.
    pub(crate) segments: Vec<String>,
    /// Bare names of root-namespace types referenced by this non-root
    /// leaf — `from <root_dots> import <name>`. Empty for the root
    /// leaf itself; root types it owns need no import.
    pub(crate) root_names: Vec<String>,
}

/// A single cross-leaf reference, expressed as a Python relative-
/// import statement: `from <dots><from_path> import <anchor>`.
///
/// Every cross-leaf reference is anchored at the SDK root: codegen
/// imports the **first segment** of the routed leaf (or the bare type
/// name for root-namespace refs), and the translator emits the
/// fully-qualified dotted form `<first>.<rest>.<bare>` for the
/// annotation. Per-package cascades in every non-root `__init__.py`
/// (`from . import <each_child>`) bind every intermediate segment as
/// an attribute of its parent, so pyright resolves the dotted form
/// cleanly and the runtime lookup walks bound attributes the whole
/// way down.
///
/// `depth` is the dot count for the from-clause: 1 = same package as
/// the leaf (only relevant when current is the SDK root itself),
/// 2 = parent, 3 = grandparent, etc. `from_path` is always empty
/// for cross-leaf refs (we always escape to the SDK root and import
/// the top-level segment from there).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RelImport {
    pub(crate) depth: usize,
    pub(crate) from_path: String,
    pub(crate) anchor: String,
}

impl RelImport {
    /// Render to a Python `from … import …` source line.
    pub(crate) fn render(&self, indent: &str) -> String {
        let dots = ".".repeat(self.depth);
        if self.from_path.is_empty() {
            format!(
                "{indent}from {dots} import {anchor}\n",
                anchor = self.anchor
            )
        } else {
            format!(
                "{indent}from {dots}{path} import {anchor}\n",
                path = self.from_path,
                anchor = self.anchor,
            )
        }
    }
}

#[derive(Default)]
struct RootImportSets {
    segments: BTreeSet<String>,
    root_names: BTreeSet<String>,
    /// Relative-anchored imports keyed by (depth, anchor), deduped.
    /// Field-edge refs use this representation (25b2 Phase 4); each
    /// (depth, anchor) becomes one `from <depth_dots> import <anchor>`
    /// line at render time.
    rel: BTreeSet<RelImport>,
}

impl RootImportSets {
    fn into_imports(self) -> RootImports {
        RootImports {
            segments: self.segments.into_iter().collect(),
            root_names: self.root_names.into_iter().collect(),
        }
    }
    fn into_rel(self) -> Vec<RelImport> {
        self.rel.into_iter().collect()
    }
}

/// Walk a `Ty` and record every reference it carries that needs to be
/// imported through the SDK root:
///
/// - References whose routed leaf differs from `current` and has at
///   least one segment go into `segments` (first segment only — the
///   first-segment module is imported via the root, then the type is
///   accessed via the dotted form `<seg>.<rest>.Symbol`).
/// - References whose routed leaf is empty (root namespace) and
///   `current` is non-root go into `root_names` (bare type name — the
///   translator emits these references as the bare name and relies on
///   a `from <root_dots> import <Name>` line to bring it into scope).
/// - Same-leaf references are skipped — translator emits them as bare
///   names and the symbol is locally defined.
fn collect_root_imports(ty: &Ty, current: &LeafPath, out: &mut RootImportSets) {
    match ty {
        Ty::Class(name, args) => {
            record_name_routing(name, current, out);
            for a in args {
                collect_root_imports(a, current, out);
            }
        }
        Ty::Enum(name) | Ty::TypeAlias(name) => {
            record_name_routing(name, current, out);
        }
        Ty::Optional(inner) | Ty::List(inner) => collect_root_imports(inner, current, out),
        Ty::Map { key, value } => {
            collect_root_imports(key, current, out);
            collect_root_imports(value, current, out);
        }
        Ty::Union(items) => {
            for item in items {
                collect_root_imports(item, current, out);
            }
        }
        Ty::Callable { params, ret } => {
            for p in params {
                collect_root_imports(&p.ty, current, out);
            }
            collect_root_imports(ret, current, out);
        }
        // `Ty::Media(_)` renders as `baml.media.Image` etc. — the
        // routed leaf is `baml/media`, so codegen imports the top-level
        // segment `baml` from the SDK root via
        // `from <root_dots> import baml`. The per-package cascade in
        // `baml/__init__.py` (`from . import media, http, …`) binds
        // `media` as an attribute of `baml` for the dotted access.
        //
        // `Ty::RustType` renders as `_BamlPyHandle` and gets its own
        // `from baml_core import BamlPyHandle as _BamlPyHandle`
        // line via `needs_baml_pyhandle` — it does *not* go through
        // the cross-leaf segment set.
        Ty::Media(_) => {
            // Skip when current leaf IS `baml/media` (same-leaf ref).
            let target: &[&str] = &["baml", "media"];
            let target_eq = current.segments.len() == target.len()
                && current
                    .segments
                    .iter()
                    .zip(target.iter())
                    .all(|(a, b)| a.as_str() == *b);
            if target_eq {
                return;
            }
            out.segments.insert("baml".to_string());
            // Always anchor at the SDK root: escape `current.len()`
            // levels, then import the top-level segment `baml`.
            out.rel.insert(RelImport {
                depth: current.segments.len() + 1,
                from_path: String::new(),
                anchor: "baml".to_string(),
            });
        }
        Ty::Int
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Literal(_)
        | Ty::Uint8Array
        | Ty::TypeVar(_)
        | Ty::RustType
        | Ty::BuiltinUnknown
        | Ty::Unit
        | Ty::BamlOptions => {}
    }
}

fn record_name_routing(
    name: &baml_codegen_types::Name,
    current: &LeafPath,
    out: &mut RootImportSets,
) {
    let routed = route_class_ref(name);
    if routed == *current {
        return;
    }
    if routed.segments.is_empty() {
        // Root-routed type referenced from a non-root leaf — translator
        // emits the bare name (`Foo`), so we pull it in via
        // `from <dots> import Foo`. The root leaf itself never reaches
        // here (current is also empty there, so `routed == *current`).
        if !current.segments.is_empty() {
            out.root_names.insert(name.bare_name().to_string());
            out.rel.insert(RelImport {
                depth: current.segments.len() + 1,
                from_path: String::new(),
                anchor: name.bare_name().to_string(),
            });
        }
    } else {
        // Always import the top-level segment from the SDK root. The
        // per-package cascade in every non-root `__init__.py` binds the
        // deeper segments as attributes, so the translator's emission
        // `<first>.<rest>.<bare>` resolves through bound attribute
        // accesses all the way down. This shape is uniform regardless
        // of how the current leaf and the routed leaf are positioned
        // relative to each other.
        out.segments.insert(routed.segments[0].clone());
        out.rel.insert(RelImport {
            depth: current.segments.len() + 1,
            from_path: String::new(),
            anchor: routed.segments[0].clone(),
        });
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
    //
    // Then, with a second stable pass, hoist recursive aliases to the
    // very front of the leaf. Pyright recognizes a
    // `Name = typing_extensions.TypeAliasType("Name", …)` assignment
    // as a type alias only after the assignment line; consuming
    // classes whose field annotations name the alias must come *after*
    // the assignment or pyright reports
    // `reportInvalidTypeForm` on the alias's own self-reference (18c).
    let mut out: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
    for (leaf, mut pairs) in buckets {
        pairs.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| symbol_kind_ord(&a.0).cmp(&symbol_kind_ord(&b.0)))
        });
        // Stable hoist: recursive aliases first, everything else in
        // the order produced by the previous sort.
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

/// If `c` is one of the stdlib classes that codegen emits as a one-line
/// re-export, return the (module path, exported name) pair.
///
/// `baml.media.{Image,Video,Audio,Pdf}` (15b §lines 14-19): re-exports
/// of `PyO3` types holding `Arc<MediaValue>` directly — live in
/// `baml_core.baml_py` (the `PyO3` extension module).
///
/// `baml.llm.Stream`: pure-Python wrapper re-exported from `baml_core`
/// (`sdks/python/src/baml_core/_stream.py`). Lives outside the `PyO3`
/// module because nothing on the call path needed Rust — the args
/// encoder, runtime accessor, and result decoder are all already
/// exposed to Python.
fn media_reexport_rust_name(
    c: &crate::emit::class::PyClass,
) -> Option<(&'static str, &'static str)> {
    match c.source.to_string().as_str() {
        "baml.media.Image" => Some(("baml_core.baml_py", "BamlImage")),
        "baml.media.Video" => Some(("baml_core.baml_py", "BamlVideo")),
        "baml.media.Audio" => Some(("baml_core.baml_py", "BamlAudio")),
        "baml.media.Pdf" => Some(("baml_core.baml_py", "BamlPdf")),
        "baml.llm.Stream" => Some(("baml_core", "BamlStream")),
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
{%- if let Some(doc) = docstring %}
    {{ doc }}
{%- endif %}
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
    /// Pre-rendered `"""…"""` body docstring (class summary plus a
    /// folded `Attributes:` section listing each `///`-documented
    /// field). `None` when neither the class nor any field carries a
    /// `///`. Field-level `///` is intentionally not emitted as an
    /// inline `# …` comment — the `Attributes:` section is the sole
    /// channel.
    docstring: Option<String>,
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
{%- if let Some(doc) = docstring %}
    {{ doc }}
{%- endif %}
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
    /// Pre-rendered `"""…"""` body docstring (enum summary plus a
    /// folded `Members:` section listing each `///`-documented
    /// variant). `None` when neither the enum nor any variant carries
    /// a `///`.
    docstring: Option<String>,
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

/// Recursive aliases (18c): `Name = typing_extensions.TypeAliasType("Name", <RHS>)`.
/// `<RHS>` is the structural Python expression with self-references
/// emitted as `"Name"` forward-refs so the alias's home-module globals
/// resolve them at schema-build time.
#[derive(askama::Template)]
#[template(
    source = "{{ py_name }} = typing_extensions.TypeAliasType(\"{{ py_name }}\", {{ rhs }})",
    ext = "py.j2",
    escape = "none"
)]
struct TypeAliasTypePy {
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
        defer_name_refs: false,
    };

    match s {
        EmittedSymbol::Class(c) => {
            if let Some((module, rust_name)) = media_reexport_rust_name(c) {
                // 25b2 Phase 4: media re-export is now a pure import
                // line. The engine FQN lives only in `_TYPE_MAP`'s
                // reverse map (seeded with the PyO3 identity →
                // `baml.media.*` overrides in `baml_core/typemap.py`).
                return format!(
                    "from {module} import {rust_name} as {py_name}\n",
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
            let attrs: Vec<(String, Option<String>)> = c
                .properties
                .iter()
                .map(|p| (p.name.clone(), p.docstring.clone()))
                .collect();
            let docstring = crate::utils::format_class_docstring(
                c.docstring.as_deref(),
                &attrs,
                "Attributes",
                "    ",
            );
            let mut out = ClassBodyPy {
                py_name: c.py_name.clone(),
                bases: render_class_bases(&c.generic_params),
                docstring,
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
            let members: Vec<(String, Option<String>)> = e
                .variants
                .iter()
                .map(|v| (v.ident.clone(), v.docstring.clone()))
                .collect();
            let docstring = crate::utils::format_class_docstring(
                e.docstring.as_deref(),
                &members,
                "Members",
                "    ",
            );
            let mut out = EnumBodyPy {
                py_name: e.py_name.clone(),
                docstring,
                variants,
            }
            .render()
            .expect("enum_body template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::TypeAlias(a) => {
            let mut out = render_type_alias(a, leaf);
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

/// Render a type alias to its source line. Shared between `.py` and
/// `.pyi`; the body is identical (12d §3.3).
///
/// Non-recursive aliases render as `Name: typing.TypeAlias = <RHS>`.
/// Recursive aliases (18c) render via `typing_extensions.TypeAliasType`
/// with inner self-references quoted, so Pydantic resolves them
/// through its JSON-schema definitions machinery instead of recursing.
fn render_type_alias(a: &crate::emit::type_alias::PyTypeAlias, leaf: &LeafPath) -> String {
    use askama::Template;

    // Special-case the stdlib `baml.json.json` alias.  Its expanded form is
    // a recursive JSON-shaped union (`bool | int | float | str | List[json]
    // | Dict[str, json] | None`), and pyright reports the inner forward-refs
    // inside `TypeAliasType(...)` as `reportInvalidTypeForm` (the variable
    // doesn't exist yet at module-load time, even though pydantic resolves
    // it later).  Emitting `typing.Any` instead is precise enough for user
    // signatures that traffic in `json` values and avoids the lint.
    // TODO: replace with a proper recursive-alias representation once
    // pyright handles `TypeAliasType` forward-refs (or once we move json to
    // a stricter codegen surface).
    if a.source.pkg.as_str() == "baml"
        && a.source.namespace_path.len() == 1
        && a.source.namespace_path[0].as_str() == "json"
        && a.source.bare_name() == "json"
    {
        let py_name = &a.py_name;
        return format!("{py_name}: typing.TypeAlias = typing.Any\n");
    }

    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: if a.recursive {
            Some(SelfRef {
                routed_leaf: leaf.clone(),
                bare_name: a.source.bare_name().to_string(),
            })
        } else {
            None
        },
        // 18c: a recursive alias's RHS evaluates eagerly at module
        // load (it's the RHS of `TypeAliasType(...)`). Every named
        // reference — same-leaf, cross-leaf, root-routed — has to be
        // emitted as a forward-ref string so the line doesn't
        // `NameError`: same-leaf names may not yet be defined (the
        // alias is hoisted), and cross-leaf / root-routed names live
        // under `if typing.TYPE_CHECKING:` and aren't in runtime
        // globals. Pydantic resolves the strings later when it walks
        // the alias.
        defer_name_refs: a.recursive,
    };
    let rhs = translate_ty(&a.resolves_to, &ctx);
    if a.recursive {
        TypeAliasTypePy {
            py_name: a.py_name.clone(),
            rhs,
        }
        .render()
        .expect("type_alias_type template should always render")
    } else {
        TypeAliasPy {
            py_name: a.py_name.clone(),
            rhs,
        }
        .render()
        .expect("type_alias template should always render")
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
    let required_positional_count = required_positional_count(&f.arg_defaults, 0);
    let default_arg =
        render_required_positional_arg(required_positional_count, f.param_names.len());
    format!(
        "{name}{lhs_pad} = _define_function({fqn}, {mode_str} {params}{default_arg})",
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
    let receiver_count = usize::from(matches!(m.kind, MethodKind::Instance));
    let required_positional_count = required_positional_count(&m.arg_defaults, receiver_count);
    let default_arg =
        render_required_positional_arg(required_positional_count, m.param_names.len());
    let inner = format!(
        "_define_function({fqn}, {mode_str} {params}{default_arg})",
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

fn required_positional_count(
    arg_defaults: &[Option<FunctionArgumentDefault>],
    receiver_count: usize,
) -> usize {
    receiver_count
        + arg_defaults
            .iter()
            .take_while(|default| default.is_none())
            .count()
}

fn render_required_positional_arg(required: usize, total: usize) -> String {
    if required == total {
        String::new()
    } else {
        format!(", {required}")
    }
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
    let root_imports = body.root_imports_py();
    let root_segments = &root_imports.segments;
    let root_names = &root_imports.root_names;
    // The root-import block uses `typing.TYPE_CHECKING`, so even a
    // function-only leaf with a root-routed parameter still needs
    // `typing`. Append (not insert) — "typing" sorts after "enum".
    if (!root_segments.is_empty() || !root_names.is_empty()) && !stdlibs.contains(&"typing") {
        stdlibs.push("typing");
    }
    // Generic functions emit `T = typing.TypeVar("T")` lines below; the
    // `Class`/`TypeAlias` rule in `stdlib_imports` doesn't catch the
    // function-only-but-generic case (e.g. stdlib `baml.unstable.string<T>`).
    if !body.generic_typevars().is_empty() && !stdlibs.contains(&"typing") {
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
    // Root imports go between the stdlib block and the factory
    // imports. 25b2 Phase 4: field-edge refs (Pydantic field
    // annotations + type-alias RHS) emit as unconditional imports —
    // Pydantic v2 resolves field forward-refs against the module's
    // runtime globals at model-construction time, and type-alias RHS
    // values evaluate at module load. Both fail cleanly without an
    // unconditional import now that the eager root cascade is gone.
    //
    // Each import uses the most-direct relative path via the common
    // ancestor of current and routed (see `record_name_routing`):
    // intra-subtree refs land as `from .. import http`, avoiding the
    // partial-init AttributeError that going through the SDK root
    // would trigger during subpackage init. Cross-subtree refs naturally
    // anchor at the SDK root (common-prefix length 0).
    //
    // Method/function signatures live only in `.pyi` (runtime
    // factories don't carry annotations), so the legacy
    // `TYPE_CHECKING`-guarded block stays for those refs.
    let field_edge = body.field_edge_rel_imports_py();
    let field_edge_set: BTreeSet<RelImport> = field_edge.iter().cloned().collect();
    if !field_edge.is_empty() {
        out.push('\n');
        for r in &field_edge {
            out.push_str(&r.render(""));
        }
    }
    // TYPE_CHECKING block: refs from method/function signatures
    // (non-field-edge). Filter out anything already emitted as a
    // field-edge import. Key on the (depth, from_path, anchor) triple.
    let all_refs = body.all_rel_imports_py();
    let tc_only: Vec<&RelImport> = all_refs
        .iter()
        .filter(|r| !field_edge_set.contains(*r))
        .collect();
    if !tc_only.is_empty() {
        out.push('\n');
        out.push_str("if typing.TYPE_CHECKING:\n");
        for r in &tc_only {
            out.push_str(&r.render("    "));
        }
    }
    let _ = (root_segments, root_names);
    // Factory imports use absolute paths (`baml_core` is a
    // separate installed package, not reachable from this SDK tree)
    // with a `_` alias to keep them private to the module.
    //
    // `BamlPyHandle` shares the alias scheme: it's referenced as
    // `_BamlPyHandle` from translate_ty so the local relative `baml`
    // module (the SDK's own `baml.*` namespace) doesn't shadow the
    // installed runtime package.
    let needs_pyhandle = body.needs_baml_pyhandle();
    let mut runtime_imports: Vec<(&'static str, &'static str)> = Vec::new();
    if needs_pyhandle {
        runtime_imports.push(("BamlPyHandle", "_BamlPyHandle"));
    }
    if needs_factory {
        runtime_imports.push(("define_function", "_define_function"));
    }
    if !runtime_imports.is_empty() {
        out.push('\n');
        if runtime_imports.len() == 1 {
            let (original, alias) = runtime_imports[0];
            writeln!(out, "from baml_core import {original} as {alias}").unwrap();
        } else {
            out.push_str("from baml_core import (\n");
            for (original, alias) in &runtime_imports {
                writeln!(out, "    {original} as {alias},").unwrap();
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

    // 25b2 Phase 4: per-leaf `_register_*` trailers are gone. The
    // codegen-emitted `baml_sdk/_typemap.py` carries every FQN → leaf
    // lazy entry; `set_type_map(_TYPE_MAP)` in the root init installs
    // it. `BamlTypeMap.get_class(fqn)` resolves via importlib on first
    // lookup. Class bodies are pure Pydantic — no codegen metadata.

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
    source = r#"class {{ py_name }}({{ bases }}):
{%- if properties.is_empty() && static_methods.is_empty() && instance_methods.is_empty() %}
    ...
{%- endif %}
{%- for prop in properties %}
    {{ prop.name }}: {{ prop.ty_py }}
{%- endfor %}
{%- if !properties.is_empty() && (!static_methods.is_empty() || !instance_methods.is_empty()) %}

{%- endif %}
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
{%- endfor %}"#,
    ext = "py.j2",
    escape = "none"
)]
struct ClassBodyPyi {
    py_name: String,
    bases: String,
    properties: Vec<ClassPropertyView>,
    static_methods: Vec<MethodBlockView>,
    instance_methods: Vec<MethodBlockView>,
}

struct MethodBlockView {
    block: String,
    tight_to_prev: bool,
}

/// One method's `.pyi` signature block: a single `def` line for
/// instance methods, prefixed by `@staticmethod` for statics. When the
/// method carries a `///` docstring, replaces the trailing `...` with a
/// `"""..."""` body so `__doc__` resolves at runtime.
fn render_method_block_pyi(m: &PyMethodBinding, ctx: &TranslateCtx) -> String {
    let async_kw = if matches!(m.mode, SyncAsync::Async) {
        "async "
    } else {
        ""
    };
    let typed_params = render_method_params_pyi(m, ctx);
    let ret_py = translate_ty(&m.return_ty, ctx);
    let signature = match m.docstring.as_deref() {
        Some(doc) => {
            let rendered = crate::utils::format_docstring(doc, "        ");
            format!(
                "    {async_kw}def {name}({typed_params}) -> {ret_py}:\n        {rendered}",
                name = m.py_name
            )
        }
        None => format!(
            "    {async_kw}def {name}({typed_params}) -> {ret_py}: ...",
            name = m.py_name
        ),
    };
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

/// Render one symbol into its `.pyi` source block. Classes carry
/// typed field declarations and method signature stubs; enums carry
/// their variant lines verbatim; type aliases mirror the `.py` shape;
/// functions render as typed `def`/`async def` signatures. An empty
/// class (no fields, no methods) collapses to `class Foo(...): ...`.
fn render_symbol_pyi(s: &EmittedSymbol, leaf: &LeafPath) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
    };

    match s {
        EmittedSymbol::Class(c) => {
            if let Some((module, rust_name)) = media_reexport_rust_name(c) {
                return format!(
                    "from {module} import {rust_name} as {py_name}\n",
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
            let mut out = ClassBodyPyi {
                py_name: c.py_name.clone(),
                bases: render_class_bases(&c.generic_params),
                properties,
                static_methods: build_method_block_views(&c.static_methods, &ctx),
                instance_methods: build_method_block_views(&c.instance_methods, &ctx),
            }
            .render()
            .expect("class_body.pyi template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::Enum(e) => {
            // Enum body is identical between `.py` and `.pyi` —
            // reuse the `.py` template directly. The `.pyi` form
            // omits the body docstring; `__doc__` resolves against
            // the `.py` definition.
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
                docstring: None,
                variants,
            }
            .render()
            .expect("enum_body template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::TypeAlias(a) => {
            // Type alias is identical between `.py` and `.pyi`.
            let mut out = render_type_alias(a, leaf);
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
        MethodKind::Static => render_typed_params(&m.param_names, &m.arg_tys, &m.arg_defaults, ctx),
        MethodKind::Instance => {
            let mut s = String::from("self");
            let mut inserted_kw_marker = false;
            for ((n, t), default) in m
                .param_names
                .iter()
                .skip(1)
                .zip(m.arg_tys.iter())
                .zip(m.arg_defaults.iter())
            {
                if default.is_some() && !inserted_kw_marker {
                    s.push_str(", *");
                    inserted_kw_marker = true;
                }
                s.push_str(", ");
                s.push_str(&render_param_pyi(n, t, default.as_ref(), ctx));
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
    let typed_params = render_typed_params(&f.param_names, &f.arg_tys, &f.arg_defaults, ctx);
    let ret_py = translate_ty(&f.return_ty, ctx);
    match f.docstring.as_deref() {
        Some(doc) => {
            let rendered = crate::utils::format_docstring(doc, "    ");
            format!(
                "{async_kw}def {name}({typed_params}) -> {ret_py}:\n    {rendered}",
                name = f.py_name
            )
        }
        None => format!(
            "{async_kw}def {name}({typed_params}) -> {ret_py}: ...",
            name = f.py_name
        ),
    }
}

fn render_typed_params(
    names: &[String],
    tys: &[Ty],
    defaults: &[Option<FunctionArgumentDefault>],
    ctx: &TranslateCtx,
) -> String {
    let mut s = String::new();
    let mut inserted_kw_marker = false;
    for (i, ((n, t), default)) in names
        .iter()
        .zip(tys.iter())
        .zip(defaults.iter())
        .enumerate()
    {
        if default.is_some() && !inserted_kw_marker {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push('*');
            inserted_kw_marker = true;
        }
        if i > 0 || inserted_kw_marker {
            s.push_str(", ");
        }
        s.push_str(&render_param_pyi(n, t, default.as_ref(), ctx));
    }
    s
}

fn render_param_pyi(
    name: &str,
    ty: &Ty,
    default: Option<&FunctionArgumentDefault>,
    ctx: &TranslateCtx,
) -> String {
    let mut s = format!("{name}: {}", translate_ty(ty, ctx));
    if let Some(default) = default {
        s.push_str(" = ");
        s.push_str(&render_default_pyi(default));
    }
    s
}

fn render_default_pyi(default: &FunctionArgumentDefault) -> String {
    match default {
        FunctionArgumentDefault::Null => "None".to_string(),
        FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(lit)) => {
            render_literal_default(lit)
        }
        FunctionArgumentDefault::Literal(DefaultLiteral::EmptyList) => "[]".to_string(),
        FunctionArgumentDefault::Literal(DefaultLiteral::EmptyMap) => "{}".to_string(),
        FunctionArgumentDefault::Expression { .. } => "...".to_string(),
    }
}

fn render_literal_default(lit: &Literal) -> String {
    match lit {
        Literal::Int(value) => value.to_string(),
        Literal::Float(value) => value.clone(),
        Literal::String(value) => py_string(value),
        Literal::Bool(true) => "True".to_string(),
        Literal::Bool(false) => "False".to_string(),
    }
}

/// Mirrors `render_leaf_body` with these differences: no
/// `baml_core` factory imports; `typing` is needed whenever a
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
    // 25b2 Phase 4: `.pyi` uses the same relative-anchored imports as
    // `.py`, but wraps all of them under `if typing.TYPE_CHECKING:` —
    // the stub doesn't run at runtime, so the guard is a no-op for
    // type checkers and keeps the stub minimal.
    let rel_imports = body.all_rel_imports_py();
    let needs_typing = body.needs_typing_pyi() || !rel_imports.is_empty();
    let needs_typing_extensions = body.has_recursive_alias();
    let needs_pydantic = body.needs_pydantic();
    let has_stdlib_block = needs_enum || needs_typing || needs_typing_extensions || needs_pydantic;
    if has_stdlib_block {
        out.push('\n');
        if needs_enum {
            out.push_str("import enum\n");
        }
        if needs_typing {
            out.push_str("import typing\n");
        }
        if needs_typing_extensions {
            out.push_str("import typing_extensions\n");
        }
        if needs_pydantic {
            out.push_str("import pydantic\n");
        }
    }

    if !rel_imports.is_empty() {
        out.push('\n');
        out.push_str("if typing.TYPE_CHECKING:\n");
        for r in &rel_imports {
            out.push_str(&r.render("    "));
        }
    }

    // `_BamlPyHandle` alias mirrors the `.py` import so type checkers
    // can resolve `$rust_type` field annotations.
    if body.needs_baml_pyhandle() {
        out.push('\n');
        out.push_str("from baml_core import BamlPyHandle as _BamlPyHandle\n");
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
