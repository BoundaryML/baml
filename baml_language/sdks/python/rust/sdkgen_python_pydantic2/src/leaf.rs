//! Per-leaf symbol bundle and leaf renderer. One `LeafBody` per
//! populated leaf file, pre-sorted at build time so the renderer is a
//! straight walk — no ordering logic at render time.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt::Write as _,
};

use baml_base::{Literal, qualified_name::AI_STREAM_STREAM};
use baml_codegen_types::{DefaultLiteral, FunctionArgumentDefault, Ty};
use indexmap::IndexMap;

use crate::{
    emit::{
        EmittedSymbol, SortKey, TypeVarMap, bare_callable_name, escape_python_keyword,
        function::{PyFunction, SyncAsync},
        method::{MethodKind, PyMethodBinding},
    },
    py_string,
    routing::{LeafPath, route_class_ref},
    translate_ty::{
        CallbackProtocolKey, SelfRef, TranslateCtx, callback_protocol_key, translate_ty,
    },
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
    /// function/method/field signatures, in deterministic first-seen order, each
    /// carrying the `_<owner>__<param>` prefix for its Protocol name
    /// (`render_leaf_body_pyi` appends a per-prefix counter, giving e.g.
    /// `_call_optional_int_callback_supplied__callback1`) AND the OWNER scope's
    /// `TypeVar` map. Each is rendered as a named `typing.Protocol` in the
    /// `.pyi` (see `render_callback_protocol`), because a `typing.Callable[[…],
    /// R]` type can't express per-parameter optionality.
    ///
    /// The owner map is carried so the Protocol body renders a bumped keyword
    /// `TypeVar` with the owner's declared spelling (`None`→`None__`), not the
    /// stateless escape (`None_`). De-duplication is deferred to the caller and
    /// keyed on the RENDERED identity (structural `Ty` + owner-map projection),
    /// so two owners whose maps produce the SAME spelling still share one Protocol
    /// (keyword-free schemas always do → byte-identical) while owners that spell
    /// the callback's `TypeVars` differently get distinct Protocols. `seen` is kept
    /// per-owner (not leaf-wide) precisely so a shared structural `Ty` under two
    /// incompatible owners is not collapsed before that keyed dedup runs.
    ///
    /// Type aliases are skipped: they render via the shared `.py`/`.pyi`
    /// `render_type_alias` (which has no Protocol map), so a callable alias
    /// falls back to `typing.Callable[..., R]` rather than referencing a
    /// Protocol — collecting one here would emit an unused class.
    pub(crate) fn callback_protocols(&self) -> Vec<CallbackProtocol> {
        let mut out: Vec<CallbackProtocol> = Vec::new();
        for (sym, _) in &self.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    let class_map = scope_type_var_map(&c.type_var_map);
                    // Class fields share one `seen`: the owner is the class.
                    let mut field_seen: HashSet<Ty> = HashSet::new();
                    for prop in &c.properties {
                        let base = format!("_{}__{}", c.py_name, prop.name);
                        collect_optional_callables(
                            &prop.ty,
                            &base,
                            class_map.as_ref(),
                            &mut field_seen,
                            &mut out,
                        );
                    }
                    for m in c.static_methods.iter().chain(&c.instance_methods) {
                        // `bare_callable_name` applies the companion `$` rule
                        // (`$stream` → `_stream`, `$<other>` → `__<other>`) so the
                        // Protocol identifier stays a valid Python name. Each
                        // method is its own owner scope: its annotation map
                        // is the class map merged with the method's own params.
                        let owner_map = method_scope_type_var_map(m, class_map.as_ref());
                        let owner = bare_callable_name(fqn_leaf(&m.baml_fqn));
                        let mut method_seen: HashSet<Ty> = HashSet::new();
                        for arg in &m.required_args {
                            let base = format!("_{owner}__{}", arg.name);
                            collect_optional_callables(
                                &arg.ty,
                                &base,
                                owner_map.as_ref(),
                                &mut method_seen,
                                &mut out,
                            );
                        }
                        for arg in &m.optional_args {
                            let base = format!("_{owner}__{}", arg.name);
                            collect_optional_callables(
                                &arg.ty,
                                &base,
                                owner_map.as_ref(),
                                &mut method_seen,
                                &mut out,
                            );
                        }
                        let base = format!("_{owner}__ret");
                        collect_optional_callables(
                            &m.return_ty,
                            &base,
                            owner_map.as_ref(),
                            &mut method_seen,
                            &mut out,
                        );
                    }
                }
                EmittedSymbol::Function(f) => {
                    // See the method case: normalize the companion `$` suffix so
                    // the Protocol identifier is a valid Python name.
                    let owner_map = scope_type_var_map(&f.type_var_map);
                    let owner = bare_callable_name(fqn_leaf(&f.baml_fqn));
                    let mut fn_seen: HashSet<Ty> = HashSet::new();
                    for (name, ty) in f.param_names.iter().zip(f.arg_tys.iter()) {
                        let base = format!("_{owner}__{name}");
                        collect_optional_callables(
                            ty,
                            &base,
                            owner_map.as_ref(),
                            &mut fn_seen,
                            &mut out,
                        );
                    }
                    let base = format!("_{owner}__ret");
                    collect_optional_callables(
                        &f.return_ty,
                        &base,
                        owner_map.as_ref(),
                        &mut fn_seen,
                        &mut out,
                    );
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
                        collect_root_imports(&prop.ty, current, &mut acc);
                    }
                    for m in &c.static_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
                        }
                        collect_root_imports(&m.return_ty, current, &mut acc);
                    }
                    for m in &c.instance_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
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
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
                        }
                        collect_root_imports(&m.return_ty, current, &mut acc);
                    }
                    for m in &c.instance_methods {
                        for arg in &m.required_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
                        }
                        for arg in &m.optional_args {
                            collect_root_imports(&arg.ty, current, &mut acc);
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

/// The last `.`-separated segment of a BAML FQN — the unqualified
/// function/method name, used as the owner part of a callback Protocol name.
fn fqn_leaf(fqn: &str) -> &str {
    fqn.rsplit('.').next().unwrap_or(fqn)
}

/// One optional-argument callback `Ty::Function` collected for `.pyi` Protocol
/// rendering, with everything needed to name, dedup, and render it:
/// the structural callback type, the `_<owner>__<param>` name prefix, and the
/// OWNER scope's `TypeVar` map (so the Protocol body renders bumped keyword
/// `TypeVars` with the owner's declared spelling).
pub(crate) struct CallbackProtocol {
    pub(crate) ty: Ty,
    pub(crate) base: String,
    pub(crate) owner_map: Option<std::rc::Rc<TypeVarMap>>,
}

/// Recursively collect every optional-argument `Ty::Function` reachable from
/// `ty`, in first-seen order, skipping duplicates within this owner (`seen`).
/// Each carries `base` — the `_<owner>__<param>` prefix for its Protocol name
/// (the final name appends a per-base counter in `render_leaf_body_pyi`) — and
/// `owner_map`, the enclosing scope's raw→emitted `TypeVar` map. Children are
/// visited before the enclosing callable so nested callbacks get earlier names.
fn collect_optional_callables(
    ty: &Ty,
    base: &str,
    owner_map: Option<&std::rc::Rc<TypeVarMap>>,
    seen: &mut HashSet<Ty>,
    out: &mut Vec<CallbackProtocol>,
) {
    match ty {
        Ty::List(inner, _) => collect_optional_callables(inner, base, owner_map, seen, out),
        Ty::Map { key, value, .. } => {
            collect_optional_callables(key, base, owner_map, seen, out);
            collect_optional_callables(value, base, owner_map, seen, out);
        }
        Ty::Union(items, _) => {
            for item in items {
                collect_optional_callables(item, base, owner_map, seen, out);
            }
        }
        Ty::Class(_, args, _) => {
            for a in args {
                collect_optional_callables(a, base, owner_map, seen, out);
            }
        }
        Ty::Function { params, ret, .. } => {
            for p in params {
                collect_optional_callables(&p.ty, base, owner_map, seen, out);
            }
            collect_optional_callables(ret, base, owner_map, seen, out);
            let has_optional = params
                .iter()
                .any(|p| p.mode == baml_codegen_types::CodegenFunctionParamMode::Optional);
            if has_optional && seen.insert(ty.clone()) {
                out.push(CallbackProtocol {
                    ty: ty.clone(),
                    base: base.to_string(),
                    owner_map: owner_map.cloned(),
                });
            }
        }
        _ => {}
    }
}

fn collect_root_imports(ty: &Ty, current: &LeafPath, out: &mut RootImportSets) {
    match ty {
        Ty::Class(name, args, _) => {
            record_name_routing(name, current, out);
            for a in args {
                collect_root_imports(a, current, out);
            }
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) | Ty::TypeAlias(name, _) => {
            record_name_routing(name, current, out);
        }
        Ty::List(inner, _) => collect_root_imports(inner, current, out),
        Ty::Map { key, value, .. } => {
            collect_root_imports(key, current, out);
            collect_root_imports(value, current, out);
        }
        Ty::Union(items, _) => {
            for item in items {
                collect_root_imports(item, current, out);
            }
        }
        Ty::Function { params, ret, .. } => {
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
        | Ty::BuiltinUnknown { .. }
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
) {
    let routed = route_class_ref(name);
    if routed == *current {
        return;
    }
    if routed.segments.is_empty() {
        // Root-routed type referenced from a non-root leaf — the translator
        // emits the keyword-escaped bare name (`None_`, via
        // `translate_ty::render_name_ref`), so the import anchor and the
        // recorded root name must carry the SAME escaped spelling or the
        // `from <dots> import None_` line and the root's `class None_`
        // definition would diverge (`from .. import None` is a SyntaxError and
        // leaves `None_` unbound). The root leaf itself never reaches here
        // (current is also empty there, so `routed == *current`). The non-root
        // branch below routes by module-path segment, which the routing
        // sanitizer handles separately (upstream TODO, left to a follow-up change).
        if !current.segments.is_empty() {
            let bare = escape_python_keyword(name.bare_name().to_string());
            out.root_names.insert(bare.clone());
            out.rel.insert(RelImport {
                depth: current.segments.len() + 1,
                from_path: String::new(),
                anchor: bare,
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
        symbols.extend(sort_aliases(recursive_aliases));
        symbols.extend(other_symbols);
        symbols.extend(sort_aliases(non_recursive_aliases));
        out.insert(leaf.clone(), LeafBody { leaf, symbols });
    }
    out
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
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Void { .. }
        | Ty::Interface(..)
        | Ty::Future(..) => {}
    }
}

/// Allocate every generic scope's `TypeVar` names ONCE per leaf against
/// a leaf-global reservation set, replacing the old per-`<…>`-list allocation.
///
/// Runs after `group_and_sort`, when every symbol that lands in a leaf is known.
/// At entry each scope's `generic_params` still hold the RAW binder names and
/// `type_var_map` is empty (see `emit::build_emitted`); this pass rewrites both.
/// `children` is the directory→immediate-child-name map (built in `lib.rs` before
/// this call) so the reservation set can cover module-level bindings that live
/// outside `body.symbols`.
///
/// Guarantees:
/// - **Non-keyword raw names map to themselves, unconditionally** — even if the
///   name collides with a leaf class named `T` (that ordinary class-vs-TypeVar
///   module collision is pre-existing on canary and explicitly NOT fixed this
///   round). This preserves STRICT byte-identity for keyword-free schemas.
/// - **Keyword raw names bump** (`None`→`None_`→`None__`…) past a reservation set
///   made of every module-level binding a bumped name could shadow:
///   (a) every leaf emitted symbol name (class/enum/alias/function `py_name`);
///   (b) every module import anchor brought in through the SDK root;
///   (c) `__all__`; (d) every raw `TypeVar` name in the leaf;
///   (e) every already-allocated emitted `TypeVar` name;
///   (f) every immediate child-package/submodule name of this leaf's directory
///   (`from . import <child>` / `_LAZY_CHILDREN`), and every keyword-escaped
///   callable-child import anchor aggregated into the parent stub. (f) is
///   supplied by `compute_module_binding_reservations` — those bindings are
///   cross-body / render-time facts the per-body walk cannot see, and omitting
///   them would let a keyword `TypeVar` overwrite a real child module or rebind
///   an imported child class.
/// - **Deliberately NOT reserved, unreachable by construction:** stdlib module
///   imports (`typing`/`pydantic`/`enum`/…) and every `_`-leading helper
///   (`_{name}_namespace`, `_BamlCallableNamespace_…`, runtime aliases). Bumping
///   only appends trailing `_` to a hard keyword, so an emitted `TypeVar` name
///   can never be underscore-leading and never equals a stdlib module name — no
///   reservation entry can ever be consulted for them.
/// - The map is keyed by **raw name**, so distinct raw NAMES/SPELLINGS never
///   collapse to one emitted name; identical raw spellings (the same `T` reused
///   across scopes, or two independently-generic functions each declaring `T`)
///   deliberately share ONE module-level `TypeVar`, which is sound because each
///   scope is independently generic on it — closing the emitted-name and runtime
///   `_types`-key collapse.
pub(crate) fn allocate_leaf_type_vars(
    bodies: &mut BTreeMap<LeafPath, LeafBody>,
    children: &BTreeMap<Vec<String>, BTreeSet<String>>,
) {
    // Read-only pre-pass FIRST: collect each leaf's module-binding reservation
    // supplement while `bodies` is only shared-borrowed, so the mutation loop
    // below can hold the exclusive `values_mut()` borrow without conflict
    // (the anchor set is a pure function of the already-built bodies).
    let supplement = compute_module_binding_reservations(bodies, children);
    for (leaf, body) in bodies.iter_mut() {
        allocate_leaf_body_type_vars(body, supplement.get(leaf));
    }
}

/// Per-leaf reservation supplement (category (f) in `allocate_leaf_type_vars`):
/// module-level bindings that occupy a leaf's namespace but are NOT in
/// `body.symbols` or `body.root_imports_py()`, so the per-body reservation walk
/// misses them. Computed here in a read-only pre-pass over `bodies`:
///
/// - **(a) immediate child-package/submodule names.** `children` maps each
///   directory to the emitted attribute names of its immediate children
///   (`from . import None_`, `_LAZY_CHILDREN {"None_"}`, PEP-562 `__getattr__`).
///   A keyword `TypeVar` bumping onto a child name — e.g. generic raw `None`
///   escaping to `None_` onto a child module `None_` — would overwrite the child
///   binding at import time. Names are the emitted segments already
///   (routing sanitized them), so reserving them verbatim is exact.
/// - **(b) keyword-escaped callable-child import anchors.** `render_leaf_body_pyi`
///   aggregates a callable child's argument/return imports into the parent stub
///   (`callable_child_parent_rel_imports`), gated to keyword-escaped classes.
///   A bumped keyword `TypeVar` colliding with such a
///   `from ... import None_` would rebind the imported class to the `TypeVar`.
///   Only keyword-
///   escaped anchors survive the gate, and those are exactly the collision
///   candidates, so reserving just them suffices. Name-only: depth is irrelevant
///   for a reservation set.
fn compute_module_binding_reservations(
    bodies: &BTreeMap<LeafPath, LeafBody>,
    children: &BTreeMap<Vec<String>, BTreeSet<String>>,
) -> BTreeMap<LeafPath, BTreeSet<String>> {
    let mut out: BTreeMap<LeafPath, BTreeSet<String>> = BTreeMap::new();
    for (leaf, body) in bodies {
        let mut names: BTreeSet<String> = BTreeSet::new();
        let kids = children.get(&leaf.segments).cloned().unwrap_or_default();
        // (a) immediate child-package/submodule names.
        names.extend(kids.iter().cloned());
        // (b) keyword-escaped, aggregated callable-child import anchors. Mirror
        //     the render path's aggregation but keep only the anchor NAMES.
        let callable_child_names = body.callable_child_names(&kids);
        let callable_child_bodies =
            crate::callable_child_bodies(&leaf.segments, &callable_child_names, bodies);
        for anchor in callable_child_parent_rel_imports(leaf, &callable_child_bodies) {
            names.insert(anchor.anchor);
        }
        if !names.is_empty() {
            out.insert(leaf.clone(), names);
        }
    }
    out
}

fn collect_raw_type_vars(names: &[String], order: &mut Vec<String>, seen: &mut HashSet<String>) {
    for n in names {
        if seen.insert(n.clone()) {
            order.push(n.clone());
        }
    }
}

/// Rewrite one scope: `generic_params` (still RAW at entry) becomes the emitted
/// names in declaration order, and `type_var_map` becomes the restriction of the
/// leaf map to this scope's raw names.
fn project_scope_type_vars(
    generic_params: &mut Vec<String>,
    type_var_map: &mut TypeVarMap,
    leaf_map: &TypeVarMap,
) {
    let raws = std::mem::take(generic_params);
    let mut map = TypeVarMap::new();
    let mut emitted = Vec::with_capacity(raws.len());
    for raw in raws {
        let e = leaf_map.get(&raw).cloned().unwrap_or_else(|| raw.clone());
        map.insert(raw, e.clone());
        emitted.push(e);
    }
    *generic_params = emitted;
    *type_var_map = map;
}

fn allocate_leaf_body_type_vars(body: &mut LeafBody, extra_reserved: Option<&BTreeSet<String>>) {
    // 1. Distinct RAW TypeVar names across every scope, first-appearance order
    //    (class params, then its methods' params, then free functions).
    let mut raw_order: Vec<String> = Vec::new();
    let mut raw_set: HashSet<String> = HashSet::new();
    for (sym, _) in &body.symbols {
        match sym {
            EmittedSymbol::Class(c) => {
                collect_raw_type_vars(&c.generic_params, &mut raw_order, &mut raw_set);
                for m in c.static_methods.iter().chain(&c.instance_methods) {
                    collect_raw_type_vars(&m.generic_params, &mut raw_order, &mut raw_set);
                }
            }
            EmittedSymbol::Function(f) => {
                collect_raw_type_vars(&f.generic_params, &mut raw_order, &mut raw_set);
            }
            EmittedSymbol::Enum(_) | EmittedSymbol::TypeAlias(_) => {}
        }
    }
    if raw_order.is_empty() {
        return; // no generics in this leaf — nothing to allocate
    }

    // 2. Reservation set for keyword bumps.
    let mut reserved: HashSet<String> = HashSet::new();
    // (a) every leaf-level emitted symbol name (class/enum/alias/function).
    for (sym, _) in &body.symbols {
        reserved.insert(sym.py_name().to_string());
    }
    // (b) module import anchors brought in through the SDK root.
    let imports = body.root_imports_py();
    reserved.extend(imports.segments);
    reserved.extend(imports.root_names);
    // (c) the `__all__` module binding. The `_{name}_namespace` callable-child
    //     helpers are unreachable by keyword bumping (bumping only appends `_`,
    //     which can never produce a `_`-leading name), so they need no entry.
    reserved.insert("__all__".to_string());
    // (d) every raw TypeVar name in the leaf, any scope.
    reserved.extend(raw_set.iter().cloned());
    // (f) module-level bindings this leaf emits that are NOT symbols or import
    //     anchors: immediate child-package/submodule names and keyword-escaped
    //     callable-child import anchors. Threaded in from the read-only
    //     pre-pass (`compute_module_binding_reservations`) because they are
    //     cross-body / render-time facts the per-body walk above cannot see —
    //     omitting them would let a keyword TypeVar overwrite a real child module
    //     or rebind an imported child class.
    if let Some(extra) = extra_reserved {
        reserved.extend(extra.iter().cloned());
    }

    // 3. Allocate the ONE leaf map (raw -> emitted).
    let mut leaf_map: TypeVarMap = TypeVarMap::new();
    for raw in &raw_order {
        if !crate::emit::is_python_hard_keyword(raw) {
            leaf_map.insert(raw.clone(), raw.clone());
        } else {
            let mut candidate = format!("{raw}_");
            while crate::emit::is_python_hard_keyword(&candidate) || reserved.contains(&candidate) {
                candidate.push('_');
            }
            reserved.insert(candidate.clone()); // (e) already-allocated emitted names
            leaf_map.insert(raw.clone(), candidate);
        }
    }

    // 4. Project the leaf map onto each scope.
    for (sym, _) in &mut body.symbols {
        match sym {
            EmittedSymbol::Class(c) => {
                project_scope_type_vars(&mut c.generic_params, &mut c.type_var_map, &leaf_map);
                for m in c
                    .static_methods
                    .iter_mut()
                    .chain(c.instance_methods.iter_mut())
                {
                    project_scope_type_vars(&mut m.generic_params, &mut m.type_var_map, &leaf_map);
                }
            }
            EmittedSymbol::Function(f) => {
                project_scope_type_vars(&mut f.generic_params, &mut f.type_var_map, &leaf_map);
            }
            EmittedSymbol::Enum(_) | EmittedSymbol::TypeAlias(_) => {}
        }
    }
}

/// Render-boundary guard mirroring the Go generator's `GoIdent::new`
/// assertion (PR #4067): no identifier reaching the emitter may still be a
/// Python hard keyword. A miss here is a codegen bug (an unescaped emit site),
/// so it panics loudly in debug builds with the offending name. Off in release.
fn debug_assert_identifier(name: &str) {
    debug_assert!(
        !crate::emit::is_python_hard_keyword(name),
        "Python keyword escaped too late: {name}"
    );
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
        AI_STREAM_STREAM => Some(("baml_bridge", "BamlStream")),
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
    model_config = pydantic.ConfigDict(extra="forbid"{% if populate_by_name %}, populate_by_name=True{% endif %})
{%- if let Some(marker) = wire_names_marker %}
    {{ marker }}
{%- endif %}
{%- for prop in properties %}
    {{ prop.name }}: {{ prop.ty_py }}{{ prop.field_expr }}
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
    /// True when ≥1 field was keyword-escaped and therefore carries a
    /// `pydantic.Field(alias=…)`. Adds `populate_by_name=True` to the
    /// `ConfigDict` so the model validates from both the escaped Python
    /// attribute name and the raw BAML/JSON key. Classes with no escaped
    /// field leave this `false` and render byte-identically to today.
    populate_by_name: bool,
    /// `Some("__baml_wire_names__ = {…}")` when
    /// ≥1 field was keyword-escaped: an explicit map of escaped Python attribute
    /// name → raw BAML/wire name, consumed by the bridge encode path
    /// (`proto.py`) as the sole provenance signal. A dunder name keeps it out of
    /// pydantic's field set / `model_json_schema()` (mirrors the enum
    /// `__baml_wire_values__` marker), and avoids a runtime `typing.get_type_hints`
    /// hazard from bare-builtin subscripts shadowed by a user `type dict = …`.
    /// `None` (no escaped field) emits nothing, so
    /// keyword-free classes stay byte-identical. `.py`-only — the `.pyi` stub
    /// carries no runtime detail, mirroring `Field(alias=…)`.
    wire_names_marker: Option<String>,
    properties: Vec<ClassPropertyView>,
    static_methods: Vec<MethodLineView>,
    instance_methods: Vec<MethodLineView>,
}

struct ClassPropertyView {
    name: String,
    ty_py: String,
    /// Trailing field expression appended after the annotation. Empty for a
    /// plain field; ` = pydantic.Field(alias="<raw>")` for a keyword-escaped
    /// field so the raw BAML name stays the wire/JSON key. Only consumed by
    /// the `.py` template — the `.pyi` stub renders the escaped name alone.
    field_expr: String,
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
{%- endif %}
{%- if let Some(marker) = wire_values_marker %}
    {{ marker }}
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
    /// `Some("__baml_wire_values__ = {…}")` when ≥1 member was keyword-escaped: a
    /// plain dunder dict mapping escaped Python member name → raw BAML/wire name.
    /// A dunder is excluded from `enum.Enum` membership by
    /// `EnumMeta`, so it never becomes a variant. Consumed by the bridge encode
    /// path as the sole provenance signal. `None` (no escaped member)
    /// emits nothing, so keyword-free enums stay byte-identical. `.py`-only,
    /// mirroring the class marker and `Field(alias=…)`.
    wire_values_marker: Option<String>,
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
fn build_method_line_views(
    methods: &[PyMethodBinding],
    class_generic_params: &[String],
) -> Vec<MethodLineView> {
    let mut out = Vec::with_capacity(methods.len());
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = source_method_root(&m.baml_fqn);
        let tight_to_prev = match prev_root {
            None => true,
            Some(p) => p == root,
        };
        out.push(MethodLineView {
            line: render_method_binding(m, class_generic_params),
            tight_to_prev,
        });
        prev_root = Some(root);
    }
    out
}

/// Wrap a scope's raw→emitted `TypeVar` map into the `Option<Rc<…>>` a
/// `TranslateCtx` carries. `None` (the common case) when the scope declares no
/// generics, so every non-generic body builds a byte-identical ctx.
fn scope_type_var_map(map: &TypeVarMap) -> Option<std::rc::Rc<TypeVarMap>> {
    if map.is_empty() {
        None
    } else {
        Some(std::rc::Rc::new(map.clone()))
    }
}

/// The map a method's `.pyi` signature is translated under: the method's own
/// `<…>` params always bind, and the enclosing class's params bind too.
/// BAML statics legitimately reference the enclosing class's `TypeVars` in their
/// signatures (stdlib `Array.filled`/`generate`), so the class map is threaded
/// for ANNOTATION resolution regardless of `MethodKind`. Only the RUNTIME
/// `_types=` receiver binding still differs by kind (`render_method_binding`):
/// an instance method recovers the class `TypeVars` from `self`; a static does not.
fn method_scope_type_var_map(
    m: &PyMethodBinding,
    class_map: Option<&std::rc::Rc<TypeVarMap>>,
) -> Option<std::rc::Rc<TypeVarMap>> {
    match (class_map, m.type_var_map.is_empty()) {
        (None, true) => None,
        (Some(cm), true) => Some(cm.clone()),
        (None, false) => Some(std::rc::Rc::new(m.type_var_map.clone())),
        (Some(cm), false) => {
            // Method params shadow class params on a name clash.
            let mut merged: TypeVarMap = (**cm).clone();
            merged.extend(m.type_var_map.iter().map(|(k, v)| (k.clone(), v.clone())));
            Some(std::rc::Rc::new(merged))
        }
    }
}

/// Build the `__baml_wire_names__` class marker: a plain dunder dict mapping each
/// keyword-escaped Python attribute name to its raw BAML/wire name, in declaration
/// order. `None` when no field was escaped, so keyword-free classes emit nothing
/// and stay byte-identical. A dunder name is excluded from pydantic's field set,
/// so the assignment carries no annotation (mirrors the enum `__baml_wire_values__`
/// marker) and cannot trip `typing.get_type_hints`. `.py`-only (see `ClassBodyPy`).
fn build_wire_names_marker(properties: &[crate::emit::class::PyClassProperty]) -> Option<String> {
    let entries: Vec<String> = properties
        .iter()
        .filter_map(|p| {
            p.alias
                .as_deref()
                .map(|raw| format!("{}: {}", py_string(&p.name), py_string(raw)))
        })
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(format!("__baml_wire_names__ = {{{}}}", entries.join(", ")))
    }
}

/// Build the `__baml_wire_values__` enum marker: a plain dunder dict
/// mapping each keyword-escaped Python member name to its raw BAML/wire variant
/// name, in declaration order. `None` when no member was escaped, so keyword-free
/// enums emit nothing and stay byte-identical. A dunder name is excluded from
/// `enum.Enum` membership by `EnumMeta`, so the assignment never becomes a
/// variant. `.py`-only (see `EnumBodyPy`).
fn build_wire_values_marker(variants: &[crate::emit::enum_::PyEnumVariant]) -> Option<String> {
    let entries: Vec<String> = variants
        .iter()
        .filter_map(|v| {
            v.wire_name
                .as_deref()
                .map(|raw| format!("{}: {}", py_string(&v.ident), py_string(raw)))
        })
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(format!("__baml_wire_values__ = {{{}}}", entries.join(", ")))
    }
}

/// Render one symbol into its `.py` source block, including trailing `\n`.
fn render_symbol(s: &EmittedSymbol, leaf: &LeafPath) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        // Runtime `.py`: callback Protocols are stub-only, so optional-arg
        // callables widen to `typing.Callable[..., R]` here.
        callback_protocols: None,
        // Set per-symbol below; only the class body translates field types.
        type_var_map: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
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
            debug_assert_identifier(&c.py_name);
            // Field annotations resolve TypeVars through this class's scope map.
            let ctx = TranslateCtx {
                type_var_map: scope_type_var_map(&c.type_var_map),
                ..ctx
            };
            let properties = c
                .properties
                .iter()
                .map(|prop| {
                    debug_assert_identifier(&prop.name);
                    ClassPropertyView {
                        name: prop.name.clone(),
                        ty_py: translate_ty(&prop.ty, &ctx),
                        field_expr: match &prop.alias {
                            Some(raw) => {
                                format!(" = pydantic.Field(alias={})", py_string(raw))
                            }
                            None => String::new(),
                        },
                    }
                })
                .collect();
            let populate_by_name = c.properties.iter().any(|p| p.alias.is_some());
            // Emit `__baml_wire_names__` ONLY when ≥1 field is
            // keyword-escaped (carries `alias`). Maps the escaped Python attr
            // name to its raw BAML/wire name so the bridge encoder can recover
            // the wire key without shape-guessing. Keyword-free classes
            // have no escaped field, so the marker is absent and their output is
            // byte-identical to today.
            let wire_names_marker = build_wire_names_marker(&c.properties);
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
                populate_by_name,
                wire_names_marker,
                properties,
                static_methods: build_method_line_views(&c.static_methods, &c.generic_params),
                instance_methods: build_method_line_views(&c.instance_methods, &c.generic_params),
            }
            .render()
            .expect("class_body template should always render");
            out.push('\n');
            out
        }
        EmittedSymbol::Enum(e) => {
            debug_assert_identifier(&e.py_name);
            let variants = e
                .variants
                .iter()
                .map(|v| {
                    debug_assert_identifier(&v.ident);
                    EnumVariantView {
                        ident: v.ident.clone(),
                        value: py_string(&v.value),
                    }
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
            // Emit `__baml_wire_values__` ONLY when ≥1 member is
            // keyword-escaped. Keyword-free enums have no escaped member, so the
            // marker is absent and their output is byte-identical to today.
            let wire_values_marker = build_wire_values_marker(&e.variants);
            let mut out = EnumBodyPy {
                py_name: e.py_name.clone(),
                docstring,
                variants,
                wire_values_marker,
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

/// Render a type alias to its source line. Shared between `.py` and
/// `.pyi`; the body is identical (12d §3.3).
///
/// Non-recursive aliases render as `Name: typing.TypeAlias = <RHS>`.
/// Recursive aliases (18c) render via `typing_extensions.TypeAliasType`
/// with inner self-references quoted, so Pydantic resolves them
/// through its JSON-schema definitions machinery instead of recursing.
fn render_type_alias(a: &crate::emit::type_alias::PyTypeAlias, leaf: &LeafPath) -> String {
    use askama::Template;

    debug_assert_identifier(&a.py_name);

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
        // Alias bodies are shared between `.py` and `.pyi`; a callable alias
        // with optional params widens to `typing.Callable[..., R]` rather than
        // referencing a stub-only Protocol.
        callback_protocols: None,
        // Type aliases declare no generic scope; any `TypeVar` in the RHS is
        // free and falls back to the stateless escape.
        type_var_map: None,
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
    // Free functions have no enclosing class, so only their own `<...>` params
    // (bound via `_types=`) participate.
    let generic_kwargs = render_generic_kwargs(&f.generic_params, &[]);
    format!(
        "{name}{lhs_pad} = _define_function({fqn}, {mode_str} {required_params}{optional_params}{generic_kwargs})",
        name = f.py_name,
        fqn = py_string(&f.baml_fqn),
    )
}

/// One method-binding line, indented for a class body. Mirrors
/// `render_factory_binding` modulo the factory alias and the
/// `staticmethod(...)` wrap on statics.
fn render_method_binding(m: &PyMethodBinding, class_generic_params: &[String]) -> String {
    let (lhs_pad, mode_str) = match m.mode {
        SyncAsync::Sync => ("      ", "\"sync\", "),
        SyncAsync::Async => ("", "\"async\","),
    };
    let required_params = m.runtime_required_names();
    let optional_params = m.optional_names();
    let required_params = render_param_list(&required_params);
    let optional_params = optional_param_list_arg(&optional_params);
    // Instance methods recover the enclosing class's TypeVars from the `self`
    // receiver; static methods have no receiver, so only their own `<...>`
    // params (via `_types=`) bind.
    let class_type_params: &[String] = match m.kind {
        MethodKind::Instance => class_generic_params,
        MethodKind::Static => &[],
    };
    let generic_kwargs = render_generic_kwargs(&m.generic_params, class_type_params);
    let inner = format!(
        "_define_function({fqn}, {mode_str} {required_params}{optional_params}{generic_kwargs})",
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

    let typevars = body.generic_typevars();
    if !typevars.is_empty() {
        out.push_str("\n\n");
        for tv in &typevars {
            debug_assert_identifier(tv);
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
    let mut typed_params = render_method_params_pyi(m, ctx);
    // A method with its OWN generic params (`pair_with<U>`, static `new<T>`)
    // requires the caller to bind them via a keyword-only `_types=` dict (the
    // class's TypeVars ride the receiver, not `_types=`). Mirror the runtime
    // requirement in the stub. Instance methods always have a `self` param, and
    // statics with own generics always have at least one value param, so
    // `typed_params` is never empty here.
    if !m.generic_params.is_empty() {
        if m.optional_args.is_empty() {
            typed_params.push_str(", *, _types: dict[str, type]");
        } else {
            // optionals already introduced the `*` keyword-only marker.
            typed_params.push_str(", _types: dict[str, type]");
        }
    }
    let ret_py = translate_ty(&m.return_ty, ctx);
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

/// `.pyi` counterpart of `build_method_line_views`. `class_map` is the enclosing
/// class's `TypeVar` scope map, merged per method (instance only) so a signature
/// resolves every in-scope `TypeVar` to its declared spelling.
fn build_method_block_views(
    methods: &[PyMethodBinding],
    base_ctx: &TranslateCtx,
    class_map: Option<&std::rc::Rc<TypeVarMap>>,
) -> Vec<MethodBlockView> {
    let mut out = Vec::with_capacity(methods.len());
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = source_method_root(&m.baml_fqn);
        let tight_to_prev = match prev_root {
            None => true,
            Some(p) => p == root,
        };
        let ctx = TranslateCtx {
            type_var_map: method_scope_type_var_map(m, class_map),
            ..base_ctx.clone()
        };
        out.push(MethodBlockView {
            block: render_method_block_pyi(m, &ctx),
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
    callback_protocols: Option<&std::rc::Rc<IndexMap<CallbackProtocolKey, String>>>,
) -> String {
    use askama::Template;
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: callback_protocols.cloned(),
        // Set per-symbol below (class fields, method and function signatures).
        type_var_map: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
            if let Some((module, rust_name)) = media_reexport_rust_name(c) {
                return format!(
                    "from {module} import {rust_name} as {py_name}\n",
                    py_name = c.py_name,
                );
            }
            let class_map = scope_type_var_map(&c.type_var_map);
            // Field annotations resolve TypeVars through the class scope.
            let field_ctx = TranslateCtx {
                type_var_map: class_map.clone(),
                ..ctx.clone()
            };
            let properties = c
                .properties
                .iter()
                .map(|prop| ClassPropertyView {
                    name: prop.name.clone(),
                    ty_py: translate_ty(&prop.ty, &field_ctx),
                    // The `.pyi` stub renders the escaped attribute name only;
                    // the `Field(alias=…)` runtime detail lives in the `.py`.
                    field_expr: String::new(),
                })
                .collect();
            let mut out = ClassBodyPyi {
                py_name: c.py_name.clone(),
                bases: render_class_bases(&c.generic_params),
                properties,
                static_methods: build_method_block_views(
                    &c.static_methods,
                    &ctx,
                    class_map.as_ref(),
                ),
                instance_methods: build_method_block_views(
                    &c.instance_methods,
                    &ctx,
                    class_map.as_ref(),
                ),
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
            // the `.py` definition. The `__baml_wire_values__` marker is a
            // runtime-only provenance detail (consumed by the bridge encoder),
            // so the stub carries `None` — mirroring how the `.pyi` drops the
            // class `Field(alias=…)` runtime detail.
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
                wire_values_marker: None,
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
            // The signature resolves TypeVars through this function's scope.
            let ctx = TranslateCtx {
                type_var_map: scope_type_var_map(&f.type_var_map),
                ..ctx
            };
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
    // A generic free function requires the caller to bind every TypeVar via a
    // keyword-only `_types=` dict (the runtime enforces this; the stub mirrors
    // it so type checkers flag a missing/positional binding). Methods get their
    // own surface in 01pt5.
    if !f.generic_params.is_empty() {
        let has_kwonly_marker = f.arg_defaults.iter().any(Option::is_some);
        if has_kwonly_marker {
            // optionals already introduced a `*` keyword-only marker.
            typed_params.push_str(", _types: dict[str, type]");
        } else if typed_params.is_empty() {
            typed_params.push_str("*, _types: dict[str, type]");
        } else {
            typed_params.push_str(", *, _types: dict[str, type]");
        }
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
    callback_protocols: Option<&std::rc::Rc<IndexMap<CallbackProtocolKey, String>>>,
) -> String {
    let parent_ctx = TranslateCtx {
        current_leaf: parent_leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: callback_protocols.cloned(),
        type_var_map: scope_type_var_map(&parent_fn.type_var_map),
    };
    let child_base_ctx = TranslateCtx {
        current_leaf: child_body.leaf.clone(),
        self_ref: None,
        defer_name_refs: false,
        callback_protocols: callback_protocols.cloned(),
        // Set per-child function below.
        type_var_map: None,
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
            let child_ctx = TranslateCtx {
                type_var_map: scope_type_var_map(&f.type_var_map),
                ..child_base_ctx.clone()
            };
            out.push_str(&render_protocol_function_method_pyi(
                &f.py_name, f, &child_ctx,
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

/// KEYWORD-GATED: rel-imports needed by the callable-child
/// function signatures that `render_callable_child_protocol_pyi` renders into the
/// PARENT stub, computed relative to the PARENT leaf so depths/anchors match the
/// parent stub (the child's own `all_rel_imports_py` would use child-leaf depths —
/// one level too deep). Only child *functions* are rendered into the parent, so
/// only their arg/return types are walked.
///
/// The walk is gated to keyword-ESCAPED class/enum/alias anchors only
/// (`collect_keyword_escaped_root_imports`): a keyword-free callable child (one
/// returning an ordinary root class `Widget`) contributes NOTHING, so keyword-free
/// parent stubs stay strictly byte-identical, while keyword-escaped anchors —
/// the only names that can collide with a bumped keyword `TypeVar` — are
/// still aggregated and bound. The general keyword-free unbound-stub case an
/// unconditional walk would also fix is pre-existing on canary and is left as a
/// disclosed follow-up. Provenance is unambiguous at codegen (the arg/return `Ty`
/// carries the class's raw BAML name), unlike the bridge's runtime shape ambiguity.
/// Isolated to this one fn + its two callers (the render site and the reservation
/// pre-pass) so the whole aggregation can be dropped
/// deterministically if the keyword-free fixtures move.
fn callable_child_parent_rel_imports(
    parent_leaf: &LeafPath,
    callable_child_bodies: &BTreeMap<String, &LeafBody>,
) -> Vec<RelImport> {
    let mut acc = RootImportSets::default();
    for child_body in callable_child_bodies.values() {
        for (sym, _) in &child_body.symbols {
            if let EmittedSymbol::Function(f) = sym {
                for ty in &f.arg_tys {
                    collect_keyword_escaped_root_imports(ty, parent_leaf, &mut acc);
                }
                collect_keyword_escaped_root_imports(&f.return_ty, parent_leaf, &mut acc);
            }
        }
    }
    acc.into_rel()
}

/// Structural mirror of `collect_root_imports` restricted to root-routed
/// references whose RAW BAML name is a Python hard keyword — i.e. the anchor the
/// translator emits is a generated keyword-escape (`None`→`None_`). Every other
/// reference (keyword-free classes, module-segment routes, `Ty::Media`,
/// primitives) is intentionally skipped so the aggregation is a no-op for
/// keyword-free schemas. Recurses through the same container/callable shapes as
/// `collect_root_imports` so a keyword class nested in `List[..]`/`Union[..]`/a
/// callback signature is still found.
fn collect_keyword_escaped_root_imports(ty: &Ty, current: &LeafPath, out: &mut RootImportSets) {
    match ty {
        Ty::Class(name, args, _) => {
            record_keyword_escaped_name_routing(name, current, out);
            for a in args {
                collect_keyword_escaped_root_imports(a, current, out);
            }
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) | Ty::TypeAlias(name, _) => {
            record_keyword_escaped_name_routing(name, current, out);
        }
        Ty::List(inner, _) => collect_keyword_escaped_root_imports(inner, current, out),
        Ty::Map { key, value, .. } => {
            collect_keyword_escaped_root_imports(key, current, out);
            collect_keyword_escaped_root_imports(value, current, out);
        }
        Ty::Union(items, _) => {
            for item in items {
                collect_keyword_escaped_root_imports(item, current, out);
            }
        }
        Ty::Function { params, ret, .. } => {
            for p in params {
                collect_keyword_escaped_root_imports(&p.ty, current, out);
            }
            collect_keyword_escaped_root_imports(ret, current, out);
        }
        // Everything else can never be a keyword-escaped root-class anchor:
        // `Ty::Media` anchors on module segment `baml` (not keyword-escaped),
        // module-segment routing for keyword namespaces is a disclosed follow-up
        // (`sanitize_python_module_segment`), and primitives/`TypeVar`/`RustType`
        // route nothing.
        _ => {}
    }
}

/// `record_name_routing` restricted to root-routed references whose RAW BAML name
/// is a Python hard keyword. Only the root-routed branch can produce a keyword-
/// escaped bare anchor (`None`→`None_`); a non-empty `routed.segments` is a
/// module-segment route (never keyword-escaped today, disclosed follow-up) and is
/// skipped. Mirrors the escaped-spelling contract of `record_name_routing`'s
/// root branch so the reserved anchor and the emitted `from ... import None_`
/// stay identical.
fn record_keyword_escaped_name_routing(
    name: &baml_codegen_types::Name,
    current: &LeafPath,
    out: &mut RootImportSets,
) {
    let routed = route_class_ref(name);
    if routed == *current {
        return;
    }
    if routed.segments.is_empty()
        && !current.segments.is_empty()
        && crate::emit::is_python_hard_keyword(name.bare_name())
    {
        let bare = escape_python_keyword(name.bare_name().to_string());
        out.root_names.insert(bare.clone());
        out.rel.insert(RelImport {
            depth: current.segments.len() + 1,
            from_path: String::new(),
            anchor: bare,
        });
    }
}

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
    // A callable child's function signatures are
    // rendered into THIS parent stub (see `render_callable_child_protocol_pyi`),
    // but their cross-leaf dependencies were never aggregated into the parent's
    // guarded imports — a pre-existing gap the keyword escaping made visible. This
    // block + the `callable_child_parent_rel_imports` fn are the entire fix, kept
    // isolated so they can be reverted as one unit. Reverting is clean:
    // `all_rel_imports_py()` already returns a sorted, deduped vec.
    {
        rel_imports.extend(callable_child_parent_rel_imports(
            &body.leaf,
            callable_child_bodies,
        ));
        rel_imports.sort();
        rel_imports.dedup();
    }
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
            debug_assert_identifier(tv);
            writeln!(out, "{tv} = typing.TypeVar(\"{tv}\")").unwrap();
        }
    }

    // Callback Protocols: each optional-argument callable in the leaf gets a
    // `typing.Protocol` with a `__call__` carrying its precise signature, named
    // `_<owner>__<param><n>`. Signatures below reference these by name. Emitted
    // before the symbols that use them; the shared map lets nested callbacks
    // resolve to each other's Protocol names.
    //
    // Distinct Protocols are keyed on the RENDERED identity — the structural
    // callback `Ty` plus the owner-map projection over the TypeVars it names — so
    // two owners whose maps produce different spellings (a bumped `{None, None_}`
    // twin) get distinct Protocols, while identical renderings (every keyword-free
    // schema) still share ONE Protocol keyed the same way, keeping output
    // byte-identical. `translate_ty` re-derives this key from `ctx.type_var_map`
    // at each reference, so the shared map needs no per-owner threading.
    let protocol_entries = body.callback_protocols();
    // `render_list` holds one distinct Protocol each: (name, callback Ty, owner
    // map to render its body under), in first-seen order.
    let mut render_list: Vec<(String, Ty, Option<std::rc::Rc<TypeVarMap>>)> = Vec::new();
    let mut base_counts: IndexMap<String, usize> = IndexMap::new();
    let mut name_by_key: IndexMap<CallbackProtocolKey, String> = IndexMap::new();
    for cp in &protocol_entries {
        let key = callback_protocol_key(&cp.ty, cp.owner_map.as_deref());
        if name_by_key.contains_key(&key) {
            continue;
        }
        // `n` is a per-base 1-based counter (usually 1; >1 only when one
        // parameter's type nests multiple optional-arg callables under the same
        // `_<owner>__<param>` prefix).
        let n = base_counts.entry(cp.base.clone()).or_insert(0);
        *n += 1;
        let name = format!("{}{}", cp.base, n);
        name_by_key.insert(key, name.clone());
        render_list.push((name, cp.ty.clone(), cp.owner_map.clone()));
    }
    let callback_protocols: Option<std::rc::Rc<IndexMap<CallbackProtocolKey, String>>> =
        if name_by_key.is_empty() {
            None
        } else {
            Some(std::rc::Rc::new(name_by_key))
        };
    if let Some(map) = &callback_protocols {
        for (name, ty, owner_map) in &render_list {
            if let Ty::Function { params, ret, .. } = ty {
                let proto_ctx = TranslateCtx {
                    current_leaf: body.leaf.clone(),
                    self_ref: None,
                    defer_name_refs: false,
                    callback_protocols: Some(map.clone()),
                    // Render the Protocol body under the OWNER scope's map so a
                    // bumped keyword TypeVar (`None`→`None__`) matches the owner's
                    // declared spelling; a non-generic owner carries `None`, which
                    // falls back to the stateless escape (byte-identical).
                    type_var_map: owner_map.clone(),
                };
                out.push_str("\n\n");
                out.push_str(&render_callback_protocol(name, params, ret, &proto_ctx));
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
            ));
        }
    }

    out.push_str("\n\n");

    let mut prev: Option<(&SortKey, &EmittedSymbol)> = None;
    for (sym, key) in &body.symbols {
        if let EmittedSymbol::Function(f) = sym
            && f.mode == SyncAsync::Sync
            && callable_child_bodies.contains_key(&f.py_name)
        {
            continue;
        }
        let body_text = render_symbol_pyi(sym, &body.leaf, callback_protocols.as_ref());
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

    out
}
