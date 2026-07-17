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

    // `final`: generated value classes carry exact-class value semantics — the
    // encoder keys its typemap on the concrete class, so a user subclass would
    // silently break inbound encode. This covers plain value classes, generic
    // classes, and `$stream` companions (all routed through here). Sealed-union
    // interfaces and their permitted records are emitted elsewhere (records are
    // already final).
    out.push_str(&format!("public final class {ident}{generics} {{\n"));

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

    // Generic classes carry the explicit-generics value surface: a reified
    // static factory `of(<one BamlType per class type param>, <fields…>)` that
    // constructs the instance AND binds its type-arg tokens in the runtime
    // side-table, plus a `bamlTypeArgs()` readback delegating to that table.
    // The plain constructor stays unbound (an unbound generic instance). Only
    // classes with type params get this; a plain constructor is enough for the
    // rest.
    if !class.generic_params.is_empty() {
        out.push_str(&render_reified_factory(
            &ident,
            &generics,
            class.generic_params.len(),
            &fields,
        ));
        out.push_str(&render_baml_type_args_readback(&fields));
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
        // Pass the class's own generic params: an instance method is never
        // `static`, so they are not re-declared at method level (that guard is
        // `is_static`), but a generic class drives the explicit-bag receiver
        // guard (a reified receiver is required to recover the class TypeVars).
        out.push_str(&render_callable_pair(
            &fqn,
            m,
            Receiver::This,
            false,
            &class.generic_params,
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

/// The reified static factory for a generic class: `of(BamlType t1[, t2…],
/// field1, field2…)` — one type-arg token per class type param (declaration
/// order), then the fields (declaration order). It constructs the instance via
/// the plain constructor and binds the tokens in the runtime side-table
/// (`TypeRegistry.bindTypeArgs`), so the value encodes with its concrete
/// `class_ty.type_args` and `bamlTypeArgs()` can read them back. `generics` is
/// the class's `<...>` clause (re-declared at method level — a static cannot
/// reference class type vars); `n_type_params` how many token params to take.
fn render_reified_factory(
    ident: &str,
    generics: &str,
    n_type_params: usize,
    fields: &[(String, String)],
) -> String {
    let ret_ty = format!("{ident}{generics}");
    let mut params: Vec<String> = Vec::with_capacity(n_type_params + fields.len());
    let mut token_names: Vec<String> = Vec::with_capacity(n_type_params);
    for i in 0..n_type_params {
        let tok = format!("$t{i}");
        params.push(format!("baml_bridge.BamlType {tok}"));
        token_names.push(tok);
    }
    for (f_ident, f_ty) in fields {
        params.push(format!("{f_ty} {f_ident}"));
    }
    let field_args: Vec<String> = fields.iter().map(|(f_ident, _)| f_ident.clone()).collect();

    format!(
        "\n    /**\n     * Reified factory: constructs a {ident} bound to the given type-arg\n     * tokens (one per class type parameter, in declaration order), binding\n     * them in the runtime side-table so the value carries its concrete\n     * {{@code class_ty.type_args}} on the wire and {{@link #bamlTypeArgs()}}\n     * reads them back. The plain constructor leaves an instance unbound.\n     */\n    public static {generics} {ret_ty} of({}) {{\n        {ret_ty} $instance = new {ident}<>({});\n        baml_bridge.TypeRegistry.bindTypeArgs($instance, java.util.List.of({}));\n        return $instance;\n    }}\n",
        params.join(", "),
        field_args.join(", "),
        token_names.join(", "),
    )
}

/// The `bamlTypeArgs()` readback for a generic class: returns the reified
/// type-arg tokens bound on this instance (via the reified factory or wire
/// decode), or an empty list for an unbound instance. Escapes to
/// `bamlTypeArgs$()` iff a BAML field of the class already claims the name
/// (its accessor would collide) — the `Fns` → `Fns$` policy.
fn render_baml_type_args_readback(fields: &[(String, String)]) -> String {
    let name = if fields.iter().any(|(f_ident, _)| f_ident == "bamlTypeArgs") {
        "bamlTypeArgs$"
    } else {
        "bamlTypeArgs"
    };
    format!(
        "\n    /**\n     * The reified generic type-arg tokens bound on this instance (in\n     * declaration order), or an empty list when it is unbound (constructed\n     * via the plain constructor, or decoded without wire type-args).\n     */\n    public java.util.List<baml_bridge.BamlType> {name}() {{\n        return baml_bridge.TypeRegistry.typeArgsOf(this);\n    }}\n"
    )
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
    // A callable with its OWN generic params (`fn map<U>(...)`) gains the
    // explicit-binding overloads that thread a trailing `baml_bridge.BamlTypes`
    // bag; class-only params (recovered from a reified receiver / an inferred
    // instance) are not the callee's own and do not trigger the bag.
    let is_generic = !function.generic_params.is_empty();
    let is_instance = matches!(receiver, Receiver::This);
    // An explicit-bag overload on an instance method of a GENERIC class needs a
    // reified receiver to recover the class TypeVars (the bare/inference path
    // instead recovers them from the argument values engine-side). Mirrors
    // Python's host-side check (test_instance_method_unparameterized_receiver_raises).
    let receiver_guard: Option<String> = if is_instance && !class_generic_params.is_empty() {
        Some(
            "        if (baml_bridge.TypeRegistry.typeArgsOf(this).isEmpty()) {\n            throw new java.lang.IllegalArgumentException(\"explicit type bindings on a generic method require a reified receiver so the class type args can be recovered\");\n        }\n"
                .to_string(),
        )
    } else {
        None
    };
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

    let async_ret = format!("java.util.concurrent.CompletableFuture<{ret_boxed}>");
    let params = param_decls.join(", ");
    // Java identifiers already claimed by BAML arguments — the synthetic
    // trailing params (`types`, `ctx`) must yield to them (go_codegen fixtures
    // name an argument `ctx`).
    let taken: Vec<String> = required
        .iter()
        .map(|a| java_identifier(a.name.as_str()))
        .collect();

    // Required-only base: the {types?}×{ctx?} overload family (fixed trailing
    // order `f(required…, types?, ctx?)`; the `types` overloads exist only for a
    // callable with its own generic params).
    let mut out = render_overload_family(
        &doc,
        static_kw,
        &generics_kw,
        &ident,
        &params,
        "",
        &ret_top,
        &ret_boxed,
        &async_ret,
        fqn,
        &names_literal,
        &args_literal,
        &descriptor,
        is_generic,
        receiver_guard.as_deref(),
        &taken,
    );

    // Optional-argument configurator base: the same overload family over a
    // trailing `Consumer<<Ident>$Opts>` (so `f(required…, opts?, types?, ctx?)`),
    // plus the nested opts class. The required-only base above stays untouched,
    // so omitting the configurator still lets the engine evaluate BAML defaults.
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
            &taken,
            is_generic,
            receiver_guard.as_deref(),
            ctx,
            sink,
        ));
    }

    out
}

/// Emit the {types?}×{ctx?} overload family for one call base (a fixed
/// required-or-configurator parameter list `params` plus its runtime
/// name/arg arrays). Four pairs at most, in the order the trailing params
/// stack — `f(base…)`, `f(base…, ctx)`, then (only when the callable has its
/// own generic params) `f(base…, types)`, `f(base…, types, ctx)` — each a
/// sync + `_async` [`render_method_pair`]. The `types` overloads carry
/// `receiver_guard` (an instance method on a generic class must have a reified
/// receiver) prepended to their body prologue.
#[allow(clippy::too_many_arguments)]
fn render_overload_family(
    doc: &str,
    static_kw: &str,
    generics_kw: &str,
    ident: &str,
    params: &str,
    prologue: &str,
    ret_top: &str,
    ret_boxed: &str,
    async_ret: &str,
    fqn: &str,
    call_names: &str,
    call_args: &str,
    descriptor: &str,
    is_generic: bool,
    receiver_guard: Option<&str>,
    taken: &[String],
) -> String {
    // `types` and `ctx` derive from distinct base names, so their yield-to-user
    // escapes can never collide with each other.
    let ctx_name = synthetic_param_name("ctx", taken);

    let mut out = render_method_pair(
        doc,
        static_kw,
        generics_kw,
        ident,
        params,
        prologue,
        ret_top,
        ret_boxed,
        async_ret,
        fqn,
        call_names,
        call_args,
        descriptor,
        None,
        None,
    );
    out.push_str(&render_method_pair(
        doc,
        static_kw,
        generics_kw,
        ident,
        params,
        prologue,
        ret_top,
        ret_boxed,
        async_ret,
        fqn,
        call_names,
        call_args,
        descriptor,
        None,
        Some(&ctx_name),
    ));

    if is_generic {
        let types_name = synthetic_param_name("types", taken);
        // The explicit-bag overloads carry the receiver guard (when any) in
        // front of the shared prologue.
        let types_prologue = match receiver_guard {
            Some(guard) => format!("{prologue}{guard}"),
            None => prologue.to_string(),
        };
        out.push_str(&render_method_pair(
            doc,
            static_kw,
            generics_kw,
            ident,
            params,
            &types_prologue,
            ret_top,
            ret_boxed,
            async_ret,
            fqn,
            call_names,
            call_args,
            descriptor,
            Some(&types_name),
            None,
        ));
        out.push_str(&render_method_pair(
            doc,
            static_kw,
            generics_kw,
            ident,
            params,
            &types_prologue,
            ret_top,
            ret_boxed,
            async_ret,
            fqn,
            call_names,
            call_args,
            descriptor,
            Some(&types_name),
            Some(&ctx_name),
        ));
    }

    out
}

/// Render one sync + `_async` method-pair entry point for a callable. The two
/// siblings share a body shape: the sync method `callSync`s (returning the
/// boxed result, or nothing for `void`); the async method returns the
/// caller-cancellable `callAsync` future reinterpreted to the declared element
/// type via a wildcard-bridge cast — deliberately NOT a `thenApply` stage,
/// which would hand back a derived future whose `cancel` no longer reaches the
/// engine call (see `BamlFfi.callAsync` / `CancellableCall`).
///
/// `params` is the already-joined Java parameter list (required args, plus an
/// optional configurator when present). The synthetic trailing params stack in
/// a fixed order after it: `types_name` (a `baml_bridge.BamlTypes` explicit
/// binding bag) then `ctx_name` (a `baml_bridge.BamlCallContext`), each
/// appended only when set (both names already escaped past colliding user
/// arguments). They thread to the runtime through the 6-arg
/// `callSync`/`callAsync` overload `(…, returnDesc, ctx, typeArgs)`: an absent
/// `ctx` passes `null` when a bag is present, and an absent bag drops back to
/// the 5-arg (`ctx`) or 4-arg (`returnDesc`) overload byte-for-byte.
///
/// `prologue` runs before the runtime call (opts instantiation and/or the
/// receiver guard, or empty). `call_names` / `call_args` are the runtime
/// name/arg array expressions (base literals, or the opts accessors).
fn synthetic_param_name(base: &str, taken: &[String]) -> String {
    let mut name = base.to_string();
    while taken.iter().any(|t| t == &name) {
        name.push('$');
    }
    name
}

#[allow(clippy::too_many_arguments)]
fn render_method_pair(
    doc: &str,
    static_kw: &str,
    generics_kw: &str,
    ident: &str,
    params: &str,
    prologue: &str,
    ret_top: &str,
    ret_boxed: &str,
    async_ret: &str,
    fqn: &str,
    call_names: &str,
    call_args: &str,
    descriptor: &str,
    types_name: Option<&str>,
    ctx_name: Option<&str>,
) -> String {
    // Trailing synthetic params in fixed order: `types` (BamlTypes) then `ctx`
    // (BamlCallContext).
    let mut trailing = String::new();
    if let Some(name) = types_name {
        trailing.push_str(&format!(", baml_bridge.BamlTypes {name}"));
    }
    if let Some(name) = ctx_name {
        trailing.push_str(&format!(", baml_bridge.BamlCallContext {name}"));
    }
    let sig_params = if params.is_empty() {
        trailing.strip_prefix(", ").unwrap_or("").to_string()
    } else {
        format!("{params}{trailing}")
    };

    // Runtime-call suffix after the descriptor. The bag routes through the 6-arg
    // overload `(…, ctx, typeArgs)`, so a bag with no ctx passes `null` for ctx;
    // no bag keeps the 5-arg (`ctx`) or 4-arg (`returnDesc`) form unchanged.
    let call_suffix = match (ctx_name, types_name) {
        (None, None) => String::new(),
        (Some(c), None) => format!(", {c}"),
        (None, Some(t)) => format!(", null, {t}"),
        (Some(c), Some(t)) => format!(", {c}, {t}"),
    };

    let sync_call = format!(
        "baml_bridge.BamlFfi.callSync({fqn:?}, {call_names}, {call_args}, {descriptor:?}{call_suffix})"
    );
    let sync_body = if ret_top == "void" {
        format!("{prologue}        {sync_call};")
    } else {
        format!("{prologue}        return ({ret_boxed}) {sync_call};")
    };
    let async_body = format!(
        "{prologue}        return (java.util.concurrent.CompletableFuture<{ret_boxed}>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.callAsync({fqn:?}, {call_names}, {call_args}, {descriptor:?}{call_suffix});"
    );

    format!(
        "\n{doc}    public {static_kw}{generics_kw}{ret_top} {ident}({sig_params}) {{\n{sync_body}\n    }}\n\n{doc}    @SuppressWarnings(\"unchecked\")\n    public {static_kw}{generics_kw}{async_ret} {ident}_async({sig_params}) {{\n{async_body}\n    }}\n"
    )
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
    taken: &[String],
    is_generic: bool,
    receiver_guard: Option<&str>,
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

    // The `types`/`ctx` params must dodge user args AND this overload's own
    // synthetics.
    let mut taken_cfg = taken.to_vec();
    taken_cfg.push("$cfg".to_string());
    taken_cfg.push("$opts".to_string());

    // Shared prologue: instantiate the opts holder and run the configurator.
    let prologue =
        format!("        {opts_ty} $opts = new {opts_ty}();\n        $cfg.accept($opts);\n");
    // The runtime call reuses the required-only descriptor and FQN; the
    // combined names/args come from the opts accessors over the base arrays.
    let call_names = format!("$opts.$names({names_literal})");
    let call_args = format!("$opts.$args({args_literal})");

    // The configurator base's own {types?}×{ctx?} overload family.
    let mut out = render_overload_family(
        doc,
        static_kw,
        generics_kw,
        ident,
        &params,
        &prologue,
        ret_top,
        ret_boxed,
        async_ret,
        fqn,
        &call_names,
        &call_args,
        descriptor,
        is_generic,
        receiver_guard,
        &taken_cfg,
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

    out.push_str(&format!(
        "\n    /**\n     * Configurator for the optional arguments of {{@code {ident}}}. Each\n     * fluent setter records one optional; only touched optionals reach the\n     * engine (untouched ⇒ BAML default, touched-with-{{@code null}} ⇒\n     * explicit BAML {{@code null}}).\n     */\n    public static final class {ident}$Opts{gu} {{\n        private final java.util.LinkedHashMap<java.lang.String, java.lang.Object> $values = new java.util.LinkedHashMap<>();\n        private final java.util.LinkedHashSet<java.lang.String> $touched = new java.util.LinkedHashSet<>();\n{setters}\n        java.lang.String[] $names(java.lang.String[] base) {{\n            java.lang.String[] out = java.util.Arrays.copyOf(base, base.length + this.$touched.size());\n            int i$ = base.length;\n            for (java.lang.String n$ : this.$touched) {{\n                out[i$++] = n$;\n            }}\n            return out;\n        }}\n\n        java.lang.Object[] $args(java.lang.Object[] base) {{\n            java.lang.Object[] out = java.util.Arrays.copyOf(base, base.length + this.$touched.size());\n            int i$ = base.length;\n            for (java.lang.String n$ : this.$touched) {{\n                out[i$++] = this.$values.get(n$);\n            }}\n            return out;\n        }}\n    }}\n"
    ));
    out
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
