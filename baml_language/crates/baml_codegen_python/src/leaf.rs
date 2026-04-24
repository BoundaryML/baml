//! Per-leaf symbol bundle (`LeafBody`) and leaf renderer
//! (`render_leaf_body`). One `LeafBody` per populated leaf file, pre-
//! sorted at build time so the renderer is a straight walk — no
//! ordering logic at render time.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{
    emit::{EmittedSymbol, SortKey},
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
    let mut out: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
    for (leaf, mut pairs) in buckets {
        pairs.sort_by(|a, b| a.1.cmp(&b.1));
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

/// Render a single symbol into one or more Python source lines.
/// Empty `Vec` means "nothing to emit" (used for the unreachable
/// static/instance method variants G2–G4 don't produce).
fn render_symbol(s: &EmittedSymbol, leaf: &LeafPath) -> Vec<String> {
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: None,
    };

    match s {
        EmittedSymbol::Class(c) => {
            let mut lines = Vec::with_capacity(2 + c.properties.len());
            lines.push(format!("class {}(pydantic.BaseModel):", c.py_name));
            lines.push("    model_config = pydantic.ConfigDict(extra=\"forbid\")".to_string());
            for prop in &c.properties {
                lines.push(format!(
                    "    {}: {}",
                    prop.name,
                    translate_ty(&prop.ty, &ctx)
                ));
            }
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
        EmittedSymbol::Function(f) => vec![format!("{} = None", f.py_name)],
        // G4 produces no StaticMethod / InstanceMethod instances; the
        // unreachable arm returns empty so downstream separator logic
        // treats them as no-ops.
        EmittedSymbol::StaticMethod(_) | EmittedSymbol::InstanceMethod(_) => Vec::new(),
    }
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
    if !stdlibs.is_empty() || needs_pydantic {
        out.push('\n');
        for lib in &stdlibs {
            writeln!(out, "import {lib}").unwrap();
        }
        if needs_pydantic {
            out.push_str("import pydantic\n");
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
