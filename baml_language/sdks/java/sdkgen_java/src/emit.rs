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
//! Decode is type-directed: every binding passes a typed
//! `baml_bridge.BamlType` for its declared return type (see
//! [`crate::translate_ty::descriptor_expr`]) as the last argument — pooled into
//! a per-holder `private static final $RET{n}` constant, or the literal `null`
//! for a wire-driven return — so the decoder resolves union arm order and
//! element types without trusting the wire shape. Generated bodies then cast the
//! decoded `Object` to the declared type; primitives unbox implicitly on return.
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

use std::collections::BTreeSet;

use baml_codegen_types::{Class, CodegenFunctionParamMode, Enum, Function, Ty};

use crate::{
    routing::{java_identifier, java_method_identifier},
    translate_ty::{CallbackInterface, TranslateCtx, TyPosition, UnionSink, translate_ty},
};

/// Per-holder pool of return-type decode descriptors. Each distinct descriptor
/// `BamlType` builder expression is emitted once as a `private static final
/// baml_bridge.BamlType $RET{n}` constant (allocated at class load, not per
/// call), and a binding references it by name; identical descriptors within a
/// holder share one constant. A wire-driven descriptor is not pooled — the
/// binding passes the literal `null`.
#[derive(Default)]
pub(crate) struct DescriptorPool {
    /// Distinct descriptor expressions, in first-seen order; index → `$RET{i}`.
    exprs: Vec<String>,
}

impl DescriptorPool {
    /// Intern a descriptor builder expression, returning the constant name that
    /// references it (`$RET{n}`).
    fn intern(&mut self, expr: String) -> String {
        let idx = self
            .exprs
            .iter()
            .position(|e| *e == expr)
            .unwrap_or_else(|| {
                self.exprs.push(expr);
                self.exprs.len() - 1
            });
        format!("$RET{idx}")
    }

    /// The `private static final baml_bridge.BamlType $RET{n} = …;` declarations,
    /// in constant order (empty when nothing was pooled).
    fn constants(&self) -> String {
        let mut out = String::new();
        for (i, expr) in self.exprs.iter().enumerate() {
            out.push_str(&format!(
                "    private static final baml_bridge.BamlType $RET{i} = {expr};\n"
            ));
        }
        out
    }
}

/// The `_async` sibling name for a callable `ident`, escaped past a collision:
/// if the enclosing group (package for free functions, class for methods)
/// already declares a callable literally named `{ident}_async` (a user function
/// whose own sync binding would be that method), the SYNTHETIC sibling escapes
/// to `{ident}_async$` (repeating `$` per the trailing-`$` collision policy) so
/// the two never emit the same method signature.
fn async_sibling_ident(ident: &str, declared: &BTreeSet<String>) -> String {
    let mut name = format!("{ident}_async");
    while declared.contains(&name) {
        name.push('$');
    }
    name
}

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
            // `javac` decodes `\uXXXX` unicode escapes over the raw source
            // BEFORE lexing — inside comments too — so a docstring carrying the
            // escape form of `*/` (`*/`) would terminate the Javadoc
            // block early even though no literal `*/` is present. Neutralize any
            // unicode-escape run first, THEN guard the literal `*/`.
            let safe = neutralize_unicode_escapes(line).replace("*/", "* /");
            out.push_str(indent);
            // A blank body line renders as a bare ` *` (no trailing space) —
            // the rolled `Attributes:`/`Members:` blocks separate the summary
            // from the section with one, and trailing whitespace is noise.
            if safe.is_empty() {
                out.push_str(" *\n");
            } else {
                out.push_str(" * ");
                out.push_str(&safe);
                out.push('\n');
            }
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

/// Compose the rolled-up class/enum doc TEXT — the summary plus a folded
/// `Attributes:` (classes) / `Members:` (enums) section listing every
/// field/variant — mirroring the Python emitter's `format_class_docstring`,
/// but as plain text with no fences: [`render_javadoc`] wraps it in a
/// `/** */` block and handles the unicode-escape / `*/` neutralization. There
/// are no per-member Javadoc blocks — the section is the sole home for
/// field/variant `///` docs, so both SDKs surface the same rollup.
///
/// `members` is `(name, Option<doc>)` for every field/variant in declaration
/// order. Section-visibility follows the "any-doc" rule: the section appears
/// iff at least one member carries a `///`; when it appears **every** member is
/// listed — documented as `name: doc` (continuation lines indented under the
/// name), undocumented as a bare `name` (no trailing colon). A summary-only
/// type (class/enum `///` but no member documented) suppresses the section
/// entirely and renders the summary verbatim. Returns `None` when there is
/// nothing to render (no summary and no member documented).
pub(crate) fn format_rolled_docstring(
    summary: Option<&str>,
    members: &[(String, Option<String>)],
    section_label: &str,
) -> Option<String> {
    let summary = summary.filter(|s| !s.is_empty());
    let any_member_doc = members.iter().any(|(_, d)| d.is_some());

    if summary.is_none() && !any_member_doc {
        return None;
    }
    // Summary only (no member documented): the section is suppressed
    // entirely — even a multi-line summary renders verbatim.
    if !any_member_doc {
        return summary.map(std::string::ToString::to_string);
    }

    let mut lines: Vec<String> = Vec::new();
    if let Some(s) = summary {
        lines.extend(s.lines().map(std::string::ToString::to_string));
        // Blank separator between the summary and the section.
        lines.push(String::new());
    }
    lines.push(format!("{section_label}:"));
    for (name, doc) in members {
        // An empty docstring (`Some("")`) falls through to the bare-name form.
        match doc.as_deref().filter(|d| !d.is_empty()) {
            Some(d) => {
                let mut member_lines = d.lines();
                let first = member_lines.next().unwrap_or("");
                lines.push(format!("    {name}: {first}"));
                for line in member_lines {
                    lines.push(format!("        {line}"));
                }
            }
            None => lines.push(format!("    {name}")),
        }
    }
    Some(lines.join("\n"))
}

/// Break Java unicode escapes (`\uXXXX`, including multi-`u` markers) in
/// docstring text so `javac`'s pre-lex unicode-escape pass cannot decode them
/// into comment-terminating `*/` (or any other structural character): Java
/// processes `\u` escapes over the whole source — comments included — before
/// the surrounding `*/` guard can help, so `*/` would otherwise close
/// the Javadoc block.
///
/// Only an *eligible* backslash begins an escape (JLS 3.3: eligible iff preceded
/// by an even number of backslashes — so within a run of `n` backslashes the
/// last one is eligible iff `n` is odd) and only when it is followed by one or
/// more `u` and 4 hex digits. We insert a single space between that backslash
/// and the `u` run, breaking the `\`↔`u` adjacency; a space can never begin a
/// new escape, so this is safe regardless of surrounding backslash parity.
/// Non-escape text (a lone `\u`, `\username`, an inert `\\uXXXX`) is left
/// untouched, so ordinary docstrings render byte-identically.
fn neutralize_unicode_escapes(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            // Emit the whole run of consecutive backslashes.
            let run_start = i;
            while i < chars.len() && chars[i] == '\\' {
                out.push('\\');
                i += 1;
            }
            let run_len = i - run_start;
            // The trailing backslash is the only one immediately followed by the
            // next char; it is eligible to start an escape iff the run length is
            // odd. Break the escape when it begins a `u`+ hex4 sequence.
            if run_len % 2 == 1 && starts_unicode_escape(&chars, i) {
                out.push(' ');
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Whether `chars[i..]` is a Java unicode-escape tail: one or more `u` markers
/// followed by exactly 4 hex digits.
fn starts_unicode_escape(chars: &[char], mut i: usize) -> bool {
    let mut saw_u = false;
    while i < chars.len() && chars[i] == 'u' {
        saw_u = true;
        i += 1;
    }
    saw_u && i + 4 <= chars.len() && chars[i..i + 4].iter().all(char::is_ascii_hexdigit)
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
    /// `Double.compare(a, b) == 0` / `Double.hashCode`. A `double` must NOT use
    /// `==`: `NaN == NaN` is false (a round-tripped NaN would not equal itself)
    /// and `+0.0 == -0.0` is true while their `Double.hashCode`s differ (an
    /// equals/hashCode contract violation). `Double.compare`/`Double.hashCode`
    /// give total-order, contract-consistent semantics.
    Double,
    /// `Arrays.equals` / `Arrays.hashCode`.
    ByteArray,
    /// `Objects.equals` / `Objects.hash`.
    Reference,
}

fn field_eq(java_ty: &str) -> FieldEq {
    match java_ty {
        "long" | "boolean" => FieldEq::Primitive,
        "double" => FieldEq::Double,
        "byte[]" => FieldEq::ByteArray,
        _ => FieldEq::Reference,
    }
}

/// Full value-class body: fields, canonical constructor, accessors,
/// static/instance method bindings, deep `equals`/`hashCode`.
/// `class_fqn` is the class's BAML FQN (`pkg.namespace.Name`); each
/// method binds to `<class_fqn>.<method name>`. `anchor_fqn` is the root
/// runtime anchor's FQN (`baml_sdk.Baml`, or `baml_sdk.Baml$` when a user
/// type claims the `Baml` name) — a class that carries method bindings emits
/// a `static { <anchor_fqn>.ensure(); }` block so its FIRST touched
/// entrypoint (e.g. `Greeter.create()`) boots the runtime, the analog of the
/// `Fns` holder's own static init.
pub(crate) fn render_class(
    class: &Class,
    class_fqn: &str,
    anchor_fqn: &str,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    // Field `///` docs fold into the class Javadoc's `Attributes:` section
    // (Python parity — see [`format_rolled_docstring`]); fields carry no
    // per-member Javadoc block.
    let doc_members: Vec<(String, Option<String>)> = class
        .properties
        .iter()
        .map(|p| (java_identifier(p.name.as_str()), p.docstring.clone()))
        .collect();
    let rolled = format_rolled_docstring(class.docstring.as_deref(), &doc_members, "Attributes");
    let mut out = render_javadoc(rolled.as_deref(), "");
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

    // (ident, RAW java type) per property, in declaration order. The raw type
    // (no `@Nullable`) drives `equals`/`hashCode` dispatch — a nullable
    // `uint8array?` field is still `byte[]` there and must use `Arrays.equals`.
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

    // The DISPLAY type per property: the raw type with a JSpecify `@Nullable`
    // woven in when the BAML field type is nullable. Used for the public value
    // surface — field declaration, canonical constructor, accessor, reified
    // factory — so a Kotlin/IDE consumer sees the field's real nullness instead
    // of a platform type. (`equals`/`hashCode` keep the raw `fields` above.)
    let display_fields: Vec<(String, String)> = class
        .properties
        .iter()
        .zip(&fields)
        .map(|(p, (f_ident, raw))| {
            let ty = if crate::translate_ty::is_nullable(&p.ty, ctx.aliases) {
                crate::translate_ty::annotate_nullable(raw)
            } else {
                raw.clone()
            };
            (f_ident.clone(), ty)
        })
        .collect();

    // `final`: generated value classes carry exact-class value semantics — the
    // encoder keys its typemap on the concrete class, so a user subclass would
    // silently break inbound encode. This covers plain value classes, generic
    // classes, and PPIR `$stream` partial models (all routed through here). Sealed-union
    // interfaces and their permitted records are emitted elsewhere (records are
    // already final).
    out.push_str(&format!("public final class {ident}{generics} {{\n"));

    // A class that carries method bindings (static or instance) must boot the
    // runtime when it is the FIRST generated symbol a program touches — merely
    // referencing `Greeter.class` does not run a static initializer, but
    // invoking `Greeter.create()` does. Pure value classes (no bindings) are
    // only ever reached via decode, which already went through the runtime, so
    // they skip this. `ensure()` is idempotent and the anchor's own init
    // registers types by string name (never touching this class's statics), so
    // there is no init-order cycle.
    let has_bindings = !class.static_methods.is_empty() || !class.instance_methods.is_empty();
    if has_bindings {
        out.push_str(&format!(
            "    static {{\n        {anchor_fqn}.ensure();\n    }}\n\n"
        ));
    }

    for (f_ident, f_ty) in &display_fields {
        out.push_str(&format!("    private final {f_ty} {f_ident};\n"));
    }

    // Canonical all-args constructor, field declaration order.
    let params: Vec<String> = display_fields
        .iter()
        .map(|(f_ident, f_ty)| format!("{f_ty} {f_ident}"))
        .collect();
    out.push_str(&format!("\n    public {ident}({}) {{\n", params.join(", ")));
    for (f_ident, _) in &fields {
        out.push_str(&format!("        this.{f_ident} = {f_ident};\n"));
    }
    out.push_str("    }\n");

    // PreserveCase accessors — the nullable ones carry `@Nullable` on their
    // return type (the primary Kotlin-visible nullness signal). The ACCESSOR
    // name goes through `java_method_identifier`: a field named `wait` is a
    // legal field but `wait()` cannot override `java.lang.Object`'s final
    // `wait()`, so the accessor becomes `wait$()` while the field it reads
    // keeps its own name.
    for (f_ident, f_ty) in &display_fields {
        let accessor = java_method_identifier(f_ident);
        out.push_str(&format!(
            "\n    public {f_ty} {accessor}() {{\n        return this.{f_ident};\n    }}\n"
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
            &display_fields,
        ));
        out.push_str(&render_baml_type_args_readback(&fields));
    }

    // The declared-callable ident set for this class (static + instance method
    // names) — used to escape a synthetic `_async` sibling that would collide
    // with a user method literally named `{ident}_async`.
    let method_idents: BTreeSet<String> = class
        .static_methods
        .iter()
        .chain(&class.instance_methods)
        .map(|m| java_method_identifier(m.name.as_str()))
        .collect();

    // Static and instance method bindings. Static methods (like free
    // functions) render as `static` bindings; instance methods are
    // non-static and prepend the receiver (`self` / `this`) to the
    // runtime call. Sorted by `(span, name)` for deterministic output,
    // matching the free-function fan-out. The method bodies are buffered so the
    // return-descriptor constants they pool can be emitted ahead of them.
    let mut pool = DescriptorPool::default();
    let mut methods = String::new();
    let mut statics: Vec<&Function> = class.static_methods.iter().collect();
    statics.sort_by(|a, b| {
        (a.origin.span_start, a.name.as_str()).cmp(&(b.origin.span_start, b.name.as_str()))
    });
    for m in statics {
        let fqn = format!("{class_fqn}.{}", m.name.as_str());
        methods.push_str(&render_callable_pair(
            &fqn,
            m,
            Receiver::None,
            true,
            &class.generic_params,
            &method_idents,
            &mut pool,
            ctx,
            sink,
            CallProjection::Direct,
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
        methods.push_str(&render_callable_pair(
            &fqn,
            m,
            Receiver::This,
            false,
            &class.generic_params,
            &method_idents,
            &mut pool,
            ctx,
            sink,
            CallProjection::Direct,
        ));
    }
    out.push_str(&pool.constants());
    out.push_str(&methods);

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
                FieldEq::Double => {
                    format!("java.lang.Double.compare(this.{f_ident}, other.{f_ident}) == 0")
                }
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
            FieldEq::Double => format!("java.lang.Double.hashCode(this.{f_ident})"),
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
    // Variant `///` docs fold into the enum Javadoc's `Members:` section
    // (Python parity — see [`format_rolled_docstring`]); variants carry no
    // per-member Javadoc block.
    let doc_members: Vec<(String, Option<String>)> = enum_
        .variants
        .iter()
        .map(|v| (java_identifier(v.name.as_str()), v.docstring.clone()))
        .collect();
    let rolled = format_rolled_docstring(enum_.docstring.as_deref(), &doc_members, "Members");
    let mut out = render_javadoc(rolled.as_deref(), "");
    let ident = java_identifier(enum_.name.name().as_str());
    out.push_str("public enum ");
    out.push_str(&ident);
    out.push_str(" {\n");
    for variant in &enum_.variants {
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
/// `functions` is `(fqn, function)` in deterministic order. `anchor_fqn` is
/// the root runtime anchor's FQN (`baml_sdk.Baml`, or `baml_sdk.Baml$` when a
/// user type claims the `Baml` name) whose static init boots the runtime.
pub(crate) fn render_fns_holder(
    holder_ident: &str,
    java_package: &str,
    anchor_fqn: &str,
    functions: &[(String, &Function)],
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let mut out = format!(
        "/**\n * Free functions declared in the BAML namespace backing\n * `{java_package}`.\n */\npublic final class {holder_ident} {{\n    private {holder_ident}() {{}}\n\n    static {{\n        {anchor_fqn}.ensure();\n    }}\n"
    );
    // The declared-callable ident set for this package's free functions — used
    // to escape a synthetic `_async` sibling that would collide with a user
    // function literally named `{ident}_async`.
    let fn_idents: BTreeSet<String> = functions
        .iter()
        .map(|(_, f)| java_identifier(f.name.as_str()))
        .collect();
    // Buffer the bindings so their pooled return-descriptor constants can be
    // emitted ahead of them.
    let mut pool = DescriptorPool::default();
    let mut methods = String::new();
    for (fqn, function) in functions {
        methods.push_str(&render_function_pair(
            fqn, function, &fn_idents, &mut pool, ctx, sink,
        ));
    }
    out.push_str(&pool.constants());
    out.push_str(&methods);
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

#[derive(Clone, Copy)]
enum CallProjection {
    Direct,
    Spec,
    Stream,
}

struct ReturnedCallable {
    raw_type: String,
    parameter_names: String,
    return_descriptor: String,
}

/// One sync + one `_async` static binding for a free function. Thin
/// wrapper over [`render_callable_pair`] with no receiver.
fn render_function_pair(
    fqn: &str,
    function: &Function,
    sibling_idents: &BTreeSet<String>,
    pool: &mut DescriptorPool,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    let mut out = render_callable_pair(
        fqn,
        function,
        Receiver::None,
        true,
        &[],
        sibling_idents,
        pool,
        ctx,
        sink,
        CallProjection::Direct,
    );

    if let Some(spec) = &function.operations.spec {
        let mut projected = function.clone();
        projected.name = baml_base::Name::new(format!("{}_spec", function.name));
        projected.arguments.retain(|argument| !argument.injected);
        projected.return_type = spec.return_type.clone();
        projected.operations = baml_codegen_types::FunctionOperations::DIRECT;
        out.push_str(&render_callable_pair(
            fqn,
            &projected,
            Receiver::None,
            true,
            &[],
            sibling_idents,
            pool,
            ctx,
            sink,
            CallProjection::Spec,
        ));
    }

    if let Some(stream) = &function.operations.stream {
        let mut projected = function.clone();
        projected.name = baml_base::Name::new(format!("{}_stream", function.name));
        projected.arguments.retain(|argument| !argument.injected);
        projected
            .arguments
            .extend(stream.control_arguments.iter().cloned());
        projected.return_type = stream.return_type.clone();
        projected.operations = baml_codegen_types::FunctionOperations::DIRECT;
        out.push_str(&render_callable_pair(
            fqn,
            &projected,
            Receiver::None,
            true,
            &[],
            sibling_idents,
            pool,
            ctx,
            sink,
            CallProjection::Stream,
        ));
    }

    out
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
#[allow(clippy::too_many_arguments)]
fn render_callable_pair(
    fqn: &str,
    function: &Function,
    receiver: Receiver,
    is_static: bool,
    class_generic_params: &[baml_base::Name],
    sibling_idents: &BTreeSet<String>,
    pool: &mut DescriptorPool,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
    projection: CallProjection,
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
    let ident = java_method_identifier(function.name.as_str());
    // The `_async` sibling name, escaped past a user callable that already
    // claims `{ident}_async` (see [`async_sibling_ident`]).
    let async_ident = async_sibling_ident(&ident, sibling_idents);
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
            // A nullable required param (`x: T?`) carries `@Nullable` so callers
            // (Kotlin especially) see it accepts `null`.
            if crate::translate_ty::is_nullable(&a.ty, ctx.aliases) {
                ty = crate::translate_ty::annotate_nullable(&ty);
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
    for a in &required {
        let value = java_identifier(a.name.as_str());
        let mut type_vars = Vec::new();
        crate::translate_ty::collect_type_vars(&a.ty, &mut type_vars);
        let encoded = if !type_vars.is_empty() {
            value
        } else if let Some(expr) = typed_callable_expr(&a.ty, &value, pool, ctx) {
            // A callable argument carries its declared param/return descriptors
            // so the dispatch path can honor the generated signature.
            expr
        } else if needs_inbound_descriptor(&a.ty, ctx) {
            match crate::translate_ty::descriptor_expr_opt(&a.ty, ctx.aliases) {
                Some(expr) => format!(
                    "new baml_bridge.BamlTypedValue({value}, {})",
                    pool.intern(expr)
                ),
                None => value,
            }
        } else {
            value
        };
        arg_exprs.push(encoded);
    }

    if let Ty::Function { params, .. } = &function.return_type {
        if params
            .iter()
            .any(|parameter| parameter.mode == CodegenFunctionParamMode::Optional)
        {
            panic!(
                "Java generation does not yet support optional parameters on returned callable `{fqn}`"
            );
        }
    }

    let mut ret_top = translate_ty(&function.return_type, TyPosition::TopLevel, ctx, sink);
    let mut ret_boxed = translate_ty(&function.return_type, TyPosition::Boxed, ctx, sink);
    // A nullable return (`-> T?`) carries `@Nullable` on both the sync signature
    // and the async future's element type (`CompletableFuture<@Nullable T>`). A
    // nullable type is always a boxed reference, so `ret_top == "void"` (the
    // void-return sentinel checked downstream) can never be a nullable type.
    if crate::translate_ty::is_nullable(&function.return_type, ctx.aliases) {
        ret_top = crate::translate_ty::annotate_nullable(&ret_top);
        ret_boxed = crate::translate_ty::annotate_nullable(&ret_boxed);
    }

    let returned_callable = match &function.return_type {
        Ty::Function { params, ret, .. } => {
            let raw_type = ret_boxed
                .split_once('<')
                .map_or_else(|| ret_boxed.clone(), |(raw, _)| raw.to_string());
            let parameter_names = format!(
                "new java.lang.String[] {{{}}}",
                params
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| {
                        format!(
                            "{:?}",
                            parameter
                                .name
                                .as_ref()
                                .map_or_else(|| format!("arg{index}"), ToString::to_string)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let return_descriptor = match crate::translate_ty::descriptor_expr_opt(ret, ctx.aliases)
            {
                None => "null".to_string(),
                Some(expr) => pool.intern(expr),
            };
            Some(ReturnedCallable {
                raw_type,
                parameter_names,
                return_descriptor,
            })
        }
        _ => None,
    };

    // Type-directed decode descriptor for the declared return type, passed as the
    // LAST runtime-call argument so the decoder resolves union arm order / element
    // types without trusting the wire shape. It is a typed `baml_bridge.BamlType`,
    // pooled into a per-holder `private static final` constant referenced by name;
    // a wholly wire-driven return passes the literal `null`.
    let descriptor =
        match crate::translate_ty::descriptor_expr_opt(&function.return_type, ctx.aliases) {
            None => "null".to_string(),
            Some(expr) => pool.intern(expr),
        };

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
        &async_ident,
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
        returned_callable.as_ref(),
        projection,
    );

    // Optional-argument configurator base: the same overload family over a
    // trailing `Consumer<<Ident>$Opts>` (so `f(required…, opts?, types?, ctx?)`),
    // plus the nested opts class. The required-only base above stays untouched,
    // so omitting the configurator still lets the engine evaluate BAML defaults.
    if !optionals.is_empty() {
        out.push_str(&render_optional_configurator(
            &ident,
            &async_ident,
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
            pool,
            returned_callable.as_ref(),
            projection,
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
    async_ident: &str,
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
    returned_callable: Option<&ReturnedCallable>,
    projection: CallProjection,
) -> String {
    // `types` and `ctx` derive from distinct base names, so their yield-to-user
    // escapes can never collide with each other.
    let ctx_name = synthetic_param_name("ctx", taken);

    let mut out = render_method_pair(
        doc,
        static_kw,
        generics_kw,
        ident,
        async_ident,
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
        returned_callable,
        projection,
    );
    out.push_str(&render_method_pair(
        doc,
        static_kw,
        generics_kw,
        ident,
        async_ident,
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
        returned_callable,
        projection,
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
            async_ident,
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
            returned_callable,
            projection,
        ));
        out.push_str(&render_method_pair(
            doc,
            static_kw,
            generics_kw,
            ident,
            async_ident,
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
            returned_callable,
            projection,
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
    async_ident: &str,
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
    returned_callable: Option<&ReturnedCallable>,
    projection: CallProjection,
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

    let sync_call = match projection {
        CallProjection::Direct => format!(
            "baml_bridge.BamlFfi.callSync({fqn:?}, {call_names}, {call_args}, {descriptor}{call_suffix})"
        ),
        CallProjection::Spec => {
            format!(
                "baml_bridge.BamlFfi.callSyncOperation({fqn:?}, {call_names}, {call_args}, {descriptor}, {}, {}, baml_bridge.BamlFunctionOperation.SPEC)",
                ctx_name.unwrap_or("null"),
                types_name.unwrap_or("null"),
            )
        }
        CallProjection::Stream => {
            format!(
                "baml_bridge.BamlFfi.callSyncOperation({fqn:?}, {call_names}, {call_args}, {descriptor}, {}, {}, baml_bridge.BamlFunctionOperation.STREAM)",
                ctx_name.unwrap_or("null"),
                types_name.unwrap_or("null"),
            )
        }
    };
    let sync_body = if ret_top == "void" {
        format!("{prologue}        {sync_call};")
    } else if let Some(callable) = returned_callable {
        format!(
            "{prologue}        return ({ret_boxed}) baml_bridge.BamlFfi.returnedClosure({}.class, {sync_call}, {}, {});",
            callable.raw_type, callable.parameter_names, callable.return_descriptor
        )
    } else {
        format!("{prologue}        return ({ret_boxed}) {sync_call};")
    };
    let async_call = match projection {
        CallProjection::Direct => format!(
            "(java.util.concurrent.CompletableFuture<java.lang.Object>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.callAsync({fqn:?}, {call_names}, {call_args}, {descriptor}{call_suffix})"
        ),
        CallProjection::Spec => {
            format!(
                "(java.util.concurrent.CompletableFuture<java.lang.Object>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.callAsyncOperation({fqn:?}, {call_names}, {call_args}, {descriptor}, {}, {}, baml_bridge.BamlFunctionOperation.SPEC)",
                ctx_name.unwrap_or("null"),
                types_name.unwrap_or("null"),
            )
        }
        CallProjection::Stream => {
            format!(
                "(java.util.concurrent.CompletableFuture<java.lang.Object>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.callAsyncOperation({fqn:?}, {call_names}, {call_args}, {descriptor}, {}, {}, baml_bridge.BamlFunctionOperation.STREAM)",
                ctx_name.unwrap_or("null"),
                types_name.unwrap_or("null"),
            )
        }
    };
    let async_body = if let Some(callable) = returned_callable {
        format!(
            "{prologue}        return (java.util.concurrent.CompletableFuture<{ret_boxed}>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.returnedClosureAsync({async_call}, {}.class, {}, {});",
            callable.raw_type, callable.parameter_names, callable.return_descriptor,
        )
    } else if matches!(projection, CallProjection::Direct) {
        format!(
            "{prologue}        return (java.util.concurrent.CompletableFuture<{ret_boxed}>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.callAsync({fqn:?}, {call_names}, {call_args}, {descriptor}{call_suffix});"
        )
    } else {
        format!(
            "{prologue}        return (java.util.concurrent.CompletableFuture<{ret_boxed}>) (java.util.concurrent.CompletableFuture<?>) {async_call};"
        )
    };

    format!(
        "\n{doc}    public {static_kw}{generics_kw}{ret_top} {ident}({sig_params}) {{\n{sync_body}\n    }}\n\n{doc}    @SuppressWarnings(\"unchecked\")\n    public {static_kw}{generics_kw}{async_ret} {async_ident}({sig_params}) {{\n{async_body}\n    }}\n"
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
    async_ident: &str,
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
    pool: &mut DescriptorPool,
    returned_callable: Option<&ReturnedCallable>,
    projection: CallProjection,
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
        async_ident,
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
        returned_callable,
        projection,
    );

    // Fluent boxed setters. The wire key is the BAML arg name; the setter
    // method name is the Java-escaped identifier (they differ only when the
    // arg name is a Java keyword).
    let mut setters = String::new();
    for a in optionals {
        let mut boxed = translate_ty(&a.ty, TyPosition::Boxed, ctx, sink);
        // A nullable optional (`y?: T?`) accepts `null` as its VALUE (distinct
        // from omitting it → BAML default), so its setter param is `@Nullable`.
        if crate::translate_ty::is_nullable(&a.ty, ctx.aliases) {
            boxed = crate::translate_ty::annotate_nullable(&boxed);
        }
        let wire = a.name.as_str();
        let setter = java_identifier(wire);
        let mut type_vars = Vec::new();
        crate::translate_ty::collect_type_vars(&a.ty, &mut type_vars);
        let stored = if !type_vars.is_empty() {
            "v".to_string()
        } else if let Some(expr) = typed_callable_expr(&a.ty, "v", pool, ctx) {
            // An optional callable argument carries its declared param/return
            // descriptors, same as the required-arg path.
            expr
        } else if needs_inbound_descriptor(&a.ty, ctx) {
            match crate::translate_ty::descriptor_expr_opt(&a.ty, ctx.aliases) {
                Some(expr) => format!("new baml_bridge.BamlTypedValue(v, {})", pool.intern(expr)),
                None => "v".to_string(),
            }
        } else {
            "v".to_string()
        };
        setters.push_str(&format!(
            "\n        public {opts_ty} {setter}({boxed} v) {{\n            this.$values.put({wire:?}, {stored});\n            this.$touched.add({wire:?});\n            return this;\n        }}\n"
        ));
    }

    out.push_str(&format!(
        "\n    /**\n     * Configurator for the optional arguments of {{@code {ident}}}. Each\n     * fluent setter records one optional; only touched optionals reach the\n     * engine (untouched ⇒ BAML default, touched-with-{{@code null}} ⇒\n     * explicit BAML {{@code null}}).\n     */\n    public static final class {ident}$Opts{gu} {{\n        private final java.util.LinkedHashMap<java.lang.String, java.lang.Object> $values = new java.util.LinkedHashMap<>();\n        private final java.util.LinkedHashSet<java.lang.String> $touched = new java.util.LinkedHashSet<>();\n{setters}\n        java.lang.String[] $names(java.lang.String[] base) {{\n            return this.$namesExcept(base);\n        }}\n\n        java.lang.Object[] $args(java.lang.Object[] base) {{\n            return this.$argsExcept(base);\n        }}\n\n        java.lang.String[] $namesExcept(java.lang.String[] base, java.lang.String... excluded) {{\n            java.util.LinkedHashSet<java.lang.String> excluded$ = new java.util.LinkedHashSet<>(java.util.Arrays.asList(excluded));\n            java.util.ArrayList<java.lang.String> out$ = new java.util.ArrayList<>(java.util.Arrays.asList(base));\n            for (java.lang.String n$ : this.$touched) {{\n                if (!excluded$.contains(n$)) {{\n                    out$.add(n$);\n                }}\n            }}\n            return out$.toArray(new java.lang.String[0]);\n        }}\n\n        java.lang.Object[] $argsExcept(java.lang.Object[] base, java.lang.String... excluded) {{\n            java.util.LinkedHashSet<java.lang.String> excluded$ = new java.util.LinkedHashSet<>(java.util.Arrays.asList(excluded));\n            java.util.ArrayList<java.lang.Object> out$ = new java.util.ArrayList<>(java.util.Arrays.asList(base));\n            for (java.lang.String n$ : this.$touched) {{\n                if (!excluded$.contains(n$)) {{\n                    out$.add(this.$values.get(n$));\n                }}\n            }}\n            return out$.toArray(new java.lang.Object[0]);\n        }}\n\n        java.lang.Object $value(java.lang.String name) {{\n            return this.$values.get(name);\n        }}\n    }}\n"
    ));
    out
}

/// When `ty` (through non-recursive aliases) is a callable type, render the
/// `baml_bridge.BamlTypedCallable` carrier wrapping `value` with the callable's
/// declared parameter / return decode descriptors, so the bridge honors the
/// generated signature on the dispatch path: each argument BAML passes back
/// decodes against its declared parameter descriptor (a `baml.json.json`
/// parameter materializes as the generated sealed union, not the raw wire
/// value) and the returned value encodes against the declared return
/// descriptor. Returns `None` when `ty` is not a callable or when every slot is
/// wire-driven (the raw callable is then registered exactly as before).
fn typed_callable_expr(
    ty: &Ty,
    value: &str,
    pool: &mut DescriptorPool,
    ctx: &TranslateCtx<'_>,
) -> Option<String> {
    let mut resolved = ty;
    loop {
        match resolved {
            Ty::TypeAlias(name, _) => match ctx.aliases.get(name) {
                Some((inner, false)) => resolved = inner,
                _ => return None,
            },
            Ty::Union(items, _) => {
                let mut non_null = items.iter().filter(|item| !matches!(item, Ty::Null { .. }));
                let Some(inner) = non_null.next() else {
                    return None;
                };
                if non_null.next().is_some() {
                    return None;
                }
                resolved = inner;
            }
            Ty::Function { .. } => break,
            _ => return None,
        }
    }
    let Ty::Function { params, ret, .. } = resolved else {
        return None;
    };
    let mut any = false;
    let desc_of = |ty: &Ty, pool: &mut DescriptorPool, any: &mut bool| {
        match crate::translate_ty::descriptor_expr_opt(ty, ctx.aliases) {
            Some(expr) => {
                *any = true;
                pool.intern(expr)
            }
            None => "null".to_string(),
        }
    };
    let mut positional: Vec<String> = Vec::new();
    let mut optional_names: Vec<String> = Vec::new();
    let mut optional_descs: Vec<String> = Vec::new();
    for (i, p) in params.iter().enumerate() {
        let desc = desc_of(&p.ty, pool, &mut any);
        match p.mode {
            CodegenFunctionParamMode::Optional => {
                // The wire key mirrors `translate_callable`'s optional-bag
                // naming (`opt{i}` fallback indexes over ALL params).
                let wire = p
                    .name
                    .as_ref()
                    .map_or_else(|| format!("opt{i}"), |n| n.as_str().to_string());
                optional_names.push(format!("{wire:?}"));
                optional_descs.push(desc);
            }
            CodegenFunctionParamMode::Required => positional.push(desc),
        }
    }
    let ret_desc = desc_of(ret, pool, &mut any);
    if !any {
        return None;
    }
    let positional_lit = if positional.iter().all(|d| d == "null") {
        "null".to_string()
    } else {
        format!("new baml_bridge.BamlType[] {{{}}}", positional.join(", "))
    };
    if optional_names.is_empty() {
        Some(format!(
            "new baml_bridge.BamlTypedCallable({value}, {positional_lit}, {ret_desc})"
        ))
    } else {
        Some(format!(
            "new baml_bridge.BamlTypedCallable({value}, {positional_lit}, new java.lang.String[] {{{}}}, new baml_bridge.BamlType[] {{{}}}, {ret_desc})",
            optional_names.join(", "),
            optional_descs.join(", ")
        ))
    }
}

fn needs_inbound_descriptor(ty: &Ty, ctx: &TranslateCtx<'_>) -> bool {
    match ty {
        Ty::List(..) | Ty::Map { .. } | Ty::Union(..) | Ty::Literal(..) => true,
        Ty::Class(_, args, _) => !args.is_empty(),
        Ty::TypeAlias(name, _) => ctx.aliases.get(name).is_none_or(|(resolved, recursive)| {
            *recursive || needs_inbound_descriptor(resolved, ctx)
        }),
        _ => false,
    }
}

/// Render a generated host-callable `@FunctionalInterface` (design point E):
/// a single abstract SAM `apply(<required…>[, Opts $opts])`, a `default`
/// `__bamlDispatch` override that reshapes the bridge's flat declared-order
/// arg list into that SAM call (required args positionally, supplied optionals
/// folded into the bag), and — when the callable has optionals — a nested
/// always-non-null `Opts` bag whose nullable accessors read each optional
/// (`null` for the ones BAML omitted). Extends
/// `baml_bridge.BamlHostCallable` so the wire encoder detects it by
/// `instanceof`. `apply` stays the sole abstract method, so a lambda can
/// implement it and the type is a valid `@FunctionalInterface`.
pub(crate) fn render_callback_interface(iface: &CallbackInterface) -> String {
    let has_opts = !iface.optionals.is_empty();

    // SAM signature params: required params, then the Opts bag (if any).
    let mut apply_params: Vec<String> = iface
        .required
        .iter()
        .map(|(id, ty)| format!("{ty} {id}"))
        .collect();
    if has_opts {
        apply_params.push("Opts $opts".to_string());
    }

    // __bamlDispatch → apply(...) reshape: cast each positional slot; the trailing
    // Opts bag pulls each optional by its BAML wire name (null when absent).
    let mut apply_args: Vec<String> = iface
        .required
        .iter()
        .enumerate()
        .map(|(i, (_, ty))| format!("({ty}) $positional.get({i})"))
        .collect();
    if has_opts {
        let opts_args: Vec<String> = iface
            .optionals
            .iter()
            .map(|(wire, ty)| format!("({ty}) $optional.get({wire:?})"))
            .collect();
        apply_args.push(format!("new Opts({})", opts_args.join(", ")));
    }

    let mut out = String::from("@FunctionalInterface\n");
    out.push_str(&format!(
        "public interface {} extends baml_bridge.BamlHostCallable {{\n",
        iface.name
    ));
    out.push_str(&format!(
        "    {} apply({});\n\n",
        iface.ret,
        apply_params.join(", ")
    ));
    out.push_str(
        "    /**\n     * Bridge dispatch: reshape the engine's flat declared-order arg list\n     * into this callable's SAM. Required args arrive positionally; supplied\n     * optionals fold into the always-non-null {@code Opts} bag.\n     */\n    @Override\n    default java.lang.Object __bamlDispatch(java.util.List<java.lang.Object> $positional, java.util.Map<java.lang.String, java.lang.Object> $optional) {\n",
    );
    out.push_str(&format!(
        "        return apply({});\n    }}\n",
        apply_args.join(", ")
    ));

    if has_opts {
        out.push_str(&render_opts_bag(&iface.optionals));
    }
    out.push_str("}\n");
    out
}

/// The nested `Opts` bag for a callback interface with optional params: a
/// `final` value holder (implicitly `public static` inside an interface) with
/// one nullable boxed field per optional, a package-visible constructor the
/// bridge's `__bamlDispatch` calls, and a `PreserveCase` public accessor per
/// field (`null` when BAML omitted that optional). Field/accessor idents are
/// the Java-escaped BAML wire names.
fn render_opts_bag(optionals: &[(String, String)]) -> String {
    let fields: Vec<(String, String)> = optionals
        .iter()
        .map(|(wire, ty)| (java_identifier(wire), ty.clone()))
        .collect();

    let mut out = String::from(
        "\n    /**\n     * Optional-argument bag. Each accessor is {@code null} when BAML omitted\n     * the optional (the callable then applies its own fallback). Always\n     * constructed non-null \u{2014} a Java SAM is fixed-arity.\n     */\n    final class Opts {\n",
    );
    for (id, ty) in &fields {
        out.push_str(&format!("        private final {ty} {id};\n"));
    }
    let ctor_params: Vec<String> = fields.iter().map(|(id, ty)| format!("{ty} {id}")).collect();
    out.push_str(&format!("\n        Opts({}) {{\n", ctor_params.join(", ")));
    for (id, _) in &fields {
        out.push_str(&format!("            this.{id} = {id};\n"));
    }
    out.push_str("        }\n");
    // Every accessor is `@Nullable` by design — it returns `null` for an
    // optional BAML omitted (this bag is always constructed with a slot per
    // optional, `null`-filled where absent).
    for (id, ty) in &fields {
        let ret = crate::translate_ty::annotate_nullable(ty);
        out.push_str(&format!(
            "\n        public {ret} {id}() {{\n            return this.{id};\n        }}\n"
        ));
    }
    out.push_str("    }\n");
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
    let arm_tokens = crate::translate_ty::union_arm_tokens(arms);
    let arm_infos: Vec<(String, String)> = arms
        .iter()
        .zip(&arm_tokens)
        .map(|(arm, token)| {
            (
                format!("{token}Value"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javadoc_neutralizes_unicode_escape_star_slash() {
        // A docstring carrying the unicode-escape form of `*/` must not close
        // the Javadoc block: the `\` is broken from the `u` run.
        let doc = "danger \\u002a\\u002f end";
        let rendered = render_javadoc(Some(doc), "");
        // No decodable escape survives: every `\u`+hex run is broken by a space.
        assert!(!rendered.contains("\\u002a"), "{rendered}");
        assert!(!rendered.contains("\\u002f"), "{rendered}");
        assert!(rendered.contains("\\ u002a"), "{rendered}");
        assert!(rendered.contains("\\ u002f"), "{rendered}");
        // The block still opens and closes exactly once.
        assert_eq!(rendered.matches("/**").count(), 1, "{rendered}");
        assert_eq!(rendered.matches("*/").count(), 1, "{rendered}");
    }

    #[test]
    fn javadoc_handles_multi_u_and_odd_backslash_runs() {
        // Multi-`u` marker is still an escape → broken.
        assert_eq!(neutralize_unicode_escapes("\\uu002a"), "\\ uu002a");
        // An inert `\\uXXXX` (even backslash run → not an escape) is untouched.
        assert_eq!(neutralize_unicode_escapes("\\\\u002a"), "\\\\u002a");
        // Three backslashes: the last is eligible → broken.
        assert_eq!(neutralize_unicode_escapes("\\\\\\u002a"), "\\\\\\ u002a");
    }

    #[test]
    fn javadoc_leaves_ordinary_text_untouched() {
        // A lone `\u` without 4 hex digits, and a non-`u` backslash, are inert.
        assert_eq!(neutralize_unicode_escapes("path C:\\temp"), "path C:\\temp");
        assert_eq!(neutralize_unicode_escapes("\\university"), "\\university");
        assert_eq!(
            neutralize_unicode_escapes("no escapes here"),
            "no escapes here"
        );
    }

    fn doc(name: &str, text: Option<&str>) -> (String, Option<String>) {
        (name.to_string(), text.map(std::string::ToString::to_string))
    }

    #[test]
    fn rolled_docstring_none_when_nothing_to_render() {
        // No summary and no member documented → no docstring at all.
        assert_eq!(
            format_rolled_docstring(None, &[doc("a", None), doc("b", None)], "Attributes"),
            None
        );
        assert_eq!(format_rolled_docstring(Some(""), &[], "Attributes"), None);
    }

    #[test]
    fn rolled_docstring_summary_plus_attributes_section() {
        // Mirrors the `Doc` class: summary + a fully-documented Attributes
        // section (Python-parity rollup).
        let out = format_rolled_docstring(
            Some("A document with a title and an optional body."),
            &[
                doc("title", Some("Title shown in lists and search results.")),
                doc("body", Some("Free-form body text.")),
            ],
            "Attributes",
        );
        assert_eq!(
            out.as_deref(),
            Some(
                "A document with a title and an optional body.\n\
                 \n\
                 Attributes:\n\
                 \x20   title: Title shown in lists and search results.\n\
                 \x20   body: Free-form body text."
            )
        );
    }

    #[test]
    fn rolled_docstring_multiline_summary_and_bare_undocumented_member() {
        // Mirrors the `Note` class: multi-line summary, one documented field,
        // one undocumented field listed as a bare name under the "any-doc" rule.
        let out = format_rolled_docstring(
            Some("A multi-line summary.\nContinuation line."),
            &[doc("id", Some("Stable identifier.")), doc("text", None)],
            "Attributes",
        )
        .unwrap();
        assert!(
            out.starts_with("A multi-line summary.\nContinuation line."),
            "{out}"
        );
        assert!(
            out.contains("\n\nAttributes:\n    id: Stable identifier."),
            "{out}"
        );
        // Bare-name entry: just the identifier, no trailing colon, no doc.
        assert!(out.ends_with("\n    text"), "{out}");
    }

    #[test]
    fn rolled_docstring_enum_members_section() {
        // Mirrors the `Sentiment` enum: summary + Members section, one bare.
        let out = format_rolled_docstring(
            Some("Sentiment labels surfaced by the model."),
            &[
                doc("HAPPY", Some("Smiling face.")),
                doc("SAD", Some("Frowning face.")),
                doc("NEUTRAL", None),
            ],
            "Members",
        );
        assert_eq!(
            out.as_deref(),
            Some(
                "Sentiment labels surfaced by the model.\n\
                 \n\
                 Members:\n\
                 \x20   HAPPY: Smiling face.\n\
                 \x20   SAD: Frowning face.\n\
                 \x20   NEUTRAL"
            )
        );
    }

    #[test]
    fn rolled_docstring_summary_only_suppresses_section() {
        // Mirrors the `Priority` enum: a (multi-line) class-level summary with
        // every member bare → the Members section is suppressed entirely.
        let out = format_rolled_docstring(
            Some("Summary only, no member rollup:\nsecond line."),
            &[doc("HIGH", None), doc("MEDIUM", None), doc("LOW", None)],
            "Members",
        );
        assert_eq!(
            out.as_deref(),
            Some("Summary only, no member rollup:\nsecond line.")
        );
        assert!(!out.unwrap().contains("Members:"));
    }

    #[test]
    fn rolled_docstring_member_continuation_lines_indent_under_name() {
        // A multi-line member doc: the first line sits after `name: `, the
        // continuation lines are indented deeper (8 spaces).
        let out = format_rolled_docstring(
            None,
            &[doc("field", Some("first line\nsecond line"))],
            "Attributes",
        )
        .unwrap();
        assert_eq!(
            out,
            "Attributes:\n    field: first line\n        second line"
        );
    }

    #[test]
    fn javadoc_blank_body_line_has_no_trailing_space() {
        // The rolled summary/section separator is a blank body line; it renders
        // as a bare ` *` (no trailing whitespace).
        let rendered = render_javadoc(Some("summary\n\nAttributes:\n    a: x"), "");
        assert!(rendered.contains("\n *\n"), "{rendered}");
        assert!(!rendered.contains(" * \n"), "{rendered}");
    }
}
