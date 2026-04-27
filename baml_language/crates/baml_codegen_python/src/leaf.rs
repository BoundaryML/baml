//! Per-leaf symbol bundle (`LeafBody`) and leaf renderer
//! (`render_leaf_body`). One `LeafBody` per populated leaf file, pre-
//! sorted at build time so the renderer is a straight walk — no
//! ordering logic at render time.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{
    emit::{
        EmittedSymbol, SortKey,
        function::SyncAsync,
        method::{MethodKind, PyMethodBinding},
    },
    py_string,
    routing::LeafPath,
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
            lines.push(format!("class {}(pydantic.BaseModel):", c.py_name));
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

    let stdlibs = body.stdlib_imports();
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
