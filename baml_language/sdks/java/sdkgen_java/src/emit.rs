//! Phase-2 symbol bodies: structurally correct, member-free Java
//! declarations. Every top renders to a placeholder that compiles under
//! `javac --release 17` and reserves the right name, so generated-code
//! imports in the parity tests resolve; constructors, fields, methods,
//! and `Fns` bindings arrive with `translate_ty` in later phases
//! (mirroring the TS emitter's phase-2 → phase-4 progression).

use baml_codegen_types::{Class, Enum};

use crate::routing::java_identifier;

/// Render a `///` docstring as a Javadoc block. Returns an empty
/// string when there is no docstring.
pub(crate) fn render_javadoc(docstring: Option<&str>, indent: &str) -> String {
    let Some(doc) = docstring else {
        return String::new();
    };
    let mut out = String::new();
    out.push_str(indent);
    out.push_str("/**\n");
    for line in doc.lines() {
        out.push_str(indent);
        out.push_str(" * ");
        // A literal `*/` inside the docstring would terminate the
        // Javadoc block early.
        out.push_str(&line.replace("*/", "* /"));
        out.push('\n');
    }
    out.push_str(indent);
    out.push_str(" */\n");
    out
}

/// `public class Resume {}` / `public class Wrapper<T> {}` — name and
/// type parameters only; fields, constructor, and methods land in
/// phase 4.
pub(crate) fn render_class(class: &Class) -> String {
    let mut out = render_javadoc(class.docstring.as_deref(), "");
    let ident = java_identifier(class.name.name.as_str());
    out.push_str("public class ");
    out.push_str(&ident);
    if !class.generic_params.is_empty() {
        out.push('<');
        let params: Vec<String> = class
            .generic_params
            .iter()
            .map(|p| java_identifier(p.as_str()))
            .collect();
        out.push_str(&params.join(", "));
        out.push('>');
    }
    out.push_str(" {\n}\n");
    out
}

/// `public enum Sentiment { Positive, Negative }`. Constants keep the
/// BAML variant spelling (`PreserveCase`) modulo the keyword/identifier
/// escape; the wire-name serializer map that reconciles escaped
/// constants with variant spellings lands with the enum capability.
pub(crate) fn render_enum(enum_: &Enum) -> String {
    let mut out = render_javadoc(enum_.docstring.as_deref(), "");
    let ident = java_identifier(enum_.name.name.as_str());
    out.push_str("public enum ");
    out.push_str(&ident);
    out.push_str(" {\n");
    for variant in &enum_.variants {
        out.push_str(&render_javadoc(variant.docstring.as_deref(), "    "));
        out.push_str("    ");
        out.push_str(&java_identifier(variant.name.as_str()));
        out.push_str(",\n");
    }
    out.push_str("}\n");
    out
}

/// The per-package free-function holder, phase-2 shape: a final,
/// uninstantiable class with no bindings yet. `holder_ident` is `Fns`,
/// or `Fns$` when the package defines a symbol named `Fns` (the
/// conventions doc's collision escape).
pub(crate) fn render_fns_holder(holder_ident: &str, java_package: &str) -> String {
    format!(
        "/**\n * Free functions declared in the BAML namespace backing\n * `{java_package}`. Function bindings are emitted in a later phase.\n */\npublic final class {holder_ident} {{\n    private {holder_ident}() {{}}\n}}\n"
    )
}
