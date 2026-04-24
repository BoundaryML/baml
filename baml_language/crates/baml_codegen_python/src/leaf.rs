//! Per-leaf symbol bundle (`LeafBody`) and leaf renderer
//! (`render_leaf_body`). One `LeafBody` per populated leaf file, pre-
//! sorted at build time so the renderer is a straight walk — no
//! ordering logic at render time.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{
    emit::{EmittedSymbol, SortKey},
    routing::LeafPath,
};

/// All symbols that land in one leaf's body, in final render order.
/// Each entry keeps its `SortKey` so the renderer can group function
/// fan-out siblings (which share their parent's sort key) tightly
/// while separating unrelated top-level definitions with the usual
/// PEP-8 two-blank-line gap.
pub(crate) struct LeafBody {
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
        if self
            .symbols
            .iter()
            .any(|(s, _)| matches!(s, EmittedSymbol::TypeAlias(_)))
        {
            out.push("typing");
        }
        out
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
        out.insert(leaf, LeafBody { symbols: pairs });
    }
    out
}

fn render_symbol(s: &EmittedSymbol) -> String {
    match s {
        EmittedSymbol::Class(c) => format!("class {}: pass", c.py_name),
        EmittedSymbol::Enum(e) => format!("class {}(str, enum.Enum): pass", e.py_name),
        EmittedSymbol::TypeAlias(a) => {
            format!("{}: typing.TypeAlias = typing.Any", a.py_name)
        }
        EmittedSymbol::Function(f) => format!("{} = None", f.py_name),
        // G2 produces no StaticMethod / InstanceMethod instances;
        // render blank so the match stays exhaustive without
        // introducing Python syntax. If one slips through, the empty
        // line is a visible anomaly.
        EmittedSymbol::StaticMethod(_) | EmittedSymbol::InstanceMethod(_) => String::new(),
    }
}

/// Render a leaf's body section (stdlib imports + symbol lines +
/// `__all__`). Returns empty string if the leaf has no symbols.
///
/// Shape (non-empty):
///
/// ```text
/// [blank]
/// import enum       (only if the leaf has a PyEnum)
/// import typing     (only if the leaf has a PyTypeAlias)
/// [blank]
/// [blank]
/// <symbol line>
/// [blank × 2 between top-level groups; 0 between function fan-out siblings]
/// <symbol line>
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
    if !stdlibs.is_empty() {
        out.push('\n');
        for lib in &stdlibs {
            writeln!(out, "import {lib}").unwrap();
        }
    }

    // Two blank lines before the first symbol group.
    out.push_str("\n\n");

    let mut prev: Option<(&SortKey, &EmittedSymbol)> = None;
    for (sym, key) in &body.symbols {
        let line = render_symbol(sym);
        if line.is_empty() {
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
        out.push_str(&line);
        out.push('\n');
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
