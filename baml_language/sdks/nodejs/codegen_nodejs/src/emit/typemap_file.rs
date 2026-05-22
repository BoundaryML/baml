//! Codegen-emitted `baml_sdk/_typemap.ts`. Walks the grouped `LeafBody`
//! map and renders three literal records of
//! `FQN → [modulePath, attrName]` lazy entries, plus the
//! `BamlTypeMap.fromLazyEntries(...)` call that installs the populated
//! map.
//!
//! `modulePath` is the **filesystem-relative** path under the SDK root
//! (`"baml_sdk/lorem"`, etc.), not a dotted form — JS imports are
//! path-based. The runtime `BamlTypeMap.getClass(fqn)` uses
//! `require(modulePath)[attr]` semantics.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{emit::EmittedSymbol, leaf::LeafBody, routing::LeafPath, ts_string};

/// Render `_typemap.ts` for the given grouped bodies. `sdk_root` is the
/// prefix every emitted module path starts with — today `"baml_sdk"`.
/// Returns the file body without the leading banner; the caller prepends
/// it uniformly.
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
                    classes.push((c.source.to_string(), module_path.clone(), c.name.clone()));
                }
                EmittedSymbol::Enum(e) => {
                    enums.push((e.source.to_string(), module_path.clone(), e.name.clone()));
                }
                EmittedSymbol::TypeAlias(a) => {
                    aliases.push((a.source.to_string(), module_path.clone(), a.name.clone()));
                }
                EmittedSymbol::Function(_) => {}
            }
        }
    }
    classes.sort();
    enums.sort();
    aliases.sort();

    let mut out = String::new();
    out.push_str("import { BamlTypeMap } from \"@boundaryml/baml-node\";\n\n");

    write_entries(&mut out, "_CLASS_ENTRIES", &classes);
    out.push('\n');
    write_entries(&mut out, "_ENUM_ENTRIES", &enums);
    out.push('\n');
    write_entries(&mut out, "_ALIAS_ENTRIES", &aliases);
    out.push('\n');

    out.push_str(
        "export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({\n    \
         classes: _CLASS_ENTRIES,\n    \
         enums: _ENUM_ENTRIES,\n    \
         typeAliases: _ALIAS_ENTRIES,\n});\n",
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
            s.push('/');
            s.push_str(seg);
        }
        s
    }
}

fn write_entries(out: &mut String, name: &str, entries: &[(String, String, String)]) {
    if entries.is_empty() {
        writeln!(
            out,
            "const {name}: Record<string, [string, string]> = {{}};"
        )
        .unwrap();
        return;
    }
    writeln!(out, "const {name}: Record<string, [string, string]> = {{").unwrap();
    for (fqn, module_path, name) in entries {
        writeln!(
            out,
            "    {}: [{}, {}],",
            ts_string(fqn),
            ts_string(module_path),
            ts_string(name),
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
    use crate::emit::{SortKey, class::NodeClass, enum_::NodeEnum, type_alias::NodeTypeAlias};

    fn cg_name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn leaf(segs: &[&str]) -> LeafPath {
        LeafPath {
            segments: segs.iter().map(|&s| s.to_string()).collect(),
        }
    }

    fn class_sym(name: &str, n: Name) -> EmittedSymbol {
        EmittedSymbol::Class(NodeClass {
            name: name.to_string(),
            source: n,
            generic_params: vec![],
            docstring: None,
            properties: vec![],
            static_methods: vec![],
            instance_methods: vec![],
        })
    }

    fn enum_sym(name: &str, n: Name) -> EmittedSymbol {
        EmittedSymbol::Enum(NodeEnum {
            name: name.to_string(),
            source: n,
            variants: vec![],
            docstring: None,
        })
    }

    fn alias_sym(name: &str, n: Name) -> EmittedSymbol {
        EmittedSymbol::TypeAlias(NodeTypeAlias {
            name: name.to_string(),
            source: n,
            resolves_to: baml_codegen_types::Ty::Int,
            recursive: false,
        })
    }

    fn sort_key() -> SortKey {
        ("x.baml".to_string(), 0)
    }

    #[test]
    fn empty_pool_renders_three_empty_records() {
        let bodies: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains("const _CLASS_ENTRIES: Record<string, [string, string]> = {};"));
        assert!(out.contains("const _ENUM_ENTRIES: Record<string, [string, string]> = {};"));
        assert!(out.contains("const _ALIAS_ENTRIES: Record<string, [string, string]> = {};"));
        assert!(out.contains("export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({"));
    }

    #[test]
    fn nested_class_module_path() {
        let mut bodies: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
        let n = cg_name("aws", &["s3"], "Bucket");
        let lp = leaf(&["vendor", "aws", "s3"]);
        bodies.insert(
            lp.clone(),
            LeafBody {
                leaf: lp,
                symbols: vec![(class_sym("Bucket", n), sort_key())],
            },
        );
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains("\"aws.s3.Bucket\": [\"baml_sdk/vendor/aws/s3\", \"Bucket\"]"));
    }

    #[test]
    fn stream_class_module_path() {
        let mut bodies: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
        let n = cg_name("user", &["lorem"], "Resume$stream");
        let lp = leaf(&["stream_types", "lorem"]);
        bodies.insert(
            lp.clone(),
            LeafBody {
                leaf: lp,
                symbols: vec![(class_sym("Resume", n), sort_key())],
            },
        );
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains(
            "\"user.lorem.Resume$stream\": [\"baml_sdk/stream_types/lorem\", \"Resume\"]"
        ));
    }

    #[test]
    fn enums_and_aliases_routed_to_separate_records() {
        let mut bodies: BTreeMap<LeafPath, LeafBody> = BTreeMap::new();
        let lorem = leaf(&["lorem"]);
        bodies.insert(
            lorem.clone(),
            LeafBody {
                leaf: lorem,
                symbols: vec![
                    (
                        enum_sym("Sentiment", cg_name("user", &["lorem"], "Sentiment")),
                        sort_key(),
                    ),
                    (
                        alias_sym("Foo", cg_name("user", &["lorem"], "Foo")),
                        sort_key(),
                    ),
                ],
            },
        );
        let out = render_typemap_module(&bodies, "baml_sdk");
        assert!(out.contains("\"user.lorem.Sentiment\": [\"baml_sdk/lorem\", \"Sentiment\"]"));
        assert!(out.contains("\"user.lorem.Foo\": [\"baml_sdk/lorem\", \"Foo\"]"));
    }
}
