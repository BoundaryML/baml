//! Shared codegen-emitted `baml_sdk/_typemap.ts`.
//!
//! Walks the grouped `LeafBody` map and renders three literal records of
//! `FQN → () => <class/enum/alias>` **resolver thunks**, plus the
//! `BamlTypeMap.fromLazyEntries(...)` call that installs the populated map.
//! The root `index.ts` imports `_TYPE_MAP` and calls `setTypeMap(_TYPE_MAP)`.
//!
//! Each thunk closes over a statically imported namespace from the generated
//! SDK: `() => __leaf_0.Resume`. A thunk (rather than the Python-style
//! `[module_path, attr]` pair) is required because resolution must happen in the
//! generated SDK's module scope — the runtime `BamlTypeMap` lives in the
//! configured runtime package and cannot resolve a `baml_sdk/...` path.

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{emit::EmittedSymbol, leaf::LeafBody, routing::LeafPath, ts_string};

/// Render `_typemap.ts` for the given grouped bodies. `sdk_root` is unused
/// for the module path; it is kept for signature parity with the Python
/// emitter. Returns the file body (no banner; the caller prepends the standard
/// banner uniformly).
pub(crate) fn render_typemap_module(
    bodies: &BTreeMap<LeafPath, LeafBody>,
    _sdk_root: &str,
    runtime_package: &str,
) -> String {
    // (source FQN, module namespace alias, attr name)
    let mut classes: Vec<(String, String, String)> = Vec::new();
    let mut enums: Vec<(String, String, String)> = Vec::new();
    let mut aliases: Vec<(String, String, String)> = Vec::new();
    let mut module_aliases: BTreeMap<LeafPath, String> = BTreeMap::new();

    for (leaf, body) in bodies {
        for (sym, _) in &body.symbols {
            match sym {
                EmittedSymbol::Class(c) => {
                    let module_alias = alias_for_leaf(&mut module_aliases, leaf);
                    classes.push((c.source.to_string(), module_alias, c.name.clone()));
                }
                EmittedSymbol::Enum(e) => {
                    let module_alias = alias_for_leaf(&mut module_aliases, leaf);
                    enums.push((e.source.to_string(), module_alias, e.name.clone()));
                }
                EmittedSymbol::TypeAlias(a) => {
                    let module_alias = alias_for_leaf(&mut module_aliases, leaf);
                    aliases.push((a.source.to_string(), module_alias, a.name.clone()));
                }
                EmittedSymbol::Function(_) => {}
            }
        }
    }
    classes.sort();
    enums.sort();
    aliases.sort();

    let mut out = String::new();
    writeln!(out, "import {{ BamlTypeMap }} from \"{runtime_package}\";").unwrap();
    for (leaf, module_alias) in &module_aliases {
        writeln!(
            out,
            "import * as {module_alias} from {};",
            ts_string(&module_path_for_leaf(leaf)),
        )
        .unwrap();
    }
    out.push('\n');

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

fn alias_for_leaf(module_aliases: &mut BTreeMap<LeafPath, String>, leaf: &LeafPath) -> String {
    if let Some(alias) = module_aliases.get(leaf) {
        return alias.clone();
    }
    let alias = format!("__leaf_{}", module_aliases.len());
    module_aliases.insert(leaf.clone(), alias.clone());
    alias
}

/// Import path (relative to `baml_sdk/_typemap.ts`) for a leaf:
/// `./index.js` for the root, `./lorem/index.js` / `./vendor/aws/s3/index.js`
/// for nested leaves.
fn module_path_for_leaf(leaf: &LeafPath) -> String {
    if leaf.segments.is_empty() {
        "./index.js".to_string()
    } else {
        format!("./{}/index.js", leaf.segments.join("/"))
    }
}

fn write_entries(out: &mut String, name: &str, entries: &[(String, String, String)]) {
    if entries.is_empty() {
        writeln!(out, "const {name}: Record<string, () => unknown> = {{}};").unwrap();
        return;
    }
    writeln!(out, "const {name}: Record<string, () => unknown> = {{").unwrap();
    for (fqn, module_alias, ts_name) in entries {
        // A deferred resolver: `() => (__leaf_0 as Record<string, unknown>)["Resume"]`.
        writeln!(
            out,
            "  {}: () => ({} as Record<string, unknown>)[{}],",
            ts_string(fqn),
            module_alias,
            ts_string(ts_name),
        )
        .unwrap();
    }
    out.push_str("};\n");
}

#[cfg(test)]
mod tests {
    use baml_base::{Name as BaseName, qualified_name::AI_STREAM_STREAM};
    use baml_codegen_types::Name;

    use super::*;
    use crate::{
        emit::{EmittedSymbol, class::TypeScriptClass, enum_::TypeScriptEnum},
        leaf::LeafBody,
    };

    const TEST_RUNTIME_PACKAGE: &str = "@boundaryml/baml-bridge";

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn class_sym(source: Name, n: &str) -> EmittedSymbol {
        EmittedSymbol::Class(TypeScriptClass {
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
        EmittedSymbol::Enum(TypeScriptEnum {
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
        let out = render_typemap_module(&bodies, "baml_sdk", TEST_RUNTIME_PACKAGE);
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
        let out = render_typemap_module(&bodies, "baml_sdk", TEST_RUNTIME_PACKAGE);
        assert!(out.contains("import * as __leaf_0 from \"./vendor/aws/s3/index.js\";"));
        assert!(out.contains(
            "\"aws.s3.Bucket\": () => (__leaf_0 as Record<string, unknown>)[\"Bucket\"],"
        ));
    }

    #[test]
    fn stream_class_resolves_in_base_leaf_with_suffix() {
        // spec2: the `$stream` companion lives in its base type's leaf
        // (`lorem`), and its emitted identifier keeps the `$stream` suffix.
        let mut bodies = BTreeMap::new();
        let leaf = LeafPath {
            segments: vec!["lorem".into()],
        };
        bodies.insert(
            leaf.clone(),
            body(
                leaf,
                vec![class_sym(
                    name("user", &["lorem"], "Resume$stream"),
                    "Resume$stream",
                )],
            ),
        );
        let out = render_typemap_module(&bodies, "baml_sdk", TEST_RUNTIME_PACKAGE);
        assert!(out.contains("import * as __leaf_0 from \"./lorem/index.js\";"));
        assert!(out.contains(
            "\"user.lorem.Resume$stream\": () => (__leaf_0 as Record<string, unknown>)[\"Resume$stream\"],"
        ));
    }

    #[test]
    fn runtime_owned_fqns_resolve_through_generated_public_leaves() {
        let media_leaf = LeafPath {
            segments: vec!["baml".into(), "media".into()],
        };
        let stream_leaf = LeafPath {
            segments: vec!["ai".into(), "stream".into()],
        };
        let mut bodies = BTreeMap::new();
        bodies.insert(
            media_leaf.clone(),
            body(
                media_leaf,
                vec![class_sym(name("baml", &["media"], "Image"), "Image")],
            ),
        );
        bodies.insert(
            stream_leaf.clone(),
            body(
                stream_leaf,
                vec![class_sym(name("ai", &["stream"], "Stream"), "Stream")],
            ),
        );
        let out = render_typemap_module(&bodies, "baml_sdk", "@boundaryml/baml-bridge-web");
        assert!(out.contains("import * as __leaf_0 from \"./ai/stream/index.js\";"));
        assert!(out.contains("import * as __leaf_1 from \"./baml/media/index.js\";"));
        assert!(out.contains(&format!(
            "\"{AI_STREAM_STREAM}\": () => (__leaf_0 as Record<string, unknown>)[\"Stream\"],"
        )));
        assert!(out.contains(
            "\"baml.media.Image\": () => (__leaf_1 as Record<string, unknown>)[\"Image\"],"
        ));
        assert!(!out.contains("baml_sdk.baml.media.Image"));
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
        let out = render_typemap_module(&bodies, "baml_sdk", TEST_RUNTIME_PACKAGE);
        assert!(out.contains("import * as __leaf_0 from \"./index.js\";"));
        assert!(out.contains(
            "\"user.Sentiment\": () => (__leaf_0 as Record<string, unknown>)[\"Sentiment\"],"
        ));
    }
}
