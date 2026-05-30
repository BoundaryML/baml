//! Codegen-emitted `baml_sdk/_typemap.ts`.
//!
//! Walks the grouped `LeafBody` map and renders three literal records of
//! `FQN → () => <class/enum/alias>` **resolver thunks**, plus the
//! `BamlTypeMap.fromLazyEntries(...)` call that installs the populated map.
//! The root `index.ts` imports `_TYPE_MAP` and calls `setTypeMap(_TYPE_MAP)`.
//!
//! Each thunk closes over a `require` relative to *this* file
//! (`baml_sdk/_typemap.ts`): `() => require("./lorem").Resume`. A thunk
//! (rather than the Python-style `[module_path, attr]` pair) is required
//! because resolution must happen in the generated SDK's module scope — the
//! runtime `BamlTypeMap` lives in `@boundaryml/baml-core` and cannot resolve
//! a `baml_sdk/...` path. The require is deferred (lazy) so importing
//! `_typemap.ts` does not eagerly load every leaf, and so the root's own
//! `require(".")` does not deadlock the circular `index ↔ _typemap` import.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{emit::EmittedSymbol, leaf::LeafBody, routing::LeafPath, ts_string};

/// Render `_typemap.ts` for the given grouped bodies. `sdk_root` is unused
/// for the module path (thunks use a require relative to `_typemap.ts`); it
/// is kept for signature parity with the Python emitter. Returns the file
/// body (no banner; the caller prepends the standard banner uniformly).
pub(crate) fn render_typemap_module(
    bodies: &BTreeMap<LeafPath, LeafBody>,
    _sdk_root: &str,
) -> String {
    // (source FQN, require path relative to _typemap.ts, attr name)
    let mut classes: Vec<(String, String, String)> = Vec::new();
    let mut enums: Vec<(String, String, String)> = Vec::new();
    let mut aliases: Vec<(String, String, String)> = Vec::new();

    for (leaf, body) in bodies {
        let require_path = require_path_for_leaf(leaf);
        for (sym, _) in &body.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    classes.push((c.source.to_string(), require_path.clone(), c.name.clone()));
                }
                EmittedSymbol::Enum(e) => {
                    enums.push((e.source.to_string(), require_path.clone(), e.name.clone()));
                }
                EmittedSymbol::TypeAlias(a) => {
                    aliases.push((a.source.to_string(), require_path.clone(), a.name.clone()));
                }
                EmittedSymbol::Function(_) => {}
            }
        }
    }
    classes.sort();
    enums.sort();
    aliases.sort();

    let mut out = String::new();
    out.push_str("import { BamlTypeMap } from \"@boundaryml/baml-core\";\n\n");

    write_entries(&mut out, "_CLASS_ENTRIES", &classes);
    out.push('\n');
    write_entries(&mut out, "_ENUM_ENTRIES", &enums);
    out.push('\n');
    write_entries(&mut out, "_ALIAS_ENTRIES", &aliases);
    out.push('\n');

    out.push_str(
        "export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({\n  \
         classes: _CLASS_ENTRIES,\n  \
         enums: _ENUM_ENTRIES,\n  \
         typeAliases: _ALIAS_ENTRIES,\n});\n",
    );

    out
}

/// Require path (relative to `baml_sdk/_typemap.ts`) for a leaf: `.` for the
/// root, `./lorem` / `./vendor/aws/s3` for nested leaves.
fn require_path_for_leaf(leaf: &LeafPath) -> String {
    if leaf.segments.is_empty() {
        ".".to_string()
    } else {
        format!("./{}", leaf.segments.join("/"))
    }
}

fn write_entries(out: &mut String, name: &str, entries: &[(String, String, String)]) {
    if entries.is_empty() {
        writeln!(out, "const {name}: Record<string, () => unknown> = {{}};").unwrap();
        return;
    }
    writeln!(out, "const {name}: Record<string, () => unknown> = {{").unwrap();
    for (fqn, require_path, ts_name) in entries {
        // A deferred resolver: `() => require("./lorem").Resume`.
        writeln!(
            out,
            "  {}: () => require({})[{}],",
            ts_string(fqn),
            ts_string(require_path),
            ts_string(ts_name),
        )
        .unwrap();
    }
    out.push_str("};\n");
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::Name;

    use super::*;
    use crate::{
        emit::{EmittedSymbol, class::NodeClass, enum_::NodeEnum},
        leaf::LeafBody,
    };

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn class_sym(source: Name, n: &str) -> EmittedSymbol {
        EmittedSymbol::Class(NodeClass {
            name: n.to_string(),
            source,
            generic_params: Vec::new(),
            docstring: None,
            properties: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
    }

    fn enum_sym(source: Name, n: &str) -> EmittedSymbol {
        EmittedSymbol::Enum(NodeEnum {
            name: n.to_string(),
            source,
            variants: Vec::new(),
            docstring: None,
        })
    }

    fn body(leaf: LeafPath, syms: Vec<EmittedSymbol>) -> LeafBody {
        LeafBody {
            leaf,
            symbols: syms.into_iter().map(|s| (s, (String::new(), 0))).collect(),
        }
    }

    #[test]
    fn empty_pool_yields_empty_records() {
        let bodies = BTreeMap::new();
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains("const _CLASS_ENTRIES: Record<string, () => unknown> = {};"));
        assert!(out.contains("const _ENUM_ENTRIES: Record<string, () => unknown> = {};"));
        assert!(out.contains("const _ALIAS_ENTRIES: Record<string, () => unknown> = {};"));
        assert!(out.contains("BamlTypeMap.fromLazyEntries({"));
    }

    #[test]
    fn nested_vendor_leaf_module_path_is_slash_joined() {
        let mut bodies = BTreeMap::new();
        let leaf = LeafPath {
            segments: vec!["vendor".into(), "aws".into(), "s3".into()],
        };
        bodies.insert(
            leaf.clone(),
            body(
                leaf,
                vec![class_sym(name("aws", &["s3"], "Bucket"), "Bucket")],
            ),
        );
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains("\"aws.s3.Bucket\": () => require(\"./vendor/aws/s3\")[\"Bucket\"],"));
    }

    #[test]
    fn stream_class_routes_to_stream_types_module_path() {
        let mut bodies = BTreeMap::new();
        let leaf = LeafPath {
            segments: vec!["stream_types".into(), "lorem".into()],
        };
        bodies.insert(
            leaf.clone(),
            body(
                leaf,
                vec![class_sym(
                    name("user", &["lorem"], "Resume$stream"),
                    "Resume",
                )],
            ),
        );
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains(
            "\"user.lorem.Resume$stream\": () => require(\"./stream_types/lorem\")[\"Resume\"],"
        ));
    }

    #[test]
    fn root_leaf_uses_baml_sdk_module_path() {
        let mut bodies = BTreeMap::new();
        let leaf = LeafPath { segments: vec![] };
        bodies.insert(
            leaf.clone(),
            body(
                leaf,
                vec![enum_sym(name("user", &[], "Sentiment"), "Sentiment")],
            ),
        );
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains("\"user.Sentiment\": () => require(\".\")[\"Sentiment\"],"));
    }
}
