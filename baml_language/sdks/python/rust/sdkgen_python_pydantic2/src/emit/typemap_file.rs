//! Codegen-emitted `baml_sdk/_typemap.py` (25a2 §4.1, 25b2 Phase 2).
//!
//! Walks the grouped `LeafBody` map and renders three literal dicts of
//! `FQN → (module_path, attr_name)` lazy entries, plus the
//! `BamlTypeMap.from_lazy_entries(...)` call that installs the populated
//! map. The runtime `_typemap` module is imported by the root init,
//! which calls `set_type_map(_TYPE_MAP)` before any leaf import — so the
//! per-leaf `_register_class(...)` trailers (still emitted in Phase 2)
//! mutate the same typemap the lazy entries pre-populated. The two
//! populations agree by construction.
//!
//! After Phase 4 drops the trailers and the eager leaf cascade, the
//! lazy entries become the sole source of truth and resolution happens
//! via `importlib.import_module + getattr` on first lookup.
//!
//! `module_path` is the dotted form of the leaf path, prefixed with
//! `baml_sdk`. Root-leaf symbols live at module path `baml_sdk`; a
//! symbol routed to `lorem/__init__.py` lives at `baml_sdk.lorem`.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{emit::EmittedSymbol, leaf::LeafBody, py_string, routing::LeafPath};

/// Render `_typemap.py` for the given grouped bodies. `sdk_root` is the
/// dotted prefix every emitted module path starts with — today
/// `"baml_sdk"`. Returns the file body (no banner; the caller prepends
/// the standard banner uniformly).
pub(crate) fn render_typemap_module(
    bodies: &BTreeMap<LeafPath, LeafBody>,
    sdk_root: &str,
) -> String {
    let mut classes: Vec<(String, String, String)> = Vec::new();
    let mut enums: Vec<(String, String, String)> = Vec::new();
    let mut aliases: Vec<(String, String, String)> = Vec::new();

    for (leaf, body) in bodies {
        let module_path = module_path_for_leaf(leaf, sdk_root);
        for (sym, _) in &body.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    classes.push((c.source.to_string(), module_path.clone(), c.py_name.clone()));
                }
                EmittedSymbol::Enum(e) => {
                    enums.push((e.source.to_string(), module_path.clone(), e.py_name.clone()));
                }
                EmittedSymbol::TypeAlias(a) => {
                    aliases.push((a.source.to_string(), module_path.clone(), a.py_name.clone()));
                }
                EmittedSymbol::Function(_) => {}
            }
        }
    }
    classes.sort();
    enums.sort();
    aliases.sort();

    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from baml_bridge import BamlTypeMap\n\n");

    write_entries(&mut out, "_CLASS_ENTRIES", &classes);
    out.push('\n');
    write_entries(&mut out, "_ENUM_ENTRIES", &enums);
    out.push('\n');
    write_entries(&mut out, "_ALIAS_ENTRIES", &aliases);
    out.push('\n');

    out.push_str(
        "_TYPE_MAP = BamlTypeMap.from_lazy_entries(\n    sdk_module=__package__,\n    \
         classes=_CLASS_ENTRIES,\n    \
         enums=_ENUM_ENTRIES,\n    \
         type_aliases=_ALIAS_ENTRIES,\n)\n",
    );

    out
}

fn module_path_for_leaf(leaf: &LeafPath, sdk_root: &str) -> String {
    if leaf.segments.is_empty() {
        sdk_root.to_string()
    } else {
        let mut s = String::with_capacity(
            sdk_root.len() + 1 + leaf.segments.iter().map(|s| s.len() + 1).sum::<usize>(),
        );
        s.push_str(sdk_root);
        for seg in &leaf.segments {
            s.push('.');
            s.push_str(seg);
        }
        s
    }
}

fn write_entries(out: &mut String, name: &str, entries: &[(String, String, String)]) {
    if entries.is_empty() {
        writeln!(out, "{name} = {{}}").unwrap();
        return;
    }
    writeln!(out, "{name} = {{").unwrap();
    for (fqn, module_path, py_name) in entries {
        writeln!(
            out,
            "    {}: ({}, {}),",
            py_string(fqn),
            py_string(module_path),
            py_string(py_name),
        )
        .unwrap();
    }
    out.push_str("}\n");
}
