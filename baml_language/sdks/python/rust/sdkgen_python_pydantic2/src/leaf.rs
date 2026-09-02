//! Per-leaf symbol bundle and leaf renderer. One `LeafBody` per
//! populated leaf file, pre-sorted at build time so the renderer is a
//! straight walk — no ordering logic at render time.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    rc::Rc,
};

use baml_base::{
    Literal,
    qualified_name::{AI_FUNCTION_SPEC, AI_STREAM_STREAM},
};
use baml_codegen_types::{DefaultLiteral, FunctionArgumentDefault, Ty};
use indexmap::IndexMap;

use crate::{
    emit::{
        EmittedSymbol, SortKey,
        function::{PyFunction, SyncAsync},
        method::{MethodKind, PyMethodBinding},
    },
    names::PythonNames,
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
    pub(crate) names: Option<Rc<PythonNames>>,
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

    pub(crate) fn callable_child_names(&self, kids: &BTreeSet<String>) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (sym, _) in &self.symbols {
            let EmittedSymbol::Function(f) = sym else {
                continue;
            };
            if f.mode == SyncAsync::Sync && kids.contains(&f.py_name) {
                out.insert(f.py_name.clone());
            }
        }
        out
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
        if self.has_recursive_alias()
            || self
                .symbols
                .iter()
                .any(|(symbol, _)| matches!(symbol, EmittedSymbol::Class(_)))
        {
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
    /// `from baml_bridge import BamlPyHandle as _BamlPyHandle` line.
    pub(crate) fn needs_baml_pyhandle(&self) -> bool {
        fn ty_uses_rust_type(ty: &Ty) -> bool {
            match ty {
                Ty::RustType { .. } => true,
                Ty::List(inner, _) => ty_uses_rust_type(inner),
                Ty::Map { key, value, .. } => ty_uses_rust_type(key) || ty_uses_rust_type(value),
                Ty::Union(items, _) => items.iter().any(ty_uses_rust_type),
                Ty::Class(_, args, _) => args.iter().any(ty_uses_rust_type),
                Ty::Interface(_, generics, associated_types, _) => {
                    generics.iter().any(ty_uses_rust_type)
                        || associated_types.iter().any(|(_, ty)| ty_uses_rust_type(ty))
                }
                Ty::Function {
                    params,
                    ret,
                    throws,
                    ..
                } => {
                    params.iter().any(|param| ty_uses_rust_type(&param.ty))
                        || ty_uses_rust_type(ret)
                        || ty_uses_rust_type(throws)
                }
                Ty::Future(value, error, _) => ty_uses_rust_type(value) || ty_uses_rust_type(error),
                _ => false,
            }
        }
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => {
                c.properties.iter().any(|p| ty_uses_rust_type(&p.ty))
                    || c.static_methods.iter().any(|m| {
                        m.required_args.iter().any(|arg| ty_uses_rust_type(&arg.ty))
                            || m.optional_args.iter().any(|arg| ty_uses_rust_type(&arg.ty))
                            || ty_uses_rust_type(&m.return_ty)
                    })
                    || c.instance_methods.iter().any(|m| {
                        m.required_args.iter().any(|arg| ty_uses_rust_type(&arg.ty))
                            || m.optional_args.iter().any(|arg| ty_uses_rust_type(&arg.ty))
                            || ty_uses_rust_type(&m.return_ty)
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

    /// The optional-argument `Ty::Function`s reachable from this leaf's
    /// function/method/field signatures, in deterministic first-seen order,
    /// each paired with the `_<owner>__<param>` prefix for its Protocol name
    /// (`render_leaf_body_pyi` appends a per-prefix counter, giving e.g.
    /// `_call_optional_int_callback_supplied__callback1`). Each is rendered as a
    /// named `typing.Protocol` in the `.pyi` (see `render_callback_protocol`),
    /// because a `typing.Callable[[…], R]` type can't express per-parameter
    /// optionality.
    ///
    /// Type aliases are skipped: they render via the shared `.py`/`.pyi`
    /// `render_type_alias` (which has no Protocol map), so a callable alias
    /// falls back to `typing.Callable[..., R]` rather than referencing a
    /// Protocol — collecting one here would emit an unused class.
    pub(crate) fn callback_protocols(&self) -> Vec<(Ty, String)> {
        let mut seen: std::collections::HashSet<Ty> = std::collections::HashSet::new();
        let mut out: Vec<(Ty, String)> = Vec::new();
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    for prop in &c.properties {
                        let base = format!("_{}__{}", c.py_name, prop.name);
                        collect_optional_callables(&prop.ty, &base, &mut seen, &mut out);
                    }
                    for m in c.static_methods.iter().chain(&c.instance_methods) {
                        // The projected method binding is already a valid,
                        // collision-free Python identifier.
                        let owner = &m.py_name;
                        for arg in &m.required_args {
                            let base = format!("_{owner}__{}", arg.name);
                            collect_optional_callables(&arg.ty, &base, &mut seen, &mut out);
                        }
                        for arg in &m.optional_args {
                            let base = format!("_{owner}__{}", arg.name);
                            collect_optional_callables(&arg.ty, &base, &mut seen, &mut out);
                        }
                        let base = format!("_{owner}__ret");
                        collect_optional_callables(&m.return_ty, &base, &mut seen, &mut out);
                    }
                }
                EmittedSymbol::Function(f) => {
                    // See the method case: reuse the projected public binding.
                    let owner = &f.py_name;
                    for (name, ty) in f.param_names.iter().zip(f.arg_tys.iter()) {
                        let base = format!("_{owner}__{name}");
                        collect_optional_callables(ty, &base, &mut seen, &mut out);
                    }
                    let base = format!("_{owner}__ret");
                    collect_optional_callables(&f.return_ty, &base, &mut seen, &mut out);
                }
                EmittedSymbol::TypeAlias(_) | EmittedSymbol::Enum(_) => {}
            }
        }
        out
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
                        collect_root_imports(&prop.ty, current, &mut acc, self.names.as_deref());
                    }
                    for m in &c.static_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        collect_root_imports(
                            &m.return_ty,
                            current,
                            &mut acc,
                            self.names.as_deref(),
                        );
                    }
                    for m in &c.instance_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        collect_root_imports(
                            &m.return_ty,
                            current,
                            &mut acc,
                            self.names.as_deref(),
                        );
                    }
                }
                EmittedSymbol::Function(f) => {
                    for ty in &f.arg_tys {
                        collect_root_imports(ty, current, &mut acc, self.names.as_deref());
                    }
                    collect_root_imports(&f.return_ty, current, &mut acc, self.names.as_deref());
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_root_imports(&a.resolves_to, current, &mut acc, self.names.as_deref());
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
                        collect_root_imports(&prop.ty, current, &mut acc, self.names.as_deref());
                    }
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_root_imports(&a.resolves_to, current, &mut acc, self.names.as_deref());
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
        self.all_rel_imports_from(&self.leaf)
    }

    /// Collect this body's signature imports as rendered from `current`.
    /// Callable-child protocols are emitted in their parent's stub, so their
    /// imports must be anchored at the parent rather than the child leaf.
    fn all_rel_imports_from(&self, current: &LeafPath) -> Vec<RelImport> {
        let mut acc = RootImportSets::default();
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    for prop in &c.properties {
                        collect_root_imports(&prop.ty, current, &mut acc, self.names.as_deref());
                    }
                    for m in &c.static_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        collect_root_imports(
                            &m.return_ty,
                            current,
                            &mut acc,
                            self.names.as_deref(),
                        );
                    }
                    for m in &c.instance_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc, self.names.as_deref());
                        }
                        collect_root_imports(
                            &m.return_ty,
                            current,
                            &mut acc,
                            self.names.as_deref(),
                        );
                    }
                }
                EmittedSymbol::Function(f) => {
                    for ty in &f.arg_tys {
                        collect_root_imports(ty, current, &mut acc, self.names.as_deref());
                    }
                    collect_root_imports(&f.return_ty, current, &mut acc, self.names.as_deref());
                }
                EmittedSymbol::TypeAlias(a) => {
                    collect_root_imports(&a.resolves_to, current, &mut acc, self.names.as_deref());
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

    pub(crate) fn has_defaulted_call_params(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Function(f) => f.arg_defaults.iter().any(Option::is_some),
            EmittedSymbol::Class(c) => c
                .static_methods
                .iter()
                .chain(&c.instance_methods)
                .any(|m| !m.optional_args.is_empty()),
            EmittedSymbol::Enum(_) | EmittedSymbol::TypeAlias(_) => false,
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

/// Recursively collect every optional-argument `Ty::Function` reachable from
/// `ty`, in first-seen order, skipping duplicates (`seen`). Each is paired with
/// `base` — the `_<owner>__<param>` prefix for its Protocol name (the final
/// name appends a per-base counter in `LeafBody::callback_protocols`). Children
/// are visited before the enclosing callable so nested callbacks get earlier
/// names.
fn collect_optional_callables(
    ty: &Ty,
    base: &str,
    seen: &mut std::collections::HashSet<Ty>,
    out: &mut Vec<(Ty, String)>,
) {
    match ty {
        Ty::List(inner, _) => collect_optional_callables(inner, base, seen, out),
        Ty::Map { key, value, .. } => {
            collect_optional_callables(key, base, seen, out);
            collect_optional_callables(value, base, seen, out);
        }
        Ty::Union(items, _) => {
            for item in items {
                collect_optional_callables(item, base, seen, out);
            }
        }
        Ty::Class(_, args, _) => {
            for a in args {
                collect_optional_callables(a, base, seen, out);
            }
        }
        Ty::Function { params, ret, .. } => {
            for p in params {
                collect_optional_callables(&p.ty, base, seen, out);
            }
            collect_optional_callables(ret, base, seen, out);
            let has_optional = params
                .iter()
                .any(|p| p.mode == baml_codegen_types::CodegenFunctionParamMode::Optional);
            if has_optional && seen.insert(ty.clone()) {
                out.push((ty.clone(), base.to_string()));
            }
        }
        _ => {}
    }
}

fn collect_root_imports(
    ty: &Ty,
    current: &LeafPath,
    out: &mut RootImportSets,
    names: Option<&PythonNames>,
) {
    match ty {
        Ty::Class(name, args, _) => {
            record_name_routing(name, current, out, names);
            for a in args {
                collect_root_imports(a, current, out, names);
            }
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) | Ty::TypeAlias(name, _) => {
            record_name_routing(name, current, out, names);
        }
        Ty::List(inner, _) => collect_root_imports(inner, current, out, names),
        Ty::Map { key, value, .. } => {
            collect_root_imports(key, current, out, names);
            collect_root_imports(value, current, out, names);
        }
        Ty::Union(items, _) => {
            for item in items {
                collect_root_imports(item, current, out, names);
            }
        }
        Ty::Function { params, ret, .. } => {
            for p in params {
                collect_root_imports(&p.ty, current, out, names);
            }
            collect_root_imports(ret, current, out, names);
        }
        // `Ty::Media(_)` renders as `baml.media.Image` etc. — the
        // routed leaf is `baml/media`, so codegen imports the top-level
        // segment `baml` from the SDK root via
        // `from <root_dots> import baml`. The per-package cascade in
        // `baml/__init__.py` (`from . import media, http, …`) binds
        // `media` as an attribute of `baml` for the dotted access.
        //
        // `Ty::RustType` renders as `_BamlPyHandle` and gets its own
        // `from baml_bridge import BamlPyHandle as _BamlPyHandle`
        // line via `needs_baml_pyhandle` — it does *not* go through
        // the cross-leaf segment set.
        Ty::Media(..) => {
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
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Literal(..)
        | Ty::Uint8Array { .. }
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Unknown { .. }
        | Ty::Never { .. }
        | Ty::Void { .. }
        | Ty::Interface(..)
        | Ty::Future(..) => {}
    }
}

fn record_name_routing(
    name: &baml_codegen_types::Name,
    current: &LeafPath,
    out: &mut RootImportSets,
    names: Option<&PythonNames>,
) {
    let routed = names.map_or_else(
        || route_class_ref(name),
        |names| names.route_class_ref(name),
    );
    if routed == *current {
        return;
    }
    if routed.segments.is_empty() {
        // Root-routed type referenced from a non-root leaf — translator
        // emits the bare name (`Foo`), so we pull it in via
        // `from <dots> import Foo`. The root leaf itself never reaches
        // here (current is also empty there, so `routed == *current`).
        if !current.segments.is_empty() {
            let projected = names.map_or_else(
                || name.bare_name().to_string(),
                |names| names.symbol(name).into_owned(),
            );
            out.root_names.insert(projected.clone());
            out.rel.insert(RelImport {
                depth: current.segments.len() + 1,
                from_path: String::new(),
                anchor: projected,
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

pub(crate) fn group_and_sort_with_names(
    triples: Vec<(LeafPath, EmittedSymbol, SortKey)>,
    names: &Rc<PythonNames>,
) -> BTreeMap<LeafPath, LeafBody> {
    group_and_sort_inner(triples, Some(names))
}

fn group_and_sort_inner(
    triples: Vec<(LeafPath, EmittedSymbol, SortKey)>,
    names: Option<&Rc<PythonNames>>,
) -> BTreeMap<LeafPath, LeafBody> {
    let mut buckets: BTreeMap<LeafPath, Vec<(EmittedSymbol, SortKey)>> = BTreeMap::new();
    for (leaf, sym, key) in triples {
        buckets.entry(leaf).or_default().push((sym, key));
    }

    // Stable sort preserves intra-parent function fan-out order (base
    // sync, base async, companions each sync/async).
    //
    // Recursive aliases are hoisted to the front of the leaf. Pyright
    // recognizes a
    // `Name = typing_extensions.TypeAliasType("Name", ...)` assignment as a
    // type alias only after the assignment line; consuming
    // classes whose field annotations name the alias must come *after*
    // the assignment or pyright reports
    // `reportInvalidTypeForm` on the alias's own self-reference (18c).
    //
    // Non-recursive aliases are emitted after every other symbol because
    // their right-hand sides evaluate eagerly. Same-leaf dependencies are
    // topologically ordered within both alias phases so forward chains
    // evaluate safely; recursive strongly connected components stay stable.
    //
    // Exception: an alias whose RHS names nothing from this leaf's own
    // package is HOISTED above the classes. A class body runs pydantic's
    // eager annotation resolution, which imports the sibling leaves its
    // fields name; those leaves may in turn name an alias back in THIS
    // leaf, and the partially-initialized module would not have it yet
    // (`ai.content.Media.media: ai.MediaPart`, reached while
    // `ai/__init__.py` is still executing `class Journal`). Hoisting is
    // safe only for aliases that cannot themselves start such a cascade —
    // hence "nothing from this leaf's own package".
    let mut out: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
    for (leaf, mut pairs) in buckets {
        pairs.sort_by(|a, b| a.1.cmp(&b.1));

        let mut recursive_aliases = Vec::new();
        let mut other_symbols = Vec::new();
        let mut non_recursive_aliases = Vec::new();
        for pair in pairs {
            match &pair.0 {
                EmittedSymbol::TypeAlias(a) if a.recursive => recursive_aliases.push(pair),
                EmittedSymbol::TypeAlias(_) => non_recursive_aliases.push(pair),
                _ => other_symbols.push(pair),
            }
        }

        let mut symbols = Vec::with_capacity(
            recursive_aliases.len() + other_symbols.len() + non_recursive_aliases.len(),
        );
        let (hoisted_aliases, trailing_aliases) =
            split_hoistable_aliases(&leaf, non_recursive_aliases);
        symbols.extend(sort_aliases(recursive_aliases));
        symbols.extend(sort_aliases(hoisted_aliases));
        symbols.extend(other_symbols);
        symbols.extend(sort_aliases(trailing_aliases));
        out.insert(
            leaf.clone(),
            LeafBody {
                leaf,
                symbols,
                names: names.cloned(),
            },
        );
    }
    out
}

/// One leaf symbol paired with the key it renders in order by.
type SortedSymbol = (EmittedSymbol, SortKey);

/// Split the leaf's non-recursive aliases into the ones safe to emit
/// ABOVE the classes and the ones that must stay below them.
///
/// An alias is hoistable when every name its RHS mentions routes to a
/// leaf under a DIFFERENT top-level package than this leaf — plus
/// same-leaf aliases that are themselves hoistable. Anything naming this
/// leaf's own package (a same-leaf class, or a sibling leaf like
/// `ai.clients` from `ai`) can trigger an import that re-enters this
/// still-executing module, so it stays below the classes exactly as
/// before. The SDK root package (`LeafPath` with no segments) is never
/// hoisted across: every leaf imports root names eagerly.
fn split_hoistable_aliases(
    leaf: &LeafPath,
    aliases: Vec<SortedSymbol>,
) -> (Vec<SortedSymbol>, Vec<SortedSymbol>) {
    let alias_indices: BTreeMap<baml_codegen_types::Name, usize> = aliases
        .iter()
        .enumerate()
        .map(|(index, (symbol, _))| match symbol {
            EmittedSymbol::TypeAlias(alias) => (alias.source.clone(), index),
            _ => unreachable!("only type aliases are passed here"),
        })
        .collect();

    let mut hoistable = vec![true; aliases.len()];
    let mut same_leaf_deps: Vec<Vec<usize>> = vec![Vec::new(); aliases.len()];
    for (index, (symbol, _)) in aliases.iter().enumerate() {
        let EmittedSymbol::TypeAlias(alias) = symbol else {
            unreachable!("only type aliases are passed here");
        };
        let mut referenced = Vec::new();
        collect_ty_names(&alias.resolves_to, &mut referenced);
        for name in referenced {
            if let Some(dependency) = alias_indices.get(&name) {
                if *dependency != index {
                    same_leaf_deps[index].push(*dependency);
                }
            } else if !routes_outside_package(leaf, &name) {
                hoistable[index] = false;
            }
        }
    }

    // A hoisted alias may only depend on hoisted aliases.
    loop {
        let mut changed = false;
        for index in 0..aliases.len() {
            if hoistable[index] && same_leaf_deps[index].iter().any(|dep| !hoistable[*dep]) {
                hoistable[index] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut hoisted = Vec::new();
    let mut trailing = Vec::new();
    for (index, pair) in aliases.into_iter().enumerate() {
        if hoistable[index] {
            hoisted.push(pair);
        } else {
            trailing.push(pair);
        }
    }
    (hoisted, trailing)
}

/// Whether `name` lands in a leaf under a different logical package than
/// `leaf`. `stream_types` is a synthetic routing prefix, so compare the source
/// package beneath it (`ai`, `baml`, ...) rather than treating every partial
/// type as part of one giant package. The SDK root counts as nobody's outside.
pub(crate) fn routes_outside_package(leaf: &LeafPath, name: &baml_codegen_types::Name) -> bool {
    let routed = route_class_ref(name);
    match (
        logical_package_segment(&leaf.segments),
        logical_package_segment(&routed.segments),
    ) {
        (Some(current), Some(other)) => current != other,
        _ => false,
    }
}

fn logical_package_segment(segments: &[String]) -> Option<&str> {
    match segments {
        [prefix, package, ..] if prefix == "stream_types" => Some(package),
        [package, ..] => Some(package),
        [] => None,
    }
}

/// Every named symbol (class, interface, enum, alias) `ty` mentions.
fn collect_ty_names(ty: &Ty, out: &mut Vec<baml_codegen_types::Name>) {
    match ty {
        Ty::TypeAlias(name, _) | Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => {
            out.push(name.clone());
        }
        Ty::Class(name, arguments, _) => {
            out.push(name.clone());
            for argument in arguments {
                collect_ty_names(argument, out);
            }
        }
        Ty::Interface(name, arguments, associated, _) => {
            out.push(name.clone());
            for argument in arguments {
                collect_ty_names(argument, out);
            }
            for (_, assoc) in associated {
                collect_ty_names(assoc, out);
            }
        }
        Ty::List(inner, _) => collect_ty_names(inner, out),
        Ty::Future(value, error, _) => {
            collect_ty_names(value, out);
            collect_ty_names(error, out);
        }
        Ty::Map { key, value, .. } => {
            collect_ty_names(key, out);
            collect_ty_names(value, out);
        }
        Ty::Union(members, _) => {
            for member in members {
                collect_ty_names(member, out);
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                collect_ty_names(&param.ty, out);
            }
            collect_ty_names(ret, out);
            collect_ty_names(throws, out);
        }
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Literal(..)
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Unknown { .. }
        | Ty::Never { .. }
        | Ty::Void { .. } => {}
    }
}

fn sort_aliases(aliases: Vec<(EmittedSymbol, SortKey)>) -> Vec<(EmittedSymbol, SortKey)> {
    let alias_indices: BTreeMap<baml_codegen_types::Name, usize> = aliases
        .iter()
        .enumerate()
        .map(|(index, (symbol, _))| match symbol {
            EmittedSymbol::TypeAlias(alias) => (alias.source.clone(), index),
            _ => unreachable!("only type aliases are passed here"),
        })
        .collect();

    let mut dependencies = vec![BTreeSet::new(); aliases.len()];
    for (index, (symbol, _)) in aliases.iter().enumerate() {
        let EmittedSymbol::TypeAlias(alias) = symbol else {
            unreachable!("only type aliases are passed here");
        };
        collect_alias_dependencies(&alias.resolves_to, &alias_indices, &mut dependencies[index]);
        dependencies[index].remove(&index);
    }

    let mut in_degree: Vec<usize> = dependencies.iter().map(BTreeSet::len).collect();
    let mut dependents = vec![Vec::new(); aliases.len()];
    for (dependent, required) in dependencies.iter().enumerate() {
        for dependency in required {
            dependents[*dependency].push(dependent);
        }
    }

    let mut ready: BTreeSet<usize> = in_degree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect();
    let mut order = Vec::with_capacity(aliases.len());
    while let Some(index) = ready.iter().next().copied() {
        ready.remove(&index);
        order.push(index);
        for dependent in &dependents[index] {
            in_degree[*dependent] -= 1;
            if in_degree[*dependent] == 0 {
                ready.insert(*dependent);
            }
        }
    }

    // Preserve deterministic source order for strongly connected components,
    // which cannot be topologically separated.
    if order.len() != aliases.len() {
        order.extend(
            in_degree
                .iter()
                .enumerate()
                .filter_map(|(index, degree)| (*degree != 0).then_some(index)),
        );
    }

    let mut aliases: Vec<Option<(EmittedSymbol, SortKey)>> =
        aliases.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|index| aliases[index].take().expect("alias index is unique"))
        .collect()
}

fn collect_alias_dependencies(
    ty: &Ty,
    alias_indices: &BTreeMap<baml_codegen_types::Name, usize>,
    out: &mut BTreeSet<usize>,
) {
    match ty {
        Ty::TypeAlias(name, _) => {
            if let Some(index) = alias_indices.get(name) {
                out.insert(*index);
            }
        }
        Ty::Class(_, arguments, _) => {
            for argument in arguments {
                collect_alias_dependencies(argument, alias_indices, out);
            }
        }
        Ty::List(inner, _) => collect_alias_dependencies(inner, alias_indices, out),
        Ty::Map { key, value, .. } => {
            collect_alias_dependencies(key, alias_indices, out);
            collect_alias_dependencies(value, alias_indices, out);
        }
        Ty::Union(members, _) => {
            for member in members {
                collect_alias_dependencies(member, alias_indices, out);
            }
        }
        Ty::Function { params, ret, .. } => {
            for param in params {
                collect_alias_dependencies(&param.ty, alias_indices, out);
            }
            collect_alias_dependencies(ret, alias_indices, out);
        }
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Literal(..)
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Unknown { .. }
        | Ty::Never { .. }
        | Ty::Void { .. }
        | Ty::Interface(..)
        | Ty::Future(..) => {}
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
/// `baml_bridge.baml_py` (the `PyO3` extension module).
///
/// `ai.stream.Stream`: pure-Python wrapper re-exported from `baml_bridge`
/// (`sdks/python/src/baml_bridge/_stream.py`). Lives outside the `PyO3`
/// module because nothing on the call path needed Rust — the args
/// encoder, runtime accessor, and result decoder are all already
/// exposed to Python.
fn media_reexport_rust_name(
    c: &crate::emit::class::PyClass,
) -> Option<(&'static str, &'static str)> {
    match c.source.to_string().as_str() {
        "baml.media.Image" => Some(("baml_bridge.baml_py", "BamlImage")),
        "baml.media.Video" => Some(("baml_bridge.baml_py", "BamlVideo")),
        "baml.media.Audio" => Some(("baml_bridge.baml_py", "BamlAudio")),
        "baml.media.Pdf" => Some(("baml_bridge.baml_py", "BamlPdf")),
        AI_FUNCTION_SPEC => Some(("baml_bridge", "BamlFunctionSpec")),
        AI_STREAM_STREAM => Some(("baml_bridge", "BamlStream")),
        "reflect.Type" => Some(("baml_bridge.reflect", "Type")),
        _ => None,
    }
}

fn is_media_reexport(s: &EmittedSymbol) -> bool {
    match s {
        EmittedSymbol::Class(c) => media_reexport_rust_name(c).is_some(),
        _ => false,
    }
}

fn is_function_spec_reexport(s: &EmittedSymbol) -> bool {
    matches!(s, EmittedSymbol::Class(c) if c.source.to_string() == AI_FUNCTION_SPEC)
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
    model_config = pydantic.ConfigDict(
        arbitrary_types_allowed=True,
        extra="ignore",
        populate_by_name=True,
    )
{%- for prop in properties %}
    {{ prop.name }}: {{ prop.ty_py }}{{ prop.default_expr }}
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
    default_expr: String,
}

fn render_field_default(prop: &crate::emit::class::PyClassProperty) -> String {
    if prop.name != prop.wire_name {
        if prop.nullable {
            format!(
                " = pydantic.Field(default=None, alias={})",
                py_string(&prop.wire_name)
            )
        } else {
            format!(" = pydantic.Field(alias={})", py_string(&prop.wire_name))
        }
    } else if prop.nullable {
        " = None".to_string()
    } else {
        String::new()
    }
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
/// authored FQN with the previous one (direct/spec/stream fan-out).
/// The first method also gets `true` — the template emits
/// the leading blank line unconditionally.
fn build_method_line_views(
    methods: &[PyMethodBinding],
    class_py_name: &str,
    class_wire_generic_params: &[String],
) -> Vec<MethodLineView> {
    let mut out = Vec::with_capacity(methods.len());
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = m.baml_fqn.as_str();
        let tight_to_prev = match prev_root {
            None => true,
            Some(p) => p == root,
        };
        out.push(MethodLineView {
            line: render_method_binding(m, class_py_name, class_wire_generic_params),
            tight_to_prev,
        });
        prev_root = Some(root);
    }
    out
}

/// Render one symbol into its `.py` source block, including trailing `\n`.
fn render_symbol(s: &EmittedSymbol, leaf: &LeafPath, names: Option<Rc<PythonNames>>) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        // Runtime `.py`: callback Protocols are stub-only, so optional-arg
        // callables widen to `typing.Callable[..., R]` here.
        callback_protocols: None,
        type_stream_accessors: true,
        include_stream_done: false,
        names: names.clone(),
        type_var_names: BTreeMap::new(),
    };

    match s {
        EmittedSymbol::Class(c) => {
            let class_ctx = TranslateCtx {
                type_var_names: c.type_var_names.clone(),
                ..ctx
            };
            if let Some((module, rust_name)) = media_reexport_rust_name(c) {
                // 25b2 Phase 4: media re-export is now a pure import
                // line. The engine FQN lives only in `_TYPE_MAP`'s
                // reverse map (seeded with the PyO3 identity →
                // `baml.media.*` overrides in `baml_bridge/typemap.py`).
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
                    ty_py: translate_ty(&prop.ty, &class_ctx),
                    default_expr: render_field_default(prop),
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
                static_methods: build_method_line_views(
                    &c.static_methods,
                    &c.py_name,
                    &c.wire_generic_params,
                ),
                instance_methods: build_method_line_views(
                    &c.instance_methods,
                    &c.py_name,
                    &c.wire_generic_params,
                ),
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
            let mut out = render_type_alias(a, leaf, true, false, names);
            out.push('\n');
            out
        }
        EmittedSymbol::Function(f) => {
            let mut out = render_factory_binding(f);
            out.push('\n');
            // 32d: free functions get a runtime `__doc__` trailer carrying the
            // `Raises:` block (the factory binding itself has no docstring
            // slot). Gated on `raises_names` so non-throwing functions keep no
            // runtime `__doc__` — a no-op for every existing fixture. The
            // `_define_function` factory returns a real closure, so `__doc__`
            // is a writable attribute on both the sync and async siblings
            // (each renders as its own `EmittedSymbol::Function`).
            if !f.raises_names.is_empty() {
                if let Some(text) =
                    crate::utils::build_function_docstring(f.docstring.as_deref(), &f.raises_names)
                {
                    let triple = crate::utils::format_docstring(&text, "");
                    out.push_str(&f.py_name);
                    out.push_str(".__doc__ = ");
                    out.push_str(&triple);
                    out.push('\n');
                }
            }
            out
        }
    }
}

/// Render a type alias to its source line. Runtime and stub aliases share the
/// same structure and use the bridge stream type; stubs additionally include
/// the generated terminal marker in the stream's `next()` result.
///
/// Non-recursive aliases render as `Name: typing.TypeAlias = <RHS>`.
/// Recursive aliases (18c) render via `typing_extensions.TypeAliasType`
/// with inner self-references quoted, so Pydantic resolves them
/// through its JSON-schema definitions machinery instead of recursing.
fn render_type_alias(
    a: &crate::emit::type_alias::PyTypeAlias,
    leaf: &LeafPath,
    type_stream_accessors: bool,
    include_stream_done: bool,
    names: Option<Rc<PythonNames>>,
) -> String {
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
    if a.source.package().as_str() == "baml"
        && a.source.namespace().len() == 1
        && a.source.namespace()[0].as_str() == "json"
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
                bare_name: a.py_name.clone(),
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
        // Alias bodies are shared between `.py` and `.pyi`; a callable alias
        // with optional params widens to `typing.Callable[..., R]` rather than
        // referencing a stub-only Protocol.
        callback_protocols: None,
        type_stream_accessors,
        include_stream_done,
        names,
        type_var_names: BTreeMap::new(),
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

/// One factory-binding line. Sync and async siblings emit independently
/// but pad to align columns:
///
/// ```text
/// foo       = _define_function("<fqn>", "sync",  [<required>], [<optional>])
/// foo_async = _define_function("<fqn>", "async", [<required>], [<optional>])
/// ```
fn render_factory_binding(f: &crate::emit::function::PyFunction) -> String {
    // 6 = len("_async"): sync LHS aligns with async sibling's `=`
    // without inspecting the sibling. Mode literal padded so
    // `[<params>]` starts at the same column for both modes.
    let (lhs_pad, mode_str) = match f.mode {
        SyncAsync::Sync => ("      ", "\"sync\", "),
        SyncAsync::Async => ("", "\"async\","),
    };
    let (required_params, optional_params) = split_param_names(&f.param_names, &f.arg_defaults, 0);
    let required_params = render_param_list(&required_params);
    let optional_params = optional_param_list_arg(&optional_params);
    let param_aliases = f
        .param_names
        .iter()
        .zip(&f.wire_param_names)
        .filter(|(host, wire)| host != wire)
        .map(|(host, wire)| (host.clone(), wire.clone()))
        .collect::<Vec<_>>();
    let param_aliases = render_param_aliases(&param_aliases);
    let projection = String::new();
    let binding_metadata = render_binding_metadata(&f.py_name, &f.py_name);
    // Free functions have no enclosing class, so only their own `<...>` params
    // (bound via `_types=`) participate.
    let generic_kwargs = render_generic_kwargs(&f.wire_generic_params, &[]);
    format!(
        "{name}{lhs_pad} = _define_function({fqn}, {mode_str} {required_params}{optional_params}{param_aliases}{projection}{binding_metadata}{generic_kwargs})",
        name = f.py_name,
        fqn = py_string(&f.baml_fqn),
    )
}

/// One method-binding line, indented for a class body. Mirrors
/// `render_factory_binding` modulo the factory alias and the
/// `staticmethod(...)` wrap on statics.
fn render_method_binding(
    m: &PyMethodBinding,
    class_py_name: &str,
    class_wire_generic_params: &[String],
) -> String {
    let (lhs_pad, mode_str) = match m.mode {
        SyncAsync::Sync => ("      ", "\"sync\", "),
        SyncAsync::Async => ("", "\"async\","),
    };
    let required_params = m.runtime_required_names();
    let optional_params = m.optional_names();
    let required_params = render_param_list(&required_params);
    let optional_params = optional_param_list_arg(&optional_params);
    let param_aliases = render_param_aliases(&m.param_aliases());
    let projection = String::new();
    let binding_metadata =
        render_binding_metadata(&m.py_name, &format!("{class_py_name}.{}", m.py_name));
    // Instance methods recover the enclosing class's TypeVars from the `self`
    // receiver; static methods have no receiver, so only their own `<...>`
    // params (via `_types=`) bind.
    let class_type_params: &[String] = match m.kind {
        MethodKind::Instance => class_wire_generic_params,
        MethodKind::Static => &[],
    };
    let generic_kwargs = render_generic_kwargs(&m.wire_generic_params, class_type_params);
    let inner = format!(
        "_define_function({fqn}, {mode_str} {required_params}{optional_params}{param_aliases}{projection}{binding_metadata}{generic_kwargs})",
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

fn optional_param_list_arg(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!(", {}", render_param_list(names))
    }
}

fn render_param_aliases(aliases: &[(String, String)]) -> String {
    if aliases.is_empty() {
        return String::new();
    }
    let entries = aliases
        .iter()
        .map(|(host, wire)| format!("{}: {}", py_string(host), py_string(wire)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(", param_aliases={{{entries}}}")
}

fn render_binding_metadata(name: &str, qualname: &str) -> String {
    format!(
        ", binding_name={}, binding_qualname={}, binding_module=__name__",
        py_string(name),
        py_string(qualname),
    )
}

/// Trailing `_define_function` kwargs that turn on host-side `TypeVar` binding:
/// `type_params` (the callee's own `<...>` params, bound via `_types=`) and
/// `class_type_params` (the enclosing generic class's params, recovered from
/// the `self` receiver). Empty string when the callee binds nothing.
fn render_generic_kwargs(type_params: &[String], class_type_params: &[String]) -> String {
    let mut s = String::new();
    if !type_params.is_empty() {
        write!(s, ", type_params={}", render_param_list(type_params)).unwrap();
    }
    if !class_type_params.is_empty() {
        write!(
            s,
            ", class_type_params={}",
            render_param_list(class_type_params)
        )
        .unwrap();
    }
    s
}

fn split_param_names(
    names: &[String],
    arg_defaults: &[Option<FunctionArgumentDefault>],
    receiver_count: usize,
) -> (Vec<String>, Vec<String>) {
    let required = required_positional_count(arg_defaults, receiver_count);
    (names[..required].to_vec(), names[required..].to_vec())
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
pub(crate) fn render_leaf_body(body: &LeafBody, callable_child_names: &BTreeSet<String>) -> String {
    if body.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    let mut stdlibs = body.stdlib_imports();
    if !callable_child_names.is_empty() && !stdlibs.contains(&"importlib") {
        stdlibs.push("importlib");
    }
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
    // function-only-but-generic case (e.g. stdlib `string.from<T>`).
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
    // Factory imports use absolute paths (`baml_bridge` is a
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
            writeln!(out, "from baml_bridge import {original} as {alias}").unwrap();
        } else {
            out.push_str("from baml_bridge import (\n");
            for (original, alias) in &runtime_imports {
                writeln!(out, "    {original} as {alias},").unwrap();
            }
            out.push_str(")\n");
        }
    }

    // The `BamlError` / `BamlPanic` wrappers and optional-argument sentinel
    // are defined in `baml_bridge` and
    // re-exported on the top-level `baml` builtins package so user code can
    // `from baml_sdk.baml import BamlError, BamlPanic, UNSET`.
    let is_baml_builtins_root = body.leaf.segments == ["baml"];
    if is_baml_builtins_root {
        out.push('\n');
        out.push_str(
            "from baml_bridge import BamlError as BamlError, BamlPanic as BamlPanic, UNSET as UNSET\n",
        );
    }

    // Runtime-backed public classes are imports, not declaration bodies. Hoist
    // them with the import block so sibling modules and earlier annotations can
    // resolve exports such as `ai.FunctionSpec` deterministically.
    let runtime_reexports: Vec<_> = body
        .symbols
        .iter()
        .filter(|(symbol, _)| is_media_reexport(symbol))
        .collect();
    if !runtime_reexports.is_empty() {
        out.push('\n');
        for (symbol, _) in runtime_reexports {
            out.push_str(&render_symbol(symbol, &body.leaf, body.names.clone()));
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
        if is_media_reexport(sym) {
            continue;
        }
        let body_text = render_symbol(sym, &body.leaf, body.names.clone());
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

    if !callable_child_names.is_empty() {
        out.push_str("\n\n");
        for (idx, name) in callable_child_names.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            let module_var = format!("_{name}_namespace");
            writeln!(
                out,
                "{module_var} = importlib.import_module(\".{name}\", __name__)"
            )
            .unwrap();
            writeln!(
                out,
                "for _baml_child_name in getattr({module_var}, \"__all__\", ()):"
            )
            .unwrap();
            writeln!(
                out,
                "    setattr({name}, _baml_child_name, getattr({module_var}, _baml_child_name))"
            )
            .unwrap();
            writeln!(out, "del {module_var}").unwrap();
        }
    }

    // 25b2 Phase 4: per-leaf `_register_*` trailers are gone. The
    // codegen-emitted `baml_sdk/_typemap.py` carries every FQN → leaf
    // lazy entry; `set_type_map(_TYPE_MAP)` in the root init installs
    // it. `BamlTypeMap.get_class(fqn)` resolves via importlib on first
    // lookup. Class bodies are pure Pydantic — no codegen metadata.

    let mut names = body.all_names();
    // Surface the re-exported wrappers in `__all__` on the `baml` root too.
    if is_baml_builtins_root {
        names.push("BamlError");
        names.push("BamlPanic");
        names.push("UNSET");
    }
    if !names.is_empty() {
        out.push_str("\n\n");
        out.push_str("__all__ = [\n");
        for n in names {
            writeln!(out, "    \"{n}\",").unwrap();
        }
        out.push_str("]\n");
    }

    // Stream annotations point at the underlying runtime type directly;
    // this is the same `BamlStream` class that the `ai.stream` leaf re-exports
    // as `Stream`.
    if out.contains("_BamlStream[") {
        out.insert_str(0, "\nfrom baml_bridge import BamlStream as _BamlStream\n");
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
    {{ prop.name }}: {{ prop.ty_py }}{{ prop.default_expr }}
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

#[derive(Default)]
struct GenericInferencePositions {
    unambiguous_value: BTreeSet<String>,
    closure: BTreeSet<String>,
}

fn walk_generic_inference_positions(
    ty: &Ty,
    in_closure: bool,
    out: &mut GenericInferencePositions,
) {
    match ty {
        Ty::TypeVar(param, _) => {
            let target = if in_closure {
                &mut out.closure
            } else {
                &mut out.unambiguous_value
            };
            target.insert(param.as_str().to_string());
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                walk_generic_inference_positions(&param.ty, true, out);
            }
            walk_generic_inference_positions(ret, true, out);
            walk_generic_inference_positions(throws, true, out);
        }
        Ty::List(inner, _) => walk_generic_inference_positions(inner, in_closure, out),
        Ty::Map { key, value, .. } => {
            walk_generic_inference_positions(key, in_closure, out);
            walk_generic_inference_positions(value, in_closure, out);
        }
        Ty::Union(members, _) => {
            let direct_typevar_count = members
                .iter()
                .filter(|member| matches!(member, Ty::TypeVar(..)))
                .count();
            for member in members {
                // Multiple direct free vars in one value union cannot be split.
                // Do not count that occurrence as an inference source, but keep
                // walking every other occurrence: a separate value argument can
                // still bind the same var before the engine's fallback gate.
                if !in_closure && direct_typevar_count >= 2 && matches!(member, Ty::TypeVar(..)) {
                    continue;
                }
                walk_generic_inference_positions(member, in_closure, out);
            }
        }
        Ty::Class(_, args, _) => {
            for arg in args {
                walk_generic_inference_positions(arg, in_closure, out);
            }
        }
        Ty::Future(value, error, _) => {
            walk_generic_inference_positions(value, in_closure, out);
            walk_generic_inference_positions(error, in_closure, out);
        }
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Interface(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::TypeAlias(..)
        | Ty::Unknown { .. }
        | Ty::Never { .. } => {}
    }
}

fn generic_inference_positions<'a>(
    tys: impl IntoIterator<Item = &'a Ty>,
) -> GenericInferencePositions {
    let mut out = GenericInferencePositions::default();
    for ty in tys {
        walk_generic_inference_positions(ty, false, &mut out);
    }
    out
}

/// Whether omitting `_types=` is safe for every own generic parameter.
///
/// This mirrors the host engine's `classify_param_var_positions` rules over all
/// declared parameter types: a non-closure, non-ambiguous value occurrence is
/// inferable (Rule 4 supplies `RustType` when a defaulted value is omitted),
/// while any closure occurrence poisons the var globally. A direct
/// multi-TypeVar union is ambiguous only at that occurrence; another value
/// argument can still bind the same var. Class `TypeVars` are excluded by the
/// caller, which passes only the function or method's own `generic_params`.
fn own_generic_params_inferable<'a>(
    generic_params: &[String],
    tys: impl IntoIterator<Item = &'a Ty>,
) -> bool {
    let positions = generic_inference_positions(tys);
    generic_params.iter().all(|param| {
        positions.unambiguous_value.contains(param) && !positions.closure.contains(param)
    })
}

fn append_types_kwarg(typed_params: &mut String, has_keyword_only_marker: bool, optional: bool) {
    if typed_params.is_empty() {
        typed_params.push_str("*, ");
    } else if has_keyword_only_marker {
        typed_params.push_str(", ");
    } else {
        typed_params.push_str(", *, ");
    }
    if optional {
        typed_params.push_str("_types: dict[str, typing.Any] | None = None");
    } else {
        typed_params.push_str("_types: dict[str, typing.Any]");
    }
}

/// One method's `.pyi` signature block: a single `def` line for
/// instance methods, prefixed by `@staticmethod` for statics. When the
/// method carries a `///` docstring, replaces the trailing `...` with a
/// `"""..."""` body so `__doc__` resolves at runtime.
fn render_method_block_pyi(m: &PyMethodBinding, ctx: &TranslateCtx) -> String {
    let mut method_ctx = ctx.clone();
    method_ctx.type_var_names.extend(m.type_var_names.clone());
    let async_kw = if matches!(m.mode, SyncAsync::Async) {
        "async "
    } else {
        ""
    };
    let mut typed_params = render_method_params_pyi(m, &method_ctx);
    // A method with its OWN generic params (`pair_with<U>`, static `new<T>`)
    // makes `_types=` optional only when every own TypeVar has a value position.
    // Return/body-only, closure-poisoned, and ambiguous-union-only TypeVars keep
    // `_types=` required. Defaulted parameters still count because the engine's
    // Rule 4 supplies `RustType` when their value is omitted. The class's
    // TypeVars ride the receiver and are not part of this decision.
    if !m.generic_params.is_empty() {
        let inferable = own_generic_params_inferable(
            &m.wire_generic_params,
            m.required_args
                .iter()
                .map(|arg| &arg.ty)
                .chain(m.optional_args.iter().map(|arg| &arg.ty)),
        );
        append_types_kwarg(&mut typed_params, !m.optional_args.is_empty(), inferable);
    }
    let ret_py = translate_ty(&m.return_ty, &method_ctx);
    // 32d: methods carry their `Raises:` block in the `.pyi` only (no runtime
    // `.py` __doc__ trailer for methods). No-op when the method throws nothing.
    let doc = crate::utils::build_function_docstring(m.docstring.as_deref(), &m.raises_names);
    let signature = match doc.as_deref() {
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
        let root = m.baml_fqn.as_str();
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
fn render_symbol_pyi(
    s: &EmittedSymbol,
    leaf: &LeafPath,
    callback_protocols: Option<&std::rc::Rc<IndexMap<Ty, String>>>,
    names: Option<Rc<PythonNames>>,
) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: callback_protocols.cloned(),
        type_stream_accessors: true,
        include_stream_done: true,
        names: names.clone(),
        type_var_names: BTreeMap::new(),
    };

    match s {
        EmittedSymbol::Class(c) => {
            let class_ctx = TranslateCtx {
                type_var_names: c.type_var_names.clone(),
                ..ctx
            };
            if c.source.to_string() == AI_FUNCTION_SPEC {
                let params = c.generic_params.join(", ");
                return format!(
                    "class {}(_BamlFunctionSpec[{params}], typing.Generic[{params}]): ...\n",
                    c.py_name,
                    params = params,
                );
            }
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
                    ty_py: translate_ty(&prop.ty, &class_ctx),
                    default_expr: render_field_default(prop),
                })
                .collect();
            let mut out = ClassBodyPyi {
                py_name: c.py_name.clone(),
                bases: render_class_bases(&c.generic_params),
                properties,
                static_methods: build_method_block_views(&c.static_methods, &class_ctx),
                instance_methods: build_method_block_views(&c.instance_methods, &class_ctx),
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
            let mut out = render_type_alias(a, leaf, true, true, names);
            out.push('\n');
            out
        }
        EmittedSymbol::Function(f) => {
            let function_ctx = TranslateCtx {
                type_var_names: f.type_var_names.clone(),
                ..ctx
            };
            let mut out = render_function_signature_pyi(f, &function_ctx);
            out.push('\n');
            out
        }
    }
}

/// `name: ty, …` for static methods; `self, name: ty, …` for instance
/// methods (`self` takes no annotation).
fn render_method_params_pyi(m: &PyMethodBinding, ctx: &TranslateCtx) -> String {
    match m.kind {
        MethodKind::Static => render_typed_method_arguments(m, ctx),
        MethodKind::Instance => {
            let mut s = String::from("self");
            for arg in &m.required_args {
                s.push_str(", ");
                s.push_str(&render_param_pyi(arg.name.as_str(), &arg.ty, None, ctx));
            }
            if !m.optional_args.is_empty() {
                s.push_str(", *");
            }
            for arg in &m.optional_args {
                s.push_str(", ");
                s.push_str(&render_param_pyi(
                    arg.name.as_str(),
                    &arg.ty,
                    Some(&arg.default),
                    ctx,
                ));
            }
            s
        }
    }
}

fn render_typed_method_arguments(m: &PyMethodBinding, ctx: &TranslateCtx) -> String {
    let mut s = String::new();
    for (i, arg) in m.required_args.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&render_param_pyi(arg.name.as_str(), &arg.ty, None, ctx));
    }
    if !m.optional_args.is_empty() {
        if !s.is_empty() {
            s.push_str(", ");
        }
        s.push('*');
    }
    for arg in &m.optional_args {
        s.push_str(", ");
        s.push_str(&render_param_pyi(
            arg.name.as_str(),
            &arg.ty,
            Some(&arg.default),
            ctx,
        ));
    }
    s
}

fn render_function_params_pyi(f: &PyFunction, ctx: &TranslateCtx) -> String {
    let mut typed_params = render_typed_params(&f.param_names, &f.arg_tys, &f.arg_defaults, ctx);
    // `_types=` is optional only when every own TypeVar has an engine-inferable
    // value position. Defaulted parameters count via Rule 4. Methods use the
    // same predicate above.
    if !f.generic_params.is_empty() {
        let has_kwonly_marker = f.arg_defaults.iter().any(Option::is_some);
        let inferable = own_generic_params_inferable(&f.wire_generic_params, &f.arg_tys);
        append_types_kwarg(&mut typed_params, has_kwonly_marker, inferable);
    }
    typed_params
}

fn render_function_signature_pyi(f: &PyFunction, ctx: &TranslateCtx) -> String {
    let async_kw = if matches!(f.mode, SyncAsync::Async) {
        "async "
    } else {
        ""
    };
    let typed_params = render_function_params_pyi(f, ctx);
    let ret_py = translate_ty(&f.return_ty, ctx);
    // 32d: append the `Raises:` block to the stub docstring (a no-op when the
    // function throws nothing; flips `: ...` into a docstring body when it
    // throws but has no `///` summary).
    let doc = crate::utils::build_function_docstring(f.docstring.as_deref(), &f.raises_names);
    match doc.as_deref() {
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

fn render_callable_child_protocol_pyi(
    name: &str,
    parent_fn: &PyFunction,
    child_body: &LeafBody,
    parent_leaf: &LeafPath,
    callback_protocols: Option<&std::rc::Rc<IndexMap<Ty, String>>>,
    names: Option<Rc<PythonNames>>,
) -> String {
    let parent_ctx = TranslateCtx {
        current_leaf: parent_leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: callback_protocols.cloned(),
        type_stream_accessors: true,
        include_stream_done: true,
        names: names.clone(),
        type_var_names: parent_fn.type_var_names.clone(),
    };
    let child_ctx = TranslateCtx {
        // Child functions are exposed as methods on a Protocol emitted in the
        // parent stub. Translate every child-local reference from the parent
        // location so its spelling matches the parent-anchored imports.
        current_leaf: parent_leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: callback_protocols.cloned(),
        type_stream_accessors: true,
        include_stream_done: true,
        names,
        type_var_names: BTreeMap::new(),
    };
    let protocol_name = callable_child_protocol_name(name);
    let mut out = format!("class {protocol_name}(typing.Protocol):\n");
    out.push_str(&render_protocol_function_method_pyi(
        "__call__",
        parent_fn,
        &parent_ctx,
    ));
    for (sym, _) in &child_body.symbols {
        if let EmittedSymbol::Function(f) = sym {
            let child_function_ctx = TranslateCtx {
                type_var_names: f.type_var_names.clone(),
                ..child_ctx.clone()
            };
            out.push_str(&render_protocol_function_method_pyi(
                &f.py_name,
                f,
                &child_function_ctx,
            ));
        }
    }
    writeln!(out, "\n{name}: {protocol_name}").unwrap();
    out
}

fn callable_child_protocol_name(name: &str) -> String {
    format!("_BamlCallableNamespace_{name}")
}

fn render_protocol_function_method_pyi(name: &str, f: &PyFunction, ctx: &TranslateCtx) -> String {
    let async_kw = if matches!(f.mode, SyncAsync::Async) {
        "async "
    } else {
        ""
    };
    let typed_params = render_function_params_pyi(f, ctx);
    let params = if typed_params.is_empty() {
        "self".to_string()
    } else {
        format!("self, {typed_params}")
    };
    let ret_py = translate_ty(&f.return_ty, ctx);
    format!("    {async_kw}def {name}({params}) -> {ret_py}: ...\n")
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

/// Render one callback `typing.Protocol` block: a single-method Protocol whose
/// `__call__` carries the callable's precise signature. Required params are
/// positional; optional params (the `?` marker on a BAML callable type) get an
/// Ellipsis default (`= ...`) so a host callback that either supplies or omits
/// them type-checks. Unlike an optional *function* argument there is no
/// `UNSET` sentinel: BAML invokes the callback positionally, and the
/// callback's own language-level default fills any omitted trailing arg.
fn render_callback_protocol(
    name: &str,
    params: &[baml_codegen_types::CallableParam],
    ret: &Ty,
    ctx: &TranslateCtx,
) -> String {
    let mut sig = String::from("self");
    for (idx, p) in params.iter().enumerate() {
        let pname = p
            .name
            .as_ref()
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| format!("arg{idx}"));
        let pty = translate_ty(&p.ty, ctx);
        if p.mode == baml_codegen_types::CodegenFunctionParamMode::Optional {
            write!(sig, ", {pname}: {pty} = ...").unwrap();
        } else {
            write!(sig, ", {pname}: {pty}").unwrap();
        }
    }
    let ret_py = translate_ty(ret, ctx);
    format!("class {name}(typing.Protocol):\n    def __call__({sig}) -> {ret_py}: ...\n")
}

fn render_param_pyi(
    name: &str,
    ty: &Ty,
    default: Option<&FunctionArgumentDefault>,
    ctx: &TranslateCtx,
) -> String {
    let ty_py = translate_ty(ty, ctx);
    let mut s = if default.is_some() {
        format!("{name}: {}", with_unset_union(&ty_py))
    } else {
        format!("{name}: {ty_py}")
    };
    if let Some(default) = default {
        s.push_str(" = ");
        s.push_str(&render_default_pyi(default));
    }
    s
}

fn with_unset_union(ty_py: &str) -> String {
    // `UNSET` is a PEP 661 sentinel that doubles as its own type. Type
    // checkers only accept a sentinel in a type expression when it is a bare
    // name, so the leaf imports `UNSET` directly (see `render_leaf_body_pyi`)
    // and we reference it unqualified here rather than as `baml.UNSET`.
    if let Some(inner) = ty_py
        .strip_prefix("typing.Union[")
        .and_then(|inner| inner.strip_suffix(']'))
    {
        format!("typing.Union[{inner}, UNSET]")
    } else if let Some(inner) = ty_py
        .strip_prefix("typing.Optional[")
        .and_then(|inner| inner.strip_suffix(']'))
    {
        // `Optional[X]` is `Union[X, None]`; flatten so a nullable keyword
        // argument composes into a single `Union[X, None, UNSET]`.
        format!("typing.Union[{inner}, None, UNSET]")
    } else {
        format!("typing.Union[{ty_py}, UNSET]")
    }
}

fn render_default_pyi(default: &FunctionArgumentDefault) -> String {
    match default {
        FunctionArgumentDefault::Null => "None".to_string(),
        FunctionArgumentDefault::Literal(DefaultLiteral::Scalar(lit)) => {
            render_literal_default(lit)
        }
        FunctionArgumentDefault::Literal(DefaultLiteral::EmptyList) => "[]".to_string(),
        FunctionArgumentDefault::Literal(DefaultLiteral::EmptyMap) => "{}".to_string(),
        FunctionArgumentDefault::Expression { .. } => "UNSET".to_string(),
    }
}

fn render_literal_default(lit: &Literal) -> String {
    match lit {
        Literal::Int(value) => value.to_string(),
        Literal::Bigint(value) => value.to_string(),
        Literal::Float(value) => value.clone(),
        Literal::String(value) => py_string(value),
        Literal::Bool(true) => "True".to_string(),
        Literal::Bool(false) => "False".to_string(),
    }
}

/// Mirrors `render_leaf_body` with these differences: no
/// `baml_bridge` factory imports; `typing` is needed whenever a
/// signature is present (`needs_typing_pyi`); `enum` and `pydantic`
/// follow the `.py` rule.
pub(crate) fn render_leaf_body_pyi(
    body: &LeafBody,
    callable_child_bodies: &BTreeMap<String, &LeafBody>,
) -> String {
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
    let mut rel_imports = body.all_rel_imports_py();
    for child_body in callable_child_bodies.values() {
        rel_imports.extend(child_body.all_rel_imports_from(&body.leaf));
    }
    rel_imports.sort();
    rel_imports.dedup();
    if body.has_defaulted_call_params() && body.leaf.segments != ["baml"] {
        // Optional arguments annotate as `typing.Union[..., UNSET]` and
        // default to `UNSET`. The sentinel must be a bare name in the type
        // expression (type checkers reject `baml.UNSET` member access there),
        // so import it directly from the `baml` builtins package rather than
        // importing the package and using attribute access.
        rel_imports.push(RelImport {
            depth: body.leaf.segments.len() + 1,
            from_path: "baml".to_string(),
            anchor: "UNSET as UNSET".to_string(),
        });
        rel_imports.sort();
        rel_imports.dedup();
    }
    let needs_typing =
        body.needs_typing_pyi() || !rel_imports.is_empty() || !callable_child_bodies.is_empty();
    // Stream accessor annotations can use `typing_extensions.Never` for
    // Python 3.10. Stubs with type expressions import the backport alongside
    // `typing`, even when this particular leaf has no recursive alias.
    let needs_typing_extensions = body.has_recursive_alias() || needs_typing;
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
        out.push_str("from baml_bridge import BamlPyHandle as _BamlPyHandle\n");
    }

    // Mirror the `.py` re-export so `from baml_sdk.baml import BamlError,
    // BamlPanic, UNSET` type-checks.
    let is_baml_builtins_root = body.leaf.segments == ["baml"];
    if is_baml_builtins_root {
        out.push('\n');
        out.push_str(
            "from baml_bridge import BamlError as BamlError, BamlPanic as BamlPanic, UNSET as UNSET\n",
        );
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

    // Callback Protocols: each optional-argument callable in the leaf gets a
    // `typing.Protocol` with a `__call__` carrying its precise signature, named
    // `_BamlCallback{n}`. Signatures below reference these by name. Emitted
    // before the symbols that use them; the shared map lets nested callbacks
    // resolve to each other's Protocol names.
    let protocol_tys = body.callback_protocols();
    let callback_protocols: Option<std::rc::Rc<IndexMap<Ty, String>>> = if protocol_tys.is_empty() {
        None
    } else {
        // Final name = `<base><n>`, where `n` is a per-base 1-based counter
        // (usually 1; >1 only when one parameter's type nests multiple
        // optional-arg callables under the same `_<owner>__<param>` prefix).
        // `IndexMap` keeps deterministic (insertion-order) iteration so the
        // emitted Protocols are stable across runs.
        let mut base_counts: IndexMap<String, usize> = IndexMap::new();
        let mut map: IndexMap<Ty, String> = IndexMap::new();
        for (ty, base) in &protocol_tys {
            let n = base_counts.entry(base.clone()).or_insert(0);
            *n += 1;
            map.insert(ty.clone(), format!("{base}{n}"));
        }
        Some(std::rc::Rc::new(map))
    };
    if let Some(map) = &callback_protocols {
        let proto_ctx = TranslateCtx {
            current_leaf: body.leaf.clone(),
            self_ref: None,
            defer_name_refs: false,
            callback_protocols: Some(map.clone()),
            type_stream_accessors: true,
            include_stream_done: true,
            names: body.names.clone(),
            type_var_names: BTreeMap::new(),
        };
        for (ty, _base) in &protocol_tys {
            if let Ty::Function { params, ret, .. } = ty {
                out.push_str("\n\n");
                out.push_str(&render_callback_protocol(&map[ty], params, ret, &proto_ctx));
            }
        }
    }

    for (name, child_body) in callable_child_bodies {
        if let Some(parent_fn) = body.symbols.iter().find_map(|(sym, _)| match sym {
            EmittedSymbol::Function(f) if f.mode == SyncAsync::Sync && f.py_name == *name => {
                Some(f)
            }
            _ => None,
        }) {
            out.push_str("\n\n");
            out.push_str(&render_callable_child_protocol_pyi(
                name,
                parent_fn,
                child_body,
                &body.leaf,
                callback_protocols.as_ref(),
                body.names.clone(),
            ));
        }
    }

    out.push_str("\n\n");

    let mut prev: Option<(&SortKey, &EmittedSymbol)> = None;
    for (sym, key) in &body.symbols {
        if is_media_reexport(sym) && !is_function_spec_reexport(sym) {
            continue;
        }
        if let EmittedSymbol::Function(f) = sym
            && f.mode == SyncAsync::Sync
            && callable_child_bodies.contains_key(&f.py_name)
        {
            continue;
        }
        let body_text = render_symbol_pyi(
            sym,
            &body.leaf,
            callback_protocols.as_ref(),
            body.names.clone(),
        );
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

    let mut names = body.all_names();
    if is_baml_builtins_root {
        names.push("BamlError");
        names.push("BamlPanic");
        names.push("UNSET");
    }
    if !names.is_empty() {
        out.push_str("\n\n");
        out.push_str("__all__ = [\n");
        for n in names {
            writeln!(out, "    \"{n}\",").unwrap();
        }
        out.push_str("]\n");
    }

    // Stream annotations use the bridge runtime type directly. Outside the
    // `ai.stream` leaf, import the generated terminal marker under a private
    // alias so nested stream types do not depend on package attribute cascades
    // or create runtime import cycles.
    let mut stream_imports = String::new();
    if out.contains("_BamlStreamDone") {
        let dots = ".".repeat(body.leaf.segments.len() + 1);
        writeln!(
            stream_imports,
            "from {dots}ai.stream import Done as _BamlStreamDone"
        )
        .unwrap();
    }
    if out.contains("_BamlStream[") {
        stream_imports.push_str("from baml_bridge import BamlStream as _BamlStream\n");
    }
    if !stream_imports.is_empty() {
        stream_imports.insert(0, '\n');
        out.insert_str(0, &stream_imports);
    }

    out
}

/// Runtime-backed public aliases must precede eager child-module re-exports in
/// stubs. Otherwise a child such as `ai.stream` observes a partially defined
/// `ai` module and pyright permanently records `ai.FunctionSpec` as missing.
pub(crate) fn render_runtime_reexports_pyi(body: &LeafBody) -> String {
    let mut out = String::new();
    for (symbol, _) in body
        .symbols
        .iter()
        .filter(|(symbol, _)| is_media_reexport(symbol))
    {
        if is_function_spec_reexport(symbol) {
            out.push_str("from baml_bridge import BamlFunctionSpec as _BamlFunctionSpec\n");
        } else {
            out.push_str(&render_symbol_pyi(
                symbol,
                &body.leaf,
                None,
                body.names.clone(),
            ));
        }
    }
    out
}
