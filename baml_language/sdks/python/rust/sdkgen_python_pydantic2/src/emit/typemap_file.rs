//! Codegen-emitted `_typemap.py` at the SDK root (25a2 §4.1, 25b2 Phase 2).
//!
//! Walks the grouped `LeafBody` map and renders three literal dicts of
//! `FQN → (module_path, attr_name)` lazy entries, the program descriptor,
//! and the `BamlTypeMap.from_lazy_entries(...)` call that installs the
//! populated map. The runtime `_typemap` module is imported by the root
//! init, which calls `set_type_map(_TYPE_MAP)` before any leaf import — so
//! the per-leaf `_register_class(...)` trailers (still emitted in Phase 2)
//! mutate the same typemap the lazy entries pre-populated. The two
//! populations agree by construction.
//!
//! After Phase 4 drops the trailers and the eager leaf cascade, the
//! lazy entries become the sole source of truth and resolution happens
//! via `importlib.import_module + getattr` on first lookup.
//!
//! `module_path` is the dotted form of the leaf path, appended to the SDK's
//! own package name, which the module derives from its own `__name__` at
//! import time. Root-leaf symbols live at the SDK package itself; a symbol
//! routed to `lorem/__init__.py` lives at `<sdk>.lorem`. Nothing names the
//! SDK, so it imports under whatever name and location it is installed at.
//! Each distinct module path is built once, as a constant the entries share.
//!
//! The descriptor is the program's declared surface (every callable's
//! signature and the declarations those reach), encoded by
//! `baml_sdkgen_types::ProgramDescriptor`. The bridge decodes it natively and
//! encodes arguments against the types it declares; Python never reads it.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{emit::EmittedSymbol, leaf::LeafBody, py_string, routing::LeafPath};

/// Render `_typemap.py` for the given grouped bodies. `descriptor` is the
/// embedded (single-line ASCII) program descriptor. Returns the file body (no
/// banner; the caller prepends the standard banner uniformly).
pub(crate) fn render_typemap_module(
    bodies: &BTreeMap<LeafPath, LeafBody>,
    descriptor: &str,
) -> String {
    let mut classes: Vec<(String, String, String)> = Vec::new();
    let mut enums: Vec<(String, String, String)> = Vec::new();
    let mut aliases: Vec<(String, String, String)> = Vec::new();
    // `(constant, module path expression)`, one per leaf with entries, so
    // each module path is built once at import time.
    let mut module_constants: Vec<(String, String)> = Vec::new();

    for (leaf, body) in bodies {
        let mut module_constant = None;
        for (sym, _) in &body.symbols {
            let (entries, source, py_name) = match sym {
                EmittedSymbol::Class(c) => (&mut classes, &c.source, &c.py_name),
                EmittedSymbol::Enum(e) => (&mut enums, &e.source, &e.py_name),
                EmittedSymbol::TypeAlias(a) => (&mut aliases, &a.source, &a.py_name),
                EmittedSymbol::Function(_) => continue,
            };
            let module_constant = module_constant.get_or_insert_with(|| {
                let constant = format!("_M{}", module_constants.len());
                module_constants.push((constant.clone(), module_path_expression(leaf)));
                constant
            });
            entries.push((source.to_string(), module_constant.clone(), py_name.clone()));
        }
    }
    classes.sort();
    enums.sort();
    aliases.sort();

    let mut out = String::new();
    out.push_str("from __future__ import annotations\n\n");
    out.push_str("from baml_bridge import BamlTypeMap\n\n");
    out.push_str(
        "# Module paths start at this SDK's own package, so the SDK imports under\n\
         # any name and location.\n",
    );
    writeln!(out, "{SDK_PACKAGE} = __name__.rpartition(\".\")[0]").unwrap();
    for (constant, module_path) in &module_constants {
        writeln!(out, "{constant} = {module_path}").unwrap();
    }
    out.push('\n');

    write_entries(&mut out, "_CLASS_ENTRIES", &classes);
    out.push('\n');
    write_entries(&mut out, "_ENUM_ENTRIES", &enums);
    out.push('\n');
    write_entries(&mut out, "_ALIAS_ENTRIES", &aliases);
    out.push('\n');

    debug_assert!(
        descriptor.is_ascii() && !descriptor.contains(['"', '\\', '\n']),
        "an embedded descriptor is single-line base64 and needs no escaping"
    );
    out.push_str(
        "# The program's declared surface: every callable's signature and the\n\
         # declarations those reach. The bridge decodes it natively and encodes\n\
         # arguments against the types it declares.\n",
    );
    writeln!(out, "_DESCRIPTOR: bytes = b\"{descriptor}\"").unwrap();
    out.push('\n');

    out.push_str(
        "_TYPE_MAP = BamlTypeMap.from_lazy_entries(\n    \
         classes=_CLASS_ENTRIES,\n    \
         enums=_ENUM_ENTRIES,\n    \
         type_aliases=_ALIAS_ENTRIES,\n    \
         descriptor=_DESCRIPTOR,\n)\n",
    );

    out
}

/// Python variable an SDK root-level data module (`_typemap.py`,
/// `_function_registry.py`) binds to its package's own name.
pub(crate) const SDK_PACKAGE: &str = "_SDK_PACKAGE";

/// Python expression for `leaf`'s module path, relative to [`SDK_PACKAGE`].
pub(crate) fn module_path_expression(leaf: &LeafPath) -> String {
    if leaf.segments.is_empty() {
        SDK_PACKAGE.to_string()
    } else {
        format!(
            "{SDK_PACKAGE} + {}",
            py_string(&format!(".{}", leaf.segments.join(".")))
        )
    }
}

/// `entries` are `(fqn, module path constant, attribute)`.
fn write_entries(out: &mut String, name: &str, entries: &[(String, String, String)]) {
    if entries.is_empty() {
        writeln!(out, "{name} = {{}}").unwrap();
        return;
    }
    writeln!(out, "{name} = {{").unwrap();
    for (fqn, module_constant, py_name) in entries {
        writeln!(
            out,
            "    {}: ({module_constant}, {}),",
            py_string(fqn),
            py_string(py_name),
        )
        .unwrap();
    }
    out.push_str("}\n");
}
