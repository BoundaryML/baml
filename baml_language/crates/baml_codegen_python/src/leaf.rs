//! Per-leaf symbol bundle (`LeafBody`) and leaf renderer
//! (`render_leaf_body`). One `LeafBody` per populated leaf file, pre-
//! sorted at build time so the renderer is a straight walk — no
//! ordering logic at render time.

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
/// fan-out siblings (which share their parent's sort key) tightly
/// while separating unrelated top-level definitions with the usual
/// PEP-8 two-blank-line gap.
pub(crate) struct LeafBody {
    pub(crate) leaf: LeafPath,
    pub(crate) symbols: Vec<(EmittedSymbol, SortKey)>,
}

impl LeafBody {
    /// Every public `py_name` in this leaf, in render order — the
    /// exact contents of the trailing `__all__` list.
    pub(crate) fn all_names(&self) -> Vec<&str> {
        self.symbols
            .iter()
            .map(|(s, _)| s.py_name())
            // Guard against underscore-prefixed names (G4+ may
            // introduce some); G2 has no such names today.
            .filter(|n| !n.starts_with('_'))
            .collect()
    }

    /// Which stdlib imports this leaf needs in its header.
    pub(crate) fn stdlib_imports(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self
            .symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Enum(_)))
        {
            out.push("enum");
        }
        // Any class body has `typing.Optional` / `typing.List` / etc.
        // candidates in its field types, and type aliases always need
        // `typing.TypeAlias`. Be generous: if any class or alias is
        // present, import `typing`.
        if self
            .symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Class(_) | EmittedSymbol::TypeAlias(_)))
        {
            out.push("typing");
        }
        out
    }

    /// Whether this leaf needs `import pydantic`.
    pub(crate) fn needs_pydantic(&self) -> bool {
        self.symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Class(_)))
    }

    /// Whether this leaf needs `from baml.baml_core import
    /// define_function as _define_function`. True when any factory
    /// binding (free function or companion) routes here.
    pub(crate) fn needs_define_function(&self) -> bool {
        self.symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::Function(_)))
    }

    /// Whether this leaf has any class with at least one static method.
    /// Drives `from baml.baml_core import define_static_method`.
    pub(crate) fn needs_define_static_method(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => !c.static_methods.is_empty(),
            _ => false,
        })
    }

    /// Whether this leaf has any class with at least one instance method.
    /// Drives `from baml.baml_core import define_instance_method`.
    pub(crate) fn needs_define_instance_method(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Class(c) => !c.instance_methods.is_empty(),
            _ => false,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// First segments of every cross-leaf routed module path referenced
    /// by any type expression that contributes to this leaf's `.py`,
    /// deduped and sorted.
    ///
    /// Per 12f §4.1: walks class fields, function/method param+return
    /// types, and type-alias RHS. Function/method types don't render
    /// annotations in the `.py` (factory bindings only), but they're
    /// included for parity with `.pyi` so the import block is identical
    /// across the two files where the underlying type sources match.
    /// The `TYPE_CHECKING` guard makes the extras free at runtime.
    pub(crate) fn cross_leaf_first_segments_py(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
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

    /// First segments for the `.pyi` companion. Walks function /
    /// method param+return types and type-alias RHS — the type sources
    /// that mirror into the typed surface. Class fields are not
    /// mirrored into `.pyi` (12d §3.1), so they don't contribute.
    pub(crate) fn cross_leaf_first_segments_pyi(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        let current = &self.leaf;
        for (sym, _) in &self.symbols {
            match sym {
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

    /// Whether this leaf's `.pyi` needs `import typing`. Per 12d §7,
    /// any rendered signature (function, method, or type alias) pulls
    /// in `typing` — type aliases declare `typing.TypeAlias` and
    /// signatures may use `typing.Optional` / `typing.List` / etc.
    /// Class field types are not mirrored into the `.pyi` (12d §3.1),
    /// so a property-only class on its own does not require `typing`.
    /// Generic classes pull in `typing` for the `typing.Generic[T]`
    /// base and the leaf-level `typing.TypeVar("T")` declarations.
    pub(crate) fn needs_typing_pyi(&self) -> bool {
        self.symbols.iter().any(|(s, _)| match s {
            EmittedSymbol::Function(_) | EmittedSymbol::TypeAlias(_) => true,
            EmittedSymbol::Class(c) => {
                !c.static_methods.is_empty()
                    || !c.instance_methods.is_empty()
                    || !c.generic_params.is_empty()
            }
            EmittedSymbol::Enum(_) => false,
        })
    }

    /// Union of `TypeVar` identifiers declared by any generic definition
    /// routed to this leaf — classes, functions, and methods. Each name
    /// appears once; the result is alphabetic so emission order is
    /// deterministic. Per `13a` §4.2, the leaf renderer turns each entry
    /// into a `T = typing.TypeVar("T")` line at the top of the file.
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

/// Walk a `Ty` and add the first routed segment of every `Name`-bearing
/// reference (class / enum / type alias) whose routed leaf differs from
/// `current`. The empty-routed-leaf case (root-leaf reference from a
/// non-root leaf) emits no import — the translator already renders such
/// references as a bare name without prefix.
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
        Ty::Int
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Literal(_)
        | Ty::Uint8Array
        | Ty::Media(_)
        | Ty::TypeVar(_)
        | Ty::BuiltinUnknown
        | Ty::Unit
        | Ty::BamlOptions => {}
    }
}

/// Group `(leaf, symbol, sort_key)` triples into a `LeafBody` per
/// leaf, sorted by `SortKey` within each leaf.
pub(crate) fn group_and_sort(
    triples: Vec<(LeafPath, EmittedSymbol, SortKey)>,
) -> BTreeMap<LeafPath, LeafBody> {
    let mut buckets: BTreeMap<LeafPath, Vec<(EmittedSymbol, SortKey)>> = BTreeMap::new();
    for (leaf, sym, key) in triples {
        buckets.entry(leaf).or_default().push((sym, key));
    }

    // Stable sort preserves the intra-parent function fan-out order
    // established in `expand_function` (base sync, base async,
    // companions each sync/async).
    //
    // Tertiary tie-breaker: when sort keys collide (PPIR assigns
    // synthetic `$stream` symbols `TextRange::default()` so they all
    // share span 0), emit type aliases last. The alias's RHS may
    // reference stream classes that live in the same leaf, and a class
    // must be defined before the alias's RHS evaluates.
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

/// Tertiary sort ordinal: when `SortKey` ties (e.g. PPIR-emitted
/// stream symbols all share span 0), emit type aliases last. Other
/// kinds keep their incoming order via stable sort.
fn symbol_kind_ord(sym: &EmittedSymbol) -> u8 {
    match sym {
        EmittedSymbol::TypeAlias(_) => 1,
        _ => 0,
    }
}

/// Render the base-class list for a class definition. Non-generic
/// classes render as just `pydantic.BaseModel`; generic classes append
/// `, typing.Generic[T, …]` per `13a` §4.1. The list mirrors itself
/// across `.py` and `.pyi` so a class's interface declaration is
/// consistent.
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

/// Render a single symbol into one or more Python source lines.
fn render_symbol(s: &EmittedSymbol, leaf: &LeafPath) -> Vec<String> {
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
            let total_method_lines = c.static_methods.len() + c.instance_methods.len();
            let mut lines = Vec::with_capacity(2 + c.properties.len() + total_method_lines + 2);
            let bases = render_class_bases(&c.generic_params);
            lines.push(format!("class {}({bases}):", c.py_name));
            lines.push("    model_config = pydantic.ConfigDict(extra=\"forbid\")".to_string());
            for prop in &c.properties {
                lines.push(format!(
                    "    {}: {}",
                    prop.name,
                    translate_ty(&prop.ty, &ctx)
                ));
            }
            push_method_block(&mut lines, &c.static_methods);
            push_method_block(&mut lines, &c.instance_methods);
            lines
        }
        EmittedSymbol::Enum(e) => {
            let mut lines = Vec::with_capacity(1 + e.variants.len().max(1));
            lines.push(format!("class {}(str, enum.Enum):", e.py_name));
            if e.variants.is_empty() {
                // BAML forbids empty enums at parse time; emit a
                // defensive `pass` if the IR somehow produces one.
                lines.push("    pass".to_string());
            } else {
                for v in &e.variants {
                    lines.push(format!("    {} = {}", v.ident, py_string(&v.value)));
                }
            }
            lines
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
            vec![format!("{}: typing.TypeAlias = {}", a.py_name, rhs)]
        }
        EmittedSymbol::Function(f) => vec![render_factory_binding(f)],
    }
}

/// Append a class's method block to `lines`. Each source-declared
/// method's sync/async/companion fan-out is contiguous; consecutive
/// distinct source methods are separated by a blank line, matching the
/// shape used for free-function fan-out at the leaf level.
fn push_method_block(lines: &mut Vec<String>, methods: &[PyMethodBinding]) {
    if methods.is_empty() {
        return;
    }
    // One blank line separates the method block from the property
    // block above (or from a preceding method block of a different
    // kind).
    lines.push(String::new());
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = source_method_root(&m.baml_fqn);
        if let Some(prev) = prev_root {
            // Distinct source method → blank line. Sync/async pair and
            // parent+companion siblings share `source_method_root`, so
            // they stay tight.
            if prev != root {
                lines.push(String::new());
            }
        }
        lines.push(render_method_binding(m));
        prev_root = Some(root);
    }
}

/// FQN prefix up to the first `$` (exclusive), or the whole FQN if
/// there's no `$`. Sync/async pairs of a parent method share this
/// prefix; companions of that parent also share it, since their FQN
/// is `<parent_fqn>$<suffix>`.
fn source_method_root(fqn: &str) -> &str {
    match fqn.find('$') {
        Some(i) => &fqn[..i],
        None => fqn,
    }
}

/// Render one factory-binding line per `PyFunction`. Sync and async
/// stubs share the same FQN and `param_names`; only the LHS name
/// padding and the mode literal differ.
///
/// Layout matches 09b §3 / G5 §5.1:
///
/// ```text
/// foo       = _define_function("<fqn>", "sync",  [<params>])
/// foo_async = _define_function("<fqn>", "async", [<params>])
/// ```
///
/// The sync LHS is right-padded with `len("_async")` spaces so its
/// `=` aligns with the async sibling's. The mode literal is right-
/// padded so the `[<params>]` argument starts at the same column for
/// both modes.
///
/// The single-underscore alias (rather than double) avoids Python's
/// class-body name mangling (PEP 8): a `__name`-prefixed reference
/// inside a `class` body is textually replaced with `_ClassName__name`,
/// breaking method-binding emission. Single underscore is unmangled
/// and still excluded from `from module import *`.
fn render_factory_binding(f: &crate::emit::function::PyFunction) -> String {
    // 6 = len("_async"); makes the sync LHS column-equal to its async
    // sibling without inspecting the sibling at render time.
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

/// Render a single static or instance method binding line. Sits one
/// indentation level deep inside the owning class body; layout
/// mirrors the free-function `_define_function(...)` form modulo the
/// factory alias and the `staticmethod(...)` wrap on static methods.
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
    let rhs = match m.kind {
        // The `staticmethod(...)` wrap is required so Python's
        // descriptor protocol doesn't pass the class as positional
        // arg 0 when the method is invoked through the class.
        MethodKind::Static => format!("staticmethod({inner})"),
        // Instance methods rely on the descriptor protocol — Python
        // auto-passes the receiver as positional arg 0 when the method
        // is reached through an instance. No wrap.
        MethodKind::Instance => inner,
    };
    format!("    {name}{lhs_pad} = {rhs}", name = m.py_name)
}

/// Render a parameter-name list as a Python list literal.
/// `[]` for empty, `["a", "b", ...]` otherwise.
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

/// Render a leaf's body section (imports + symbol bodies + `__all__`).
/// Returns empty string if the leaf has no symbols.
///
/// Shape (non-empty):
///
/// ```text
/// [blank]
/// import enum          (if the leaf has a PyEnum)
/// import typing        (if the leaf has a PyClass or PyTypeAlias)
/// import pydantic      (if the leaf has a PyClass)
/// [blank]
/// [blank]
/// <symbol body>
/// [blank × 2 between top-level groups; 0 between function fan-out siblings]
/// <symbol body>
/// [blank]
/// [blank]
/// __all__ = [
///     "X",
///     ...
/// ]
/// ```
///
/// Two blank lines between distinct top-level definitions follows
/// PEP 8. Function fan-out siblings share their parent's sort key, so
/// we tighten them into a contiguous block.
pub(crate) fn render_leaf_body(body: &LeafBody) -> String {
    if body.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    let mut stdlibs = body.stdlib_imports();
    let cross_leaf_segments = body.cross_leaf_first_segments_py();
    // Cross-leaf TYPE_CHECKING blocks reference `typing.TYPE_CHECKING`,
    // so a leaf whose only typing usage is the cross-leaf block (e.g.
    // a function-only leaf with a cross-leaf parameter type) still
    // needs `import typing`. `stdlib_imports` returns `["enum"?,
    // "typing"?]` in alphabetical order; appending keeps that order
    // because "typing" sorts after "enum".
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
    // imports, wrapped in `if typing.TYPE_CHECKING:`. The guard keeps
    // them out of the runtime import graph — required so recursive
    // cross-leaf type references (leaf A → leaf B → leaf A) don't
    // create an import cycle. `from __future__ import annotations`
    // (already in every header) makes the type annotations resolve
    // lazily as strings, so type checkers see the imports while
    // runtime never executes them.
    //
    // Dot count = depth + 1: anchors every cross-leaf import at the
    // `baml_sdk/` root regardless of what absolute name the package
    // ends up with after install. One `from <dots> import <name>` per
    // first-segment, alphabetically sorted by the BTreeSet collector.
    if !cross_leaf_segments.is_empty() {
        out.push('\n');
        out.push_str("if typing.TYPE_CHECKING:\n");
        let dots = ".".repeat(body.leaf.segments.len() + 1);
        for seg in &cross_leaf_segments {
            writeln!(out, "    from {dots} import {seg}").unwrap();
        }
    }
    // Factory imports come after stdlib/pydantic, before the body —
    // per 09b §9 / 09b2 §3 they're absolute (not relative) and aliased
    // with a single-underscore prefix so they're private to the module.
    // (Double underscore would trigger Python's class-body name
    // mangling in method-binding emission, which prepends the class
    // name to any `__name` reference and breaks the binding.)
    // The set of factories imported per leaf is the minimal one — only
    // those actually referenced by the leaf's bindings.
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
    // Alphabetized by original name. `define_function` < `define_instance_method`
    // < `define_static_method` lexicographically, so the push order above
    // already produces the alphabetized form.
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

    // Per-leaf TypeVar declarations (13a §4.2). One `T = typing.TypeVar("T")`
    // line per TypeVar declared by any generic definition routed here.
    // Alphabetic order, deterministic. The block sits between the import
    // section and the first symbol body, separated by the same two-blank
    // gap used elsewhere.
    let typevars = body.generic_typevars();
    if !typevars.is_empty() {
        out.push_str("\n\n");
        for tv in &typevars {
            writeln!(out, "{tv} = typing.TypeVar(\"{tv}\")").unwrap();
        }
    }

    // Two blank lines before the first symbol group.
    out.push_str("\n\n");

    let mut prev: Option<(&SortKey, &EmittedSymbol)> = None;
    for (sym, key) in &body.symbols {
        let lines = render_symbol(sym, &body.leaf);
        if lines.is_empty() {
            continue;
        }
        match prev {
            None => {}
            Some((p, prev_sym))
                if p == key
                    && matches!(prev_sym, EmittedSymbol::Function(_))
                    && matches!(sym, EmittedSymbol::Function(_)) =>
            {
                // Function fan-out: siblings share their parent's
                // sort key and render contiguously.
            }
            Some(_) => {
                // New top-level definition — PEP-8 two blanks.
                out.push_str("\n\n");
            }
        }
        for line in &lines {
            out.push_str(line);
            out.push('\n');
        }
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

/// Render a single symbol into one or more `.pyi` source lines. Per
/// 12d §3 the body of class and enum stubs is name-only with `...`;
/// type aliases mirror the `.py` shape; functions render as typed
/// `def`/`async def` signatures with `...` bodies.
fn render_symbol_pyi(s: &EmittedSymbol, leaf: &LeafPath) -> Vec<String> {
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
            let total = c.static_methods.len() + c.instance_methods.len();
            let bases = render_class_bases(&c.generic_params);
            if total == 0 {
                vec![format!("class {}({bases}): ...", c.py_name)]
            } else {
                let mut lines = Vec::with_capacity(1 + total * 2);
                lines.push(format!("class {}({bases}):", c.py_name));
                push_method_signatures_pyi(&mut lines, &c.static_methods, &ctx);
                if !c.static_methods.is_empty() && !c.instance_methods.is_empty() {
                    lines.push(String::new());
                }
                push_method_signatures_pyi(&mut lines, &c.instance_methods, &ctx);
                lines
            }
        }
        EmittedSymbol::Enum(e) => {
            vec![format!("class {}(str, enum.Enum): ...", e.py_name)]
        }
        EmittedSymbol::TypeAlias(a) => {
            // Type-alias body is identical to the `.py` form per
            // 12d §3.3 — same `translate_ty` call and the same whole-
            // RHS single-quote wrap for recursive aliases.
            let rhs = translate_ty(&a.resolves_to, &ctx);
            let rhs = if a.recursive { format!("'{rhs}'") } else { rhs };
            vec![format!("{}: typing.TypeAlias = {}", a.py_name, rhs)]
        }
        EmittedSymbol::Function(f) => vec![render_function_signature_pyi(f, &ctx)],
    }
}

/// Render one method signature line (or two: decorator + signature for
/// statics) inside a class body. Each `PyMethodBinding` corresponds to
/// one fan-out entry — sync, async, companion sync, and companion
/// async are each rendered as their own typed signature.
fn push_method_signatures_pyi(
    lines: &mut Vec<String>,
    methods: &[PyMethodBinding],
    ctx: &TranslateCtx,
) {
    let mut prev_root: Option<&str> = None;
    for m in methods {
        let root = source_method_root(&m.baml_fqn);
        if let Some(prev) = prev_root {
            // Distinct source method → blank separator. Sync/async pair
            // and parent+companion siblings share `source_method_root`,
            // so they stay tight.
            if prev != root {
                lines.push(String::new());
            }
        }
        if matches!(m.kind, MethodKind::Static) {
            lines.push("    @staticmethod".to_string());
        }
        let async_kw = if matches!(m.mode, SyncAsync::Async) {
            "async "
        } else {
            ""
        };
        let typed_params = render_method_params_pyi(m, ctx);
        let ret_py = translate_ty(&m.return_ty, ctx);
        lines.push(format!(
            "    {async_kw}def {name}({typed_params}) -> {ret_py}: ...",
            name = m.py_name
        ));
        prev_root = Some(root);
    }
}

/// Render the parameter list of a method as `name: ty, …`. For
/// instance methods the leading `self` (no annotation) is taken from
/// `param_names[0]` and the remaining names are zipped with `arg_tys`;
/// for static methods names and types are zipped 1:1.
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

/// Render a free-function signature line. The async-ness lives in the
/// `def` keyword (per 12d §3.4); the return annotation is the same
/// for sync and async fan-out siblings.
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

/// Zip parameter names with their translated Python type expressions
/// into a comma-separated `name: ty, …` list. Empty when both slices
/// are empty.
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

/// Render a leaf's `.pyi` body section (imports + symbol bodies +
/// `__all__`). Returns empty string if the leaf has no symbols.
///
/// Layout mirrors `render_leaf_body` modulo the differences spelled out
/// in 12d §7:
/// - `from baml.baml_core import …` factory imports are omitted
/// - `import typing` is needed if any signature is present (function,
///   method, or type alias) — see `LeafBody::needs_typing_pyi`
/// - `import enum` and `import pydantic` follow the same rule as `.py`
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
    // Same rule as `.py`: typing is needed if any signature is rendered
    // OR the cross-leaf block fires. The block uses
    // `typing.TYPE_CHECKING`, so the import has to be in scope even if
    // no signature would otherwise pull it in.
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

    // Cross-leaf TYPE_CHECKING block — same shape as `.py` per 12f §4.2.
    // Mirroring keeps `.py` and `.pyi` easy to diff; the guard is a
    // no-op in `.pyi` (already type-check-only) but matters in `.py`.
    if !cross_leaf_segments.is_empty() {
        out.push('\n');
        out.push_str("if typing.TYPE_CHECKING:\n");
        let dots = ".".repeat(body.leaf.segments.len() + 1);
        for seg in &cross_leaf_segments {
            writeln!(out, "    from {dots} import {seg}").unwrap();
        }
    }

    // Per-leaf TypeVar declarations (13a §4.2). The `.pyi` re-declares
    // them because stubs don't import from sibling `.py` files —
    // self-contained for static checkers.
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
        let lines = render_symbol_pyi(sym, &body.leaf);
        if lines.is_empty() {
            continue;
        }
        match prev {
            None => {}
            Some((p, prev_sym))
                if p == key
                    && matches!(prev_sym, EmittedSymbol::Function(_))
                    && matches!(sym, EmittedSymbol::Function(_)) =>
            {
                // Function fan-out: siblings share their parent's
                // sort key and render contiguously.
            }
            Some(_) => {
                out.push_str("\n\n");
            }
        }
        for line in &lines {
            out.push_str(line);
            out.push('\n');
        }
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
