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
//!         "user.primitives.return_int", NAMES, new Object[] {}, "int");
//! }
//! ```
//!
//! Decode is type-directed: every binding passes a descriptor string
//! for its declared return type (see [`crate::translate_ty::descriptor_token`])
//! as the last argument, so the decoder resolves union arm order and
//! element types without trusting the wire shape. Generated bodies then
//! cast the decoded `Object` to the declared type; primitives unbox
//! implicitly on return.
//!
//! Class static and instance methods render as sibling bindings on the
//! value class itself: static methods as `static` bindings (same shape
//! as free functions), instance methods as non-static bindings that
//! prepend the receiver (`self` / `this`) to the runtime call so the
//! engine sees it as required param 0. A method's binding FQN is
//! `<class fqn>.<method name>`.
//!
//! Optional arguments render as an AWS-SDK-v2-style trailing configurator
//! overload beside the required-only pair: a nested `<Ident>$Opts` options
//! class (fluent boxed setters recording touched entries) plus a
//! `Consumer<<Ident>$Opts>`-taking sync/async overload. An untouched
//! optional is simply absent from the wire arrays (the engine evaluates
//! the BAML default); a touched-with-null optional contributes an explicit
//! BAML `null`. This preserves the omit-vs-null tri-state with no sentinel.
//!
//! Not yet emitted (later capabilities): explicit generic type-args
//! overloads.

use baml_codegen_types::{Class, Enum, Function, Ty};

use crate::{
    routing::java_identifier,
    translate_ty::{TranslateCtx, TyPosition, UnionSink, translate_ty},
};

/// Render a `///` docstring as a Javadoc block. Returns an empty
/// string when there is no docstring.
pub(crate) fn render_javadoc(docstring: Option<&str>, indent: &str) -> String {
    render_javadoc_with_throws(docstring, &[], indent)
}

/// Render a `///` docstring plus the callable's thrown-type contract as
/// a Javadoc block, one `@throws <UnqualifiedName>` tag per thrown type
/// (`throws_names`, in source order). The completeness doc commits the
/// throws contract to Javadoc `@throws` tags — there are no checked
/// exceptions on the JVM side (see ref-java-state-of-completeness.md).
///
/// The summary (when present) precedes the tags, separated by a blank
/// Javadoc line. Returns an empty string when there is neither a
/// docstring nor any thrown type, so a non-throwing, undocumented
/// callable renders no comment at all.
pub(crate) fn render_javadoc_with_throws(
    docstring: Option<&str>,
    throws_names: &[String],
    indent: &str,
) -> String {
    if docstring.is_none() && throws_names.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(indent);
    out.push_str("/**\n");
    if let Some(doc) = docstring {
        for line in doc.lines() {
            out.push_str(indent);
            out.push_str(" * ");
            // A literal `*/` inside the docstring would terminate the
            // Javadoc block early.
            out.push_str(&line.replace("*/", "* /"));
            out.push('\n');
        }
    }
    if !throws_names.is_empty() {
        // Blank separator between the summary and the `@throws` tags,
        // only when a summary precedes them.
        if docstring.is_some() {
            out.push_str(indent);
            out.push_str(" *\n");
        }
        for name in throws_names {
            out.push_str(indent);
            out.push_str(" * @throws ");
            out.push_str(name);
            out.push('\n');
        }
    }
    out.push_str(indent);
    out.push_str(" */\n");
    out
}

/// Collect the unqualified leaf names of the thrown types in a `throws`
/// `Ty`, in source order, de-duping exact-equal names. `Class`/`Enum`/
/// `TypeAlias` contribute their unqualified leaf name; a union contributes
/// each member's; anything else (primitives) contributes nothing. Mirrors
/// the Python generator's `collect_raises_names` so both SDKs surface the
/// same names.
pub(crate) fn collect_raises_names(throws: Option<&Ty>) -> Vec<String> {
    fn walk(ty: &Ty, out: &mut Vec<String>) {
        match ty {
            Ty::Class(name, ..) | Ty::Enum(name, ..) | Ty::TypeAlias(name, ..) => {
                let n = name.name().as_str().to_string();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Ty::Union(members, _) => members.iter().for_each(|m| walk(m, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    if let Some(ty) = throws {
        walk(ty, &mut out);
    }
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
/// static/instance method bindings, deep `equals`/`hashCode`.
/// `class_fqn` is the class's BAML FQN (`pkg.namespace.Name`); each
/// method binds to `<class_fqn>.<method name>`.
pub(crate) fn render_class(
    class: &Class,
    class_fqn: &str,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let mut out = render_javadoc(class.docstring.as_deref(), "");
    let ident = java_identifier(class.name.name().as_str());
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

    // Static and instance method bindings. Static methods (like free
    // functions) render as `static` bindings; instance methods are
    // non-static and prepend the receiver (`self` / `this`) to the
    // runtime call. Sorted by `(span, name)` for deterministic output,
    // matching the free-function fan-out.
    let mut statics: Vec<&Function> = class.static_methods.iter().collect();
    statics.sort_by(|a, b| {
        (a.origin.span_start, a.name.as_str()).cmp(&(b.origin.span_start, b.name.as_str()))
    });
    for m in statics {
        let fqn = format!("{class_fqn}.{}", m.name.as_str());
        out.push_str(&render_callable_pair(
            &fqn,
            m,
            Receiver::None,
            true,
            &class.generic_params,
            ctx,
            sink,
        ));
    }
    let mut instances: Vec<&Function> = class.instance_methods.iter().collect();
    instances.sort_by(|a, b| {
        (a.origin.span_start, a.name.as_str()).cmp(&(b.origin.span_start, b.name.as_str()))
    });
    for m in instances {
        let fqn = format!("{class_fqn}.{}", m.name.as_str());
        out.push_str(&render_callable_pair(
            &fqn,
            m,
            Receiver::This,
            false,
            &[],
            ctx,
            sink,
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
    let ident = java_identifier(enum_.name.name().as_str());
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
        "/**\n * Free functions declared in the BAML namespace backing\n * `{java_package}`.\n */\npublic final class {holder_ident} {{\n    private {holder_ident}() {{}}\n\n    static {{\n        baml_sdk.Baml.ensure();\n    }}\n"
    );
    for (fqn, function) in functions {
        out.push_str(&render_function_pair(fqn, function, ctx, sink));
    }
    out.push_str("}\n");
    out
}

/// Where the receiver goes on a callable binding. Free functions and
/// class static methods have no receiver; instance methods prepend
/// `self` (runtime param name) / `this` (runtime arg) so the engine
/// sees the receiver as required param 0.
#[derive(Clone, Copy)]
enum Receiver {
    None,
    This,
}

/// One sync + one `_async` static binding for a free function. Thin
/// wrapper over [`render_callable_pair`] with no receiver.
fn render_function_pair(
    fqn: &str,
    function: &Function,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    render_callable_pair(fqn, function, Receiver::None, true, &[], ctx, sink)
}

/// One sync + one `_async` binding for a callable — a free function, a
/// class static method, or a class instance method. `is_static` toggles
/// the `static` modifier; `receiver` prepends the instance receiver
/// (`self` / `this`) to the runtime param-names / args arrays. The
/// required-only pair takes just the defaultless arguments; when the
/// callable also has optional (defaulted) arguments, a second sync/async
/// overload pair taking a trailing `Consumer<<Ident>$Opts>` is appended,
/// along with the nested `<Ident>$Opts` options class (see
/// [`render_optional_configurator`]).
fn render_callable_pair(
    fqn: &str,
    function: &Function,
    receiver: Receiver,
    is_static: bool,
    class_generic_params: &[baml_base::Name],
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let ident = java_identifier(function.name.as_str());
    let required: Vec<_> = function
        .arguments
        .iter()
        .filter(|a| a.default.is_none())
        .collect();
    // Optionals (defaulted args) drive the configurator overload; they
    // never appear as positional Java parameters.
    let optionals: Vec<_> = function
        .arguments
        .iter()
        .filter(|a| a.default.is_some())
        .collect();

    let param_decls: Vec<String> = required
        .iter()
        .map(|a| {
            // `void` is legal only as a return type; a unit-typed
            // parameter (stdlib type-position args) boxes to Void.
            let mut ty = translate_ty(&a.ty, TyPosition::TopLevel, ctx, sink);
            if ty == "void" {
                ty = "java.lang.Void".to_string();
            }
            format!("{} {}", ty, java_identifier(a.name.as_str()))
        })
        .collect();

    // Runtime param-names / args arrays. An instance receiver is
    // prepended (`self` name, `this` arg) so it becomes required param
    // 0; the Java signature above never lists it.
    let mut param_names_java: Vec<String> = Vec::new();
    let mut arg_exprs: Vec<String> = Vec::new();
    if matches!(receiver, Receiver::This) {
        param_names_java.push(format!("{:?}", "self"));
        arg_exprs.push("this".to_string());
    }
    param_names_java.extend(required.iter().map(|a| format!("{:?}", a.name.as_str())));
    arg_exprs.extend(required.iter().map(|a| java_identifier(a.name.as_str())));

    let ret_top = translate_ty(&function.return_type, TyPosition::TopLevel, ctx, sink);
    let ret_boxed = translate_ty(&function.return_type, TyPosition::Boxed, ctx, sink);

    // Type-directed decode descriptor for the declared return type,
    // passed as the LAST runtime-call argument so the decoder can resolve
    // union arm order / element types without trusting the wire shape.
    let descriptor = crate::translate_ty::descriptor_token(&function.return_type, ctx.aliases);

    let names_literal = format!("new java.lang.String[] {{{}}}", param_names_java.join(", "));
    let args_literal = format!("new java.lang.Object[] {{{}}}", arg_exprs.join(", "));

    // The thrown-type contract renders as `@throws <UnqualifiedName>`
    // tags shared by the sync binding, its `_async` sibling, and any
    // optional-configurator overloads, so every entry point documents
    // the same contract.
    let raises = collect_raises_names(function.throws.as_ref());
    let doc = render_javadoc_with_throws(function.docstring.as_deref(), &raises, "    ");
    let static_kw = if is_static { "static " } else { "" };

    // Method-level generic parameters (`fn map<U>(...)`) must be
    // declared on the Java method; type args are inferred engine-side.
    // Static methods cannot reference class-level type variables, so a
    // static method on a generic class re-declares the class's params
    // at method level (harmless when unused).
    let mut generic_names: Vec<String> = Vec::new();
    if is_static {
        generic_names.extend(
            class_generic_params
                .iter()
                .map(|p| java_identifier(p.as_str())),
        );
    }
    for p in &function.generic_params {
        let id = java_identifier(p.as_str());
        if !generic_names.contains(&id) {
            generic_names.push(id);
        }
    }
    let generics_kw = if generic_names.is_empty() {
        String::new()
    } else {
        format!("<{}> ", generic_names.join(", "))
    };

    let sync_body = if ret_top == "void" {
        format!(
            "        baml_bridge.BamlFfi.callSync({fqn:?}, {names_literal}, {args_literal}, {descriptor:?});"
        )
    } else {
        format!(
            "        return ({ret_boxed}) baml_bridge.BamlFfi.callSync({fqn:?}, {names_literal}, {args_literal}, {descriptor:?});"
        )
    };

    // Async siblings return the boxed shape; the cast happens in a
    // thenApply so the future's element type is precise.
    let async_ret = format!("java.util.concurrent.CompletableFuture<{ret_boxed}>");
    let async_body = format!(
        "        return baml_bridge.BamlFfi.callAsync({fqn:?}, {names_literal}, {args_literal}, {descriptor:?}).thenApply(v$ -> ({ret_boxed}) v$);"
    );

    let mut out = format!(
        "\n{doc}    public {static_kw}{generics_kw}{ret_top} {ident}({params}) {{\n{sync_body}\n    }}\n\n{doc}    @SuppressWarnings(\"unchecked\")\n    public {static_kw}{generics_kw}{async_ret} {ident}_async({params}) {{\n{async_body}\n    }}\n",
        params = param_decls.join(", "),
    );

    // Optional-argument configurator: a second sync/async overload pair
    // (trailing `Consumer<<Ident>$Opts>`) plus the nested opts class. The
    // required-only pair above stays untouched, so omitting the configurator
    // still lets the engine evaluate BAML defaults.
    if !optionals.is_empty() {
        out.push_str(&render_optional_configurator(
            &ident,
            &optionals,
            &doc,
            static_kw,
            &generics_kw,
            &ret_top,
            &ret_boxed,
            &async_ret,
            fqn,
            &descriptor,
            &names_literal,
            &args_literal,
            &param_decls,
            ctx,
            sink,
        ));
    }

    out
}

/// Emit the optional-argument configurator for a callable that has ≥1
/// optional (defaulted) argument: the sync + async overloads taking a
/// trailing `java.util.function.Consumer<<Ident>$Opts>` after the required
/// params, and the nested `<Ident>$Opts` options class.
///
/// The opts class records each touched optional into an insertion-ordered
/// map (`$values`) plus a `$touched` set; its package-visible `$names` /
/// `$args` accessors append the touched optionals (in touch order) onto the
/// binding's base required arrays. An untouched optional is therefore absent
/// from the wire arrays (UNSET → engine default); a touched-with-null
/// optional contributes a `null` arg (explicit BAML `null`).
///
/// `names_literal` / `args_literal` are the binding's base required arrays
/// (already including the instance receiver when present); the overload
/// passes them as the accessor base.
#[allow(clippy::too_many_arguments)]
fn render_optional_configurator(
    ident: &str,
    optionals: &[&baml_codegen_types::FunctionArgument],
    doc: &str,
    static_kw: &str,
    generics_kw: &str,
    ret_top: &str,
    ret_boxed: &str,
    async_ret: &str,
    fqn: &str,
    descriptor: &str,
    names_literal: &str,
    args_literal: &str,
    param_decls: &[String],
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    // A generic callable's optional arg may reference class/method type
    // vars; the (static) opts class must then re-declare exactly those, so
    // its setter signatures and the `Consumer<...>` type resolve. The names
    // are already in scope on the enclosing binding (class-level for
    // instance methods, re-declared at method level for statics/free fns).
    let mut opt_tvs: Vec<String> = Vec::new();
    for a in optionals {
        crate::translate_ty::collect_type_vars(&a.ty, &mut opt_tvs);
    }
    let gu = if opt_tvs.is_empty() {
        String::new()
    } else {
        format!("<{}>", opt_tvs.join(", "))
    };
    let opts_ty = format!("{ident}$Opts{gu}");
    let cfg_ty = format!("java.util.function.Consumer<{opts_ty}>");

    // Overload params: the required params, then the trailing configurator.
    let mut overload_params: Vec<String> = param_decls.to_vec();
    overload_params.push(format!("{cfg_ty} $cfg"));
    let params = overload_params.join(", ");

    // Shared prologue: instantiate the opts holder and run the configurator.
    let prologue =
        format!("        {opts_ty} $opts = new {opts_ty}();\n        $cfg.accept($opts);\n");
    // The runtime call reuses the required-only descriptor and FQN; the
    // combined names/args come from the opts accessors over the base arrays.
    let call_names = format!("$opts.$names({names_literal})");
    let call_args = format!("$opts.$args({args_literal})");

    let sync_body = if ret_top == "void" {
        format!(
            "{prologue}        baml_bridge.BamlFfi.callSync({fqn:?}, {call_names}, {call_args}, {descriptor:?});"
        )
    } else {
        format!(
            "{prologue}        return ({ret_boxed}) baml_bridge.BamlFfi.callSync({fqn:?}, {call_names}, {call_args}, {descriptor:?});"
        )
    };
    let async_body = format!(
        "{prologue}        return baml_bridge.BamlFfi.callAsync({fqn:?}, {call_names}, {call_args}, {descriptor:?}).thenApply(v$ -> ({ret_boxed}) v$);"
    );

    // Fluent boxed setters. The wire key is the BAML arg name; the setter
    // method name is the Java-escaped identifier (they differ only when the
    // arg name is a Java keyword).
    let mut setters = String::new();
    for a in optionals {
        let boxed = translate_ty(&a.ty, TyPosition::Boxed, ctx, sink);
        let wire = a.name.as_str();
        let setter = java_identifier(wire);
        setters.push_str(&format!(
            "\n        public {opts_ty} {setter}({boxed} v) {{\n            this.$values.put({wire:?}, v);\n            this.$touched.add({wire:?});\n            return this;\n        }}\n"
        ));
    }

    format!(
        "\n{doc}    public {static_kw}{generics_kw}{ret_top} {ident}({params}) {{\n{sync_body}\n    }}\n\n{doc}    @SuppressWarnings(\"unchecked\")\n    public {static_kw}{generics_kw}{async_ret} {ident}_async({params}) {{\n{async_body}\n    }}\n\n    /**\n     * Configurator for the optional arguments of {{@code {ident}}}. Each\n     * fluent setter records one optional; only touched optionals reach the\n     * engine (untouched ⇒ BAML default, touched-with-{{@code null}} ⇒\n     * explicit BAML {{@code null}}).\n     */\n    public static final class {ident}$Opts{gu} {{\n        private final java.util.LinkedHashMap<java.lang.String, java.lang.Object> $values = new java.util.LinkedHashMap<>();\n        private final java.util.LinkedHashSet<java.lang.String> $touched = new java.util.LinkedHashSet<>();\n{setters}\n        java.lang.String[] $names(java.lang.String[] base) {{\n            java.lang.String[] out = java.util.Arrays.copyOf(base, base.length + this.$touched.size());\n            int i$ = base.length;\n            for (java.lang.String n$ : this.$touched) {{\n                out[i$++] = n$;\n            }}\n            return out;\n        }}\n\n        java.lang.Object[] $args(java.lang.Object[] base) {{\n            java.lang.Object[] out = java.util.Arrays.copyOf(base, base.length + this.$touched.size());\n            int i$ = base.length;\n            for (java.lang.String n$ : this.$touched) {{\n                out[i$++] = this.$values.get(n$);\n            }}\n            return out;\n        }}\n    }}\n"
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

    // A union whose arms reference type variables must itself be
    // generic over them (`T | Done` -> `UnionTOrDone<T>`); every arm
    // record carries the full param list so it can implement the
    // parameterized interface.
    let mut tvs: Vec<String> = Vec::new();
    for arm in arms {
        crate::translate_ty::collect_type_vars(arm, &mut tvs);
    }
    let generics = if tvs.is_empty() {
        String::new()
    } else {
        format!("<{}>", tvs.join(", "))
    };

    let permits: Vec<String> = arm_infos
        .iter()
        .map(|(record, _)| format!("{ident}.{record}"))
        .collect();

    let mut out = format!(
        "/**\n * Generated union type. Arms are records; switch on them (or use\n * `instanceof` pattern matching) to narrow.\n */\npublic sealed interface {ident}{generics} permits {} {{\n",
        permits.join(", ")
    );
    for (record, java_ty) in &arm_infos {
        out.push_str(&format!(
            "    record {record}{generics}({java_ty} value) implements {ident}{generics} {{}}\n"
        ));
    }
    out.push_str("}\n");
    out
}
