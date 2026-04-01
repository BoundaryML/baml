//! Code generation for the `PanicClass` enum.
//!
//! Reads `NativeClassDef` entries whose `namespace_prefix` is `"baml.panics"`
//! and generates a `PanicClass` fieldless enum with `fqn()`, `name()`, `ALL`,
//! `ALL_NAMES`, and `from_name()`.
//!
//! Generated from `panics.baml` class definitions so that adding a new panic
//! type only requires editing the `.baml` file.

use std::fmt::Write;

use crate::types::NativeClassDef;

const PANICS_NAMESPACE: &str = "baml.panics";

/// Generate the `PanicClass` enum and associated methods.
pub fn generate_panic_enums(class_defs: &[NativeClassDef]) -> String {
    let panics: Vec<&NativeClassDef> = class_defs
        .iter()
        .filter(|c| c.namespace_prefix == PANICS_NAMESPACE)
        .collect();

    assert!(
        !panics.is_empty(),
        "no panic classes found in namespace {PANICS_NAMESPACE} — is panics.baml registered?"
    );

    let mut out = String::new();
    generate_panic_class_enum(&mut out, &panics);
    out
}

// ── PanicClass ───────────────────────────────────────────────────────────────

fn generate_panic_class_enum(out: &mut String, panics: &[&NativeClassDef]) {
    // Enum definition
    out.push_str("/// Panic class tag — one variant per `baml.panics.*` class.\n");
    out.push_str("///\n");
    out.push_str("/// Auto-generated from `panics.baml`.\n");
    out.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\n");
    out.push_str("pub enum PanicClass {\n");
    for p in panics {
        writeln!(out, "    {name},", name = p.name).unwrap();
    }
    out.push_str("}\n\n");

    out.push_str("impl PanicClass {\n");

    // fqn()
    out.push_str("    /// Fully-qualified class name (e.g. `\"baml.panics.DivisionByZero\"`).\n");
    out.push_str("    pub const fn fqn(&self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for p in panics {
        writeln!(
            out,
            "            PanicClass::{name} => \"{ns}.{name}\",",
            name = p.name,
            ns = PANICS_NAMESPACE,
        )
        .unwrap();
    }
    out.push_str("        }\n    }\n\n");

    // name()
    out.push_str("    /// Short class name (e.g. `\"DivisionByZero\"`).\n");
    out.push_str("    pub const fn name(&self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for p in panics {
        writeln!(
            out,
            "            PanicClass::{name} => \"{name}\",",
            name = p.name,
        )
        .unwrap();
    }
    out.push_str("        }\n    }\n\n");

    // ALL
    out.push_str("    /// All panic class variants.\n");
    out.push_str("    pub const ALL: &[PanicClass] = &[\n");
    for p in panics {
        writeln!(out, "        PanicClass::{name},", name = p.name).unwrap();
    }
    out.push_str("    ];\n\n");

    // ALL_NAMES
    out.push_str("    /// All panic class short names.\n");
    out.push_str("    pub const ALL_NAMES: &[&str] = &[\n");
    for p in panics {
        writeln!(out, "        \"{name}\",", name = p.name).unwrap();
    }
    out.push_str("    ];\n\n");

    // from_name()
    out.push_str(
        "    /// Look up a `PanicClass` by its short name (e.g. `\"DivisionByZero\"`).\n",
    );
    out.push_str("    pub fn from_name(name: &str) -> Option<PanicClass> {\n");
    out.push_str("        match name {\n");
    for p in panics {
        writeln!(
            out,
            "            \"{name}\" => Some(PanicClass::{name}),",
            name = p.name,
        )
        .unwrap();
    }
    out.push_str("            _ => None,\n");
    out.push_str("        }\n    }\n");

    out.push_str("}\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{NativeClassDef, NativeClassField};
    use crate::BamlType;

    fn make_panic_class(name: &str, fields: Vec<(&str, BamlType)>) -> NativeClassDef {
        NativeClassDef {
            name: name.to_string(),
            namespace_prefix: "baml.panics".to_string(),
            generic_params: vec![],
            fields: fields
                .into_iter()
                .enumerate()
                .map(|(i, (n, ty))| NativeClassField {
                    name: n.to_string(),
                    field_type: ty,
                    index: i,
                })
                .collect(),
            source_file: "<test>".to_string(),
        }
    }

    #[test]
    fn test_generate_panic_enums() {
        let class_defs = vec![
            make_panic_class("DivisionByZero", vec![("dividend", BamlType::Int)]),
            make_panic_class(
                "IndexOutOfBounds",
                vec![("index", BamlType::Int), ("length", BamlType::Int)],
            ),
            make_panic_class("StackOverflow", vec![("message", BamlType::String)]),
        ];
        let code = generate_panic_enums(&class_defs);

        // PanicClass enum
        assert!(code.contains("pub enum PanicClass {"));
        assert!(code.contains("DivisionByZero,"));
        assert!(code.contains("IndexOutOfBounds,"));
        assert!(code.contains("StackOverflow,"));

        // fqn
        assert!(code.contains("\"baml.panics.DivisionByZero\""));

        // ALL_NAMES
        assert!(code.contains("\"DivisionByZero\","));

        // No PanicInstance
        assert!(!code.contains("PanicInstance"));
    }
}
