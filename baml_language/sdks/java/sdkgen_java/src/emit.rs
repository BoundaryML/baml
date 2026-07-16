// String-builder style: the emitter appends rendered fragments; the
// `write!`-into-String alternative buys nothing here but noise.
#![allow(clippy::format_push_string)]

//! Phase-4 symbol bodies: real Java declarations.
//!
//! Classes render with `private final` fields, a canonical all-args
//! constructor (field declaration order), `PreserveCase` accessor
//! methods, and deep value equality (`Arrays.equals` for `byte[]`
//! fields — the parity tests assert whole-object equality on round
//! trips). Free functions render as static bindings on the
//! per-package `Fns` holder, calling the `baml_bridge.BamlFfi`
//! runtime entry points:
//!
//! ```java
//! public static long return_int() {
//!     return (java.lang.Long) baml_bridge.BamlFfi.callSync(
//!         "user.primitives.return_int", NAMES, new Object[] {});
//! }
//! ```
//!
//! Decode is wire-driven (FQN + typemap), so generated bodies only
//! cast the decoded `Object` to the declared type; primitives unbox
//! implicitly on return.
//!
//! Not yet emitted (later capabilities): class static/instance
//! methods, optional-arg configurator overloads, explicit generic
//! type-args overloads. Functions with optional arguments emit their
//! required-args form only, so the engine evaluates BAML defaults.

use baml_codegen_types::{Class, Enum, Function, Ty};

use crate::{
    routing::java_identifier,
    translate_ty::{TranslateCtx, TyPosition, UnionSink, translate_ty},
};

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

/// How a field participates in `equals` / `hashCode`.
enum FieldEq {
    /// `==` comparison, `Objects.hash` member.
    Primitive,
    /// `Arrays.equals` / `Arrays.hashCode`.
    ByteArray,
    /// `Objects.equals` / `Objects.hash`.
    Reference,
}

fn field_eq(java_ty: &str) -> FieldEq {
    match java_ty {
        "long" | "boolean" | "double" => FieldEq::Primitive,
        "byte[]" => FieldEq::ByteArray,
        _ => FieldEq::Reference,
    }
}

/// Full value-class body: fields, canonical constructor, accessors,
/// deep `equals`/`hashCode`. Static/instance methods land with the
/// methods capability.
pub(crate) fn render_class(class: &Class, ctx: &TranslateCtx<'_>, sink: &mut UnionSink) -> String {
    let mut out = render_javadoc(class.docstring.as_deref(), "");
    let ident = java_identifier(class.name.name.as_str());
    let generics = if class.generic_params.is_empty() {
        String::new()
    } else {
        let params: Vec<String> = class
            .generic_params
            .iter()
            .map(|p| java_identifier(p.as_str()))
            .collect();
        format!("<{}>", params.join(", "))
    };

    // (ident, java type) per property, in declaration order.
    let fields: Vec<(String, String)> = class
        .properties
        .iter()
        .map(|p| {
            (
                java_identifier(p.name.as_str()),
                translate_ty(&p.ty, TyPosition::TopLevel, ctx, sink),
            )
        })
        .collect();

    out.push_str(&format!("public class {ident}{generics} {{\n"));

    for ((f_ident, f_ty), prop) in fields.iter().zip(&class.properties) {
        out.push_str(&render_javadoc(prop.docstring.as_deref(), "    "));
        out.push_str(&format!("    private final {f_ty} {f_ident};\n"));
    }

    // Canonical all-args constructor, field declaration order.
    let params: Vec<String> = fields
        .iter()
        .map(|(f_ident, f_ty)| format!("{f_ty} {f_ident}"))
        .collect();
    out.push_str(&format!("\n    public {ident}({}) {{\n", params.join(", ")));
    for (f_ident, _) in &fields {
        out.push_str(&format!("        this.{f_ident} = {f_ident};\n"));
    }
    out.push_str("    }\n");

    // PreserveCase accessors.
    for (f_ident, f_ty) in &fields {
        out.push_str(&format!(
            "\n    public {f_ty} {f_ident}() {{\n        return this.{f_ident};\n    }}\n"
        ));
    }

    // Deep value equality. `instanceof` narrowing keeps this correct
    // for generic classes (erasure makes a parameterized check
    // impossible anyway).
    out.push_str(&format!(
        "\n    @Override\n    public boolean equals(java.lang.Object o) {{\n        if (this == o) {{\n            return true;\n        }}\n        if (!(o instanceof {ident})) {{\n            return false;\n        }}\n        {ident}{wild} other = ({ident}{wild}) o;\n",
        ident = ident,
        wild = if class.generic_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", vec!["?"; class.generic_params.len()].join(", "))
        },
    ));
    if fields.is_empty() {
        out.push_str("        return true;\n");
    } else {
        let clauses: Vec<String> = fields
            .iter()
            .map(|(f_ident, f_ty)| match field_eq(f_ty) {
                FieldEq::Primitive => format!("this.{f_ident} == other.{f_ident}"),
                FieldEq::ByteArray => {
                    format!("java.util.Arrays.equals(this.{f_ident}, other.{f_ident})")
                }
                FieldEq::Reference => {
                    format!("java.util.Objects.equals(this.{f_ident}, other.{f_ident})")
                }
            })
            .collect();
        out.push_str(&format!(
            "        return {};\n",
            clauses.join("\n            && ")
        ));
    }
    out.push_str("    }\n");

    // hashCode consistent with equals.
    let hash_members: Vec<String> = fields
        .iter()
        .map(|(f_ident, f_ty)| match field_eq(f_ty) {
            FieldEq::ByteArray => format!("java.util.Arrays.hashCode(this.{f_ident})"),
            _ => format!("this.{f_ident}"),
        })
        .collect();
    out.push_str(&format!(
        "\n    @Override\n    public int hashCode() {{\n        return java.util.Objects.hash({});\n    }}\n",
        hash_members.join(", ")
    ));

    out.push_str("}\n");
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

/// The per-package free-function holder with its static bindings.
/// `holder_ident` is `Fns`, or `Fns$` when the package defines a
/// symbol named `Fns` (the conventions doc's collision escape).
/// `functions` is `(fqn, function)` in deterministic order.
pub(crate) fn render_fns_holder(
    holder_ident: &str,
    java_package: &str,
    functions: &[(String, &Function)],
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let mut out = format!(
        "/**\n * Free functions declared in the BAML namespace backing\n * `{java_package}`.\n */\npublic final class {holder_ident} {{\n    private {holder_ident}() {{}}\n"
    );
    for (fqn, function) in functions {
        out.push_str(&render_function_pair(fqn, function, ctx, sink));
    }
    out.push_str("}\n");
    out
}

/// One sync + one `_async` static binding for a free function.
/// Required (defaultless) arguments only — optional-arg configurator
/// overloads land with the optional-args capability, so omitted
/// optionals hit the engine's BAML defaults.
fn render_function_pair(
    fqn: &str,
    function: &Function,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let ident = java_identifier(function.name.as_str());
    let required: Vec<_> = function
        .arguments
        .iter()
        .filter(|a| a.default.is_none())
        .collect();

    let param_decls: Vec<String> = required
        .iter()
        .map(|a| {
            format!(
                "{} {}",
                translate_ty(&a.ty, TyPosition::TopLevel, ctx, sink),
                java_identifier(a.name.as_str())
            )
        })
        .collect();
    let param_names_java: Vec<String> = required
        .iter()
        .map(|a| format!("{:?}", a.name.as_str()))
        .collect();
    let arg_exprs: Vec<String> = required
        .iter()
        .map(|a| java_identifier(a.name.as_str()))
        .collect();

    let ret_top = translate_ty(&function.return_type, TyPosition::TopLevel, ctx, sink);
    let ret_boxed = translate_ty(&function.return_type, TyPosition::Boxed, ctx, sink);

    let names_literal = format!("new java.lang.String[] {{{}}}", param_names_java.join(", "));
    let args_literal = format!("new java.lang.Object[] {{{}}}", arg_exprs.join(", "));

    let doc = render_javadoc(function.docstring.as_deref(), "    ");

    let sync_body = if ret_top == "void" {
        format!("        baml_bridge.BamlFfi.callSync({fqn:?}, {names_literal}, {args_literal});")
    } else {
        format!(
            "        return ({ret_boxed}) baml_bridge.BamlFfi.callSync({fqn:?}, {names_literal}, {args_literal});"
        )
    };

    // Async siblings return the boxed shape; the cast happens in a
    // thenApply so the future's element type is precise.
    let async_ret = format!("java.util.concurrent.CompletableFuture<{ret_boxed}>");
    let async_body = format!(
        "        return baml_bridge.BamlFfi.callAsync({fqn:?}, {names_literal}, {args_literal}).thenApply(v -> ({ret_boxed}) v);"
    );

    format!(
        "\n{doc}    public static {ret_top} {ident}({params}) {{\n{sync_body}\n    }}\n\n{doc}    @SuppressWarnings(\"unchecked\")\n    public static {async_ret} {ident}_async({params}) {{\n{async_body}\n    }}\n",
        params = param_decls.join(", "),
    )
}

/// A minted union: sealed interface + one record per arm, per the
/// conventions doc. `arms` come from [`UnionSink`] with the null arm
/// already stripped.
pub(crate) fn render_union(
    ident: &str,
    arms: &[Ty],
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let arm_infos: Vec<(String, String)> = arms
        .iter()
        .map(|arm| {
            (
                format!("{}Value", crate::translate_ty::union_arm_token(arm)),
                translate_ty(arm, TyPosition::Boxed, ctx, sink),
            )
        })
        .collect();

    let permits: Vec<String> = arm_infos
        .iter()
        .map(|(record, _)| format!("{ident}.{record}"))
        .collect();

    let mut out = format!(
        "/**\n * Generated union type. Arms are records; switch on them (or use\n * `instanceof` pattern matching) to narrow.\n */\npublic sealed interface {ident} permits {} {{\n",
        permits.join(", ")
    );
    for (record, java_ty) in &arm_infos {
        out.push_str(&format!(
            "    record {record}({java_ty} value) implements {ident} {{}}\n"
        ));
    }
    out.push_str("}\n");
    out
}
