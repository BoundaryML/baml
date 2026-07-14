//! C++ SDK emitter. Slices 1-3 of the bridge-cpp codegen spec: the
//! single-header layout, namespace routing, free functions, classes + enums
//! with generated `codec<T>` specializations, static + instance methods, and
//! optional arguments via per-function opts structs (spec D4). Aliases,
//! generics, streaming, companions, and recursive classes land in later
//! slices; symbols they gate on are skipped and reported in a trailing
//! header comment (no silent caps).
//!
//! Output layout (spec D1):
//!   `include/baml_sdk.hpp`   - the typed surface
//!   `src/bindings.cpp`       - function definitions over `::baml::detail`
//!   `src/_inlinedbaml.cpp`   - embedded BAML sources + lazy runtime init
//!
//! Runtime init embeds the user's `.baml` sources and initializes through
//! `create_baml_runtime`; it switches to embedded bytecode once
//! `initialize_runtime_from_bytecode` is exported over the C ABI.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{Class, Enum, Function, Name, Symbol, SymbolPool, Ty};
pub use baml_codegen_types::{NamingConvention, OutputType};

/// A user BAML source file as it should appear in the emitter's
/// inlined-baml output. `rel_path` is relative to the `baml_src/` root.
pub type UserBamlFile = (PathBuf, String);

/// Build the C++ SDK output tree for `pool`. Returned paths are relative to
/// the `baml_sdk/` output root.
pub fn to_source_code(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    let mut skipped: Vec<String> = Vec::new();

    let mut names: Vec<_> = pool.keys().collect();
    names.sort();

    // Pass 1: enums (no dependencies).
    let mut enums: Vec<EmittedEnum> = Vec::new();
    let mut emitted_types: BTreeSet<Name> = BTreeSet::new();
    for name in &names {
        if name.pkg.as_str() != "user" || name.is_stream() {
            continue;
        }
        if let Symbol::Enum(enum_def) = &pool[*name] {
            enums.push(emit_enum(name, enum_def));
            emitted_types.insert((*name).clone());
        }
    }

    // Pass 2: class fields, to a fixed point so field dependencies resolve
    // in emission (= declaration) order. Classes left over depend on
    // something this slice cannot emit (recursion, generics, unsupported
    // field types).
    let mut classes: Vec<EmittedClass> = Vec::new();
    let mut pending: Vec<&Name> = names
        .iter()
        .copied()
        .filter(|name| {
            name.pkg.as_str() == "user"
                && !name.is_stream()
                && matches!(&pool[*name], Symbol::Class(_))
        })
        .collect();
    loop {
        let mut progressed = false;
        let mut still_pending = Vec::new();
        for name in pending {
            let Symbol::Class(class_def) = &pool[name] else {
                unreachable!()
            };
            match emit_class(name, class_def, &emitted_types) {
                Ok(Some(emitted)) => {
                    classes.push(emitted);
                    emitted_types.insert(name.clone());
                    progressed = true;
                }
                Ok(None) => still_pending.push(name),
                Err(reason) => {
                    skipped.push(format!("{name}: {reason}"));
                    progressed = true;
                }
            }
        }
        pending = still_pending;
        if pending.is_empty() || !progressed {
            break;
        }
    }
    for name in pending {
        skipped.push(format!(
            "{name}: field dependency cycle or dependency on a skipped type \
             (recursive classes land with baml::Box in a later slice)"
        ));
    }

    // Pass 3: methods, against the final emitted type set (declarations may
    // reference any emitted class thanks to the forward-declaration block).
    for class in &mut classes {
        let Symbol::Class(class_def) = &pool[&class.pool_name] else {
            unreachable!()
        };
        for method in &class_def.static_methods {
            match emit_callable(
                &method_fqn(&class.pool_name, method),
                method,
                &emitted_types,
            ) {
                Ok(emitted) => class.static_methods.push(emitted),
                Err(reason) => {
                    skipped.push(format!("{}.{}: {reason}", class.pool_name, method.name));
                }
            }
        }
        for method in &class_def.instance_methods {
            match emit_callable(
                &method_fqn(&class.pool_name, method),
                method,
                &emitted_types,
            ) {
                Ok(emitted) => class.instance_methods.push(emitted),
                Err(reason) => {
                    skipped.push(format!("{}.{}: {reason}", class.pool_name, method.name));
                }
            }
        }
    }

    // Pass 4: free functions over the emitted type set.
    let mut fns_by_namespace: BTreeMap<Vec<String>, Vec<EmittedFn>> = BTreeMap::new();
    for name in &names {
        let Symbol::Function(function) = &pool[*name] else {
            continue;
        };
        if name.pkg.as_str() != "user" {
            continue; // stdlib/vendor surfaces come with later slices
        }
        if name.is_stream() || name.bare_name().contains('$') {
            skipped.push(format!("{name}: companion functions land in a later slice"));
            continue;
        }
        match emit_callable(&name.to_string(), function, &emitted_types) {
            Ok(emitted) => {
                let ns = cpp_namespace_of(name);
                fns_by_namespace.entry(ns).or_default().push(emitted);
            }
            Err(reason) => skipped.push(format!("{name}: {reason}")),
        }
    }
    for name in &names {
        if let Symbol::TypeAlias(_) = &pool[*name] {
            if name.pkg.as_str() == "user" {
                skipped.push(format!("{name}: type aliases land in a later slice"));
            }
        }
    }

    let mut out = HashMap::new();
    out.insert(
        PathBuf::from("include/baml_sdk.hpp"),
        render_header(&enums, &classes, &fns_by_namespace, &skipped),
    );
    out.insert(
        PathBuf::from("src/bindings.cpp"),
        render_bindings(&classes, &fns_by_namespace),
    );
    out.insert(
        PathBuf::from("src/_inlinedbaml.cpp"),
        render_inlinedbaml(user_baml_files),
    );
    out
}

fn method_fqn(class_name: &Name, method: &Function) -> String {
    format!("{class_name}.{}", method.name)
}

fn cpp_namespace_of(name: &Name) -> Vec<String> {
    name.namespace_path
        .iter()
        .map(|seg| sanitize(seg.as_str()))
        .collect()
}

fn qualified_cpp_name(name: &Name) -> String {
    let mut out = String::from("::baml_sdk");
    for seg in &name.namespace_path {
        out.push_str("::");
        out.push_str(&sanitize(seg.as_str()));
    }
    out.push_str("::");
    out.push_str(&sanitize(name.bare_name()));
    out
}

/// `optional_args_probe` -> `OptionalArgsProbe`. Used only for synthesized
/// symbols (opts structs), which have no BAML source name to preserve.
fn pascal_case(name: &str) -> String {
    let mut out = String::new();
    for part in name.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

struct EmittedEnum {
    ns: Vec<String>,
    cpp_name: String,
    qualified: String,
    fqn: String,
    doc: Option<String>,
    /// (C++ enumerator, BAML variant value on the wire)
    variants: Vec<(String, String)>,
}

fn emit_enum(name: &Name, enum_def: &Enum) -> EmittedEnum {
    EmittedEnum {
        ns: cpp_namespace_of(name),
        cpp_name: sanitize(name.bare_name()),
        qualified: qualified_cpp_name(name),
        fqn: name.to_string(),
        doc: enum_def.docstring.clone(),
        variants: enum_def
            .variants
            .iter()
            .map(|v| (sanitize(v.name.as_str()), v.value.clone()))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

struct EmittedClass {
    pool_name: Name,
    ns: Vec<String>,
    cpp_name: String,
    qualified: String,
    fqn: String,
    doc: Option<String>,
    /// (C++ field name, C++ type, wire field name)
    fields: Vec<(String, String, String)>,
    static_methods: Vec<EmittedFn>,
    instance_methods: Vec<EmittedFn>,
}

/// Ok(None) = not emittable *yet* (a field references a class not emitted so
/// far); the fixed-point loop retries. Err = never emittable in this slice.
fn emit_class(
    name: &Name,
    class_def: &Class,
    emitted_types: &BTreeSet<Name>,
) -> Result<Option<EmittedClass>, String> {
    if !class_def.generic_params.is_empty() {
        return Err("generic classes land in a later slice".to_string());
    }
    let mut fields = Vec::new();
    for prop in &class_def.properties {
        match translate_ty(&prop.ty, emitted_types) {
            Translated::Cpp(ty) => {
                fields.push((sanitize(prop.name.as_str()), ty, prop.name.to_string()));
            }
            Translated::NotYet => return Ok(None),
            Translated::Unsupported(reason) => {
                return Err(format!("field `{}`: {reason}", prop.name));
            }
        }
    }
    Ok(Some(EmittedClass {
        pool_name: name.clone(),
        ns: cpp_namespace_of(name),
        cpp_name: sanitize(name.bare_name()),
        qualified: qualified_cpp_name(name),
        fqn: name.to_string(),
        doc: class_def.docstring.clone(),
        fields,
        static_methods: Vec::new(),
        instance_methods: Vec::new(),
    }))
}

// ---------------------------------------------------------------------------
// Callables (free functions and methods share this shape)
// ---------------------------------------------------------------------------

struct EmittedFn {
    cpp_name: String,
    fqn: String,
    ret: String,
    /// Required parameters: (C++ name, C++ type, wire name).
    params: Vec<(String, String, String)>,
    /// Optional parameters: (C++ name, normalized C++ type, wire name).
    /// Rendered as Arg<type> fields on the opts struct.
    opt_params: Vec<(String, String, String)>,
    /// Opts struct bare name, when `opt_params` is non-empty.
    opts_name: Option<String>,
    doc: Option<String>,
    raises: Vec<String>,
}

fn emit_callable(
    fqn: &str,
    function: &Function,
    emitted_types: &BTreeSet<Name>,
) -> Result<EmittedFn, String> {
    if !function.generic_params.is_empty() {
        return Err("generic functions land in a later slice".to_string());
    }
    let mut params = Vec::new();
    let mut opt_params = Vec::new();
    for arg in &function.arguments {
        let ty = match translate_ty(&arg.ty, emitted_types) {
            Translated::Cpp(ty) => ty,
            Translated::NotYet | Translated::Unsupported(_) => {
                return Err(format!(
                    "argument `{}` has unsupported type {}",
                    arg.name, arg.ty
                ));
            }
        };
        let entry = (sanitize(arg.name.as_str()), ty, arg.name.to_string());
        if arg.default.is_some() {
            opt_params.push(entry);
        } else {
            params.push(entry);
        }
    }
    let ret = match translate_return_ty(&function.return_type, emitted_types) {
        Translated::Cpp(ty) => ty,
        Translated::NotYet | Translated::Unsupported(_) => {
            return Err(format!("unsupported return type {}", function.return_type));
        }
    };

    let raises = match &function.throws {
        None => Vec::new(),
        Some(Ty::Union(items)) => items.iter().map(unqualified_leaf_name).collect(),
        Some(ty) => vec![unqualified_leaf_name(ty)],
    };

    let opts_name = if opt_params.is_empty() {
        None
    } else {
        Some(format!("{}Opts", pascal_case(function.name.as_str())))
    };

    Ok(EmittedFn {
        cpp_name: sanitize(function.name.as_str()),
        fqn: fqn.to_string(),
        ret,
        params,
        opt_params,
        opts_name,
        doc: function.docstring.clone(),
        raises,
    })
}

fn unqualified_leaf_name(ty: &Ty) -> String {
    match ty {
        Ty::Class(name, _) | Ty::Enum(name) | Ty::TypeAlias(name) => name.bare_name().to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Type translation
// ---------------------------------------------------------------------------

enum Translated {
    Cpp(String),
    /// References a class that has not been emitted (yet) — retry later.
    NotYet,
    Unsupported(String),
}

/// Slice-1..3 type table: primitives, containers, null-normalized optionals,
/// variants, and emitted classes/enums. Everything else is unsupported here
/// and the surrounding symbol is skipped (reported, not silently dropped).
fn translate_ty(ty: &Ty, emitted_types: &BTreeSet<Name>) -> Translated {
    let translated = match ty {
        Ty::Int => "int64_t".to_string(),
        Ty::Float => "double".to_string(),
        Ty::String => "std::string".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Null => "std::monostate".to_string(),
        Ty::Uint8Array => "std::vector<uint8_t>".to_string(),
        Ty::Literal(lit) => {
            // Literal types widen to their base type (Python parity).
            match lit {
                baml_base::Literal::Int(_) => "int64_t".to_string(),
                baml_base::Literal::Bigint(_) => {
                    return Translated::Unsupported("bigint literal".to_string());
                }
                baml_base::Literal::Float(_) => "double".to_string(),
                baml_base::Literal::String(_) => "std::string".to_string(),
                baml_base::Literal::Bool(_) => "bool".to_string(),
            }
        }
        Ty::Enum(name) => {
            if name.pkg.as_str() != "user" {
                return Translated::Unsupported(format!("non-user enum {name}"));
            }
            return if emitted_types.contains(name) {
                Translated::Cpp(qualified_cpp_name(name))
            } else {
                Translated::NotYet
            };
        }
        Ty::Class(name, args) => {
            if !args.is_empty() {
                return Translated::Unsupported("generic class reference".to_string());
            }
            if name.pkg.as_str() != "user" {
                return Translated::Unsupported(format!("non-user class {name}"));
            }
            return if emitted_types.contains(name) {
                Translated::Cpp(qualified_cpp_name(name))
            } else {
                Translated::NotYet
            };
        }
        Ty::List(inner) => match translate_ty(inner, emitted_types) {
            Translated::Cpp(inner) => format!("std::vector<{inner}>"),
            other => return other,
        },
        Ty::Map { key, value } => {
            if !matches!(key.as_ref(), Ty::String) {
                return Translated::Unsupported("non-string map key".to_string());
            }
            match translate_ty(value, emitted_types) {
                Translated::Cpp(value) => format!("std::map<std::string, {value}>"),
                other => return other,
            }
        }
        Ty::Union(items) => {
            // Null-normalization (spec D-unions v2): strip the null member,
            // dedup alternatives that map to the same C++ type, emit a
            // variant (or the bare type when one alternative remains), and
            // wrap in optional when null was a member.
            let non_null: Vec<&Ty> = items.iter().filter(|t| !matches!(t, Ty::Null)).collect();
            let had_null = non_null.len() != items.len();
            let mut alternatives: Vec<String> = Vec::new();
            for item in non_null {
                match translate_ty(item, emitted_types) {
                    Translated::Cpp(alt) => {
                        if !alternatives.contains(&alt) {
                            alternatives.push(alt);
                        }
                    }
                    other => return other,
                }
            }
            let inner = match alternatives.as_slice() {
                [] => return Translated::Unsupported("empty union".to_string()),
                [single] => single.clone(),
                _ => format!("std::variant<{}>", alternatives.join(", ")),
            };
            if had_null {
                format!("std::optional<{inner}>")
            } else {
                inner
            }
        }
        other => return Translated::Unsupported(format!("type {other}")),
    };
    Translated::Cpp(translated)
}

fn translate_return_ty(ty: &Ty, emitted_types: &BTreeSet<Name>) -> Translated {
    if matches!(ty, Ty::Unit) {
        return Translated::Cpp("void".to_string());
    }
    translate_ty(ty, emitted_types)
}

fn sanitize(name: &str) -> String {
    const CPP_KEYWORDS: &[&str] = &[
        "alignas",
        "alignof",
        "asm",
        "auto",
        "bool",
        "break",
        "case",
        "catch",
        "char",
        "class",
        "concept",
        "const",
        "constexpr",
        "continue",
        "default",
        "delete",
        "do",
        "double",
        "else",
        "enum",
        "explicit",
        "export",
        "extern",
        "false",
        "float",
        "for",
        "friend",
        "goto",
        "if",
        "inline",
        "int",
        "long",
        "mutable",
        "namespace",
        "new",
        "noexcept",
        "nullptr",
        "operator",
        "private",
        "protected",
        "public",
        "register",
        "requires",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "template",
        "this",
        "throw",
        "true",
        "try",
        "typedef",
        "typeid",
        "typename",
        "union",
        "unsigned",
        "using",
        "virtual",
        "void",
        "volatile",
        "while",
    ];
    if CPP_KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// How a callable renders: as a free function, a method declaration inside a
/// struct, or an out-of-line member definition.
enum RenderPos<'a> {
    Free,
    StaticDecl,
    InstanceDecl,
    StaticDef { class: &'a EmittedClass },
    InstanceDef { class: &'a EmittedClass },
}

fn push_doc(buf: &mut String, indent: &str, doc: Option<&String>, raises: &[String]) {
    if let Some(doc) = doc {
        for line in doc.lines() {
            let _ = writeln!(buf, "{indent}/// {line}");
        }
    }
    if !raises.is_empty() {
        let _ = writeln!(buf, "{indent}/// Raises: {}", raises.join(", "));
    }
}

fn open_namespaces(buf: &mut String, ns: &[String]) {
    for seg in ns {
        let _ = writeln!(buf, "namespace {seg} {{");
    }
}

fn close_namespaces(buf: &mut String, ns: &[String]) {
    for seg in ns.iter().rev() {
        let _ = writeln!(buf, "}}  // namespace {seg}");
    }
}

fn by_value_or_cref(ty: &str) -> String {
    match ty {
        "int64_t" | "double" | "bool" | "std::monostate" => ty.to_string(),
        _ => format!("const {ty}&"),
    }
}

fn signature(f: &EmittedFn, async_variant: bool, pos: &RenderPos) -> String {
    let mut params: Vec<String> = f
        .params
        .iter()
        .map(|(name, ty, _)| format!("{} {}", by_value_or_cref(ty), name))
        .collect();
    if let Some(opts_name) = &f.opts_name {
        let default = match pos {
            RenderPos::StaticDef { .. } | RenderPos::InstanceDef { .. } => "",
            _ => " = {}",
        };
        let qualified_opts = match pos {
            RenderPos::StaticDef { class } | RenderPos::InstanceDef { class } => {
                format!("{}::{opts_name}", class.cpp_name)
            }
            _ => opts_name.clone(),
        };
        params.push(format!("{qualified_opts} opts{default}"));
    }
    let (ret, suffix) = if async_variant {
        (format!("::baml::Future<{}>", f.ret), "_async")
    } else {
        (f.ret.clone(), "")
    };
    let prefix = match pos {
        RenderPos::StaticDecl => "static ",
        _ => "",
    };
    let owner = match pos {
        RenderPos::StaticDef { class } | RenderPos::InstanceDef { class } => {
            format!("{}::", class.cpp_name)
        }
        _ => String::new(),
    };
    let constness = match pos {
        RenderPos::InstanceDecl | RenderPos::InstanceDef { .. } => " const",
        _ => "",
    };
    format!(
        "{prefix}{ret} {owner}{name}{suffix}({params}){constness}",
        name = f.cpp_name,
        params = params.join(", ")
    )
}

fn render_opts_struct(buf: &mut String, indent: &str, f: &EmittedFn) {
    let Some(opts_name) = &f.opts_name else {
        return;
    };
    let _ = writeln!(buf, "{indent}struct {opts_name} {{");
    for (name, ty, _) in &f.opt_params {
        let arg_ty = format!("::baml::Arg<{ty}>");
        let _ = writeln!(buf, "{indent}    {arg_ty} {name};");
        let _ = writeln!(
            buf,
            "{indent}    {opts_name}& set_{name}({arg_ty} v) {{\n\
             {indent}        {name} = std::move(v);\n\
             {indent}        return *this;\n\
             {indent}    }}"
        );
    }
    let _ = writeln!(buf, "{indent}}};");
}

fn render_header(
    enums: &[EmittedEnum],
    classes: &[EmittedClass],
    fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>,
    skipped: &[String],
) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #ifndef BAML_SDK_HPP\n\
         #define BAML_SDK_HPP\n\n\
         #include <cstdint>\n\
         #include <map>\n\
         #include <optional>\n\
         #include <string>\n\
         #include <utility>\n\
         #include <variant>\n\
         #include <vector>\n\n\
         #include <baml/baml.hpp>\n\n\
         namespace baml_sdk {\n\n\
         namespace detail {\n\
         // Lazily initializes the process-global runtime from the embedded\n\
         // BAML sources (see src/_inlinedbaml.cpp). Every binding calls this.\n\
         void ensure_runtime();\n\
         }  // namespace detail\n",
    );

    // Forward declarations: method signatures may reference classes defined
    // later (C++ allows incomplete types in declarations).
    if !classes.is_empty() {
        buf.push('\n');
        for c in classes {
            open_namespaces(&mut buf, &c.ns);
            let _ = writeln!(buf, "struct {};", c.cpp_name);
            close_namespaces(&mut buf, &c.ns);
        }
    }

    for e in enums {
        buf.push('\n');
        open_namespaces(&mut buf, &e.ns);
        push_doc(&mut buf, "", e.doc.as_ref(), &[]);
        let _ = writeln!(buf, "enum class {} {{", e.cpp_name);
        for (variant, _) in &e.variants {
            let _ = writeln!(buf, "    {variant},");
        }
        buf.push_str("};\n");
        close_namespaces(&mut buf, &e.ns);
    }

    // Classes are already in dependency order from the fixed-point loop.
    for c in classes {
        buf.push('\n');
        open_namespaces(&mut buf, &c.ns);
        push_doc(&mut buf, "", c.doc.as_ref(), &[]);
        let _ = writeln!(buf, "struct {} {{", c.cpp_name);
        for (name, ty, _) in &c.fields {
            let _ = writeln!(buf, "    {ty} {name};");
        }
        for f in c.static_methods.iter().chain(&c.instance_methods) {
            render_opts_struct(&mut buf, "    ", f);
        }
        for f in &c.static_methods {
            push_doc(&mut buf, "    ", f.doc.as_ref(), &f.raises);
            let _ = writeln!(buf, "    {};", signature(f, false, &RenderPos::StaticDecl));
            let _ = writeln!(buf, "    {};", signature(f, true, &RenderPos::StaticDecl));
        }
        for f in &c.instance_methods {
            push_doc(&mut buf, "    ", f.doc.as_ref(), &f.raises);
            let _ = writeln!(
                buf,
                "    {};",
                signature(f, false, &RenderPos::InstanceDecl)
            );
            let _ = writeln!(buf, "    {};", signature(f, true, &RenderPos::InstanceDecl));
        }
        let eq_terms: Vec<String> = c
            .fields
            .iter()
            .map(|(name, _, _)| format!("a.{name} == b.{name}"))
            .collect();
        let eq_expr = if eq_terms.is_empty() {
            "true".to_string()
        } else {
            eq_terms.join(" && ")
        };
        let _ = writeln!(
            buf,
            "    friend bool operator==(const {n}& a, const {n}& b) {{\n        \
             return {eq_expr};\n    }}\n    \
             friend bool operator!=(const {n}& a, const {n}& b) {{ return !(a == b); }}",
            n = c.cpp_name
        );
        buf.push_str("};\n");
        close_namespaces(&mut buf, &c.ns);
    }

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        open_namespaces(&mut buf, ns);
        for f in fns {
            render_opts_struct(&mut buf, "", f);
            push_doc(&mut buf, "", f.doc.as_ref(), &f.raises);
            let _ = writeln!(buf, "{};", signature(f, false, &RenderPos::Free));
            let _ = writeln!(buf, "{};", signature(f, true, &RenderPos::Free));
        }
        close_namespaces(&mut buf, ns);
    }

    buf.push_str("\n}  // namespace baml_sdk\n");

    render_codecs(&mut buf, enums, classes);

    if !skipped.is_empty() {
        buf.push_str("\n// Symbols not yet emitted by this sdkgen_cpp slice:\n");
        for line in skipped {
            let _ = writeln!(buf, "//   {line}");
        }
    }
    buf.push_str("\n#endif  // BAML_SDK_HPP\n");
    buf
}

/// codec<T> specializations for the generated enums and classes. Emitted in
/// the header (inline) so future generic bindings can instantiate them from
/// any translation unit.
fn render_codecs(buf: &mut String, enums: &[EmittedEnum], classes: &[EmittedClass]) {
    buf.push_str("\nnamespace baml {\n");

    for e in enums {
        let _ = writeln!(
            buf,
            "\ntemplate <>\nstruct codec<{q}> {{\n    \
             static void encode(detail::wire::Writer& value_msg, {q} v) {{\n        \
             detail::wire::Writer e;\n        \
             e.string_field(1, \"{fqn}\");\n        \
             e.string_field(2, to_wire(v));\n        \
             value_msg.message_field(9, e);\n    }}\n    \
             static {q} decode(const detail::OutboundValue& v) {{\n        \
             if (v.kind != detail::OutboundValue::Kind::Enum) {{\n            \
             detail::kind_mismatch(\"enum {fqn}\", v);\n        }}\n        \
             return from_wire(v.string_v);\n    }}",
            q = e.qualified,
            fqn = e.fqn,
        );
        buf.push_str("    static const char* to_wire(");
        let _ = write!(buf, "{} v) {{\n        switch (v) {{\n", e.qualified);
        for (variant, value) in &e.variants {
            let _ = writeln!(
                buf,
                "            case {q}::{variant}: return \"{value}\";",
                q = e.qualified
            );
        }
        buf.push_str("        }\n        throw BamlError(\"invalid enum value\");\n    }\n");
        let _ = writeln!(
            buf,
            "    static {q} from_wire(const std::string& value) {{",
            q = e.qualified
        );
        for (variant, value) in &e.variants {
            let _ = writeln!(
                buf,
                "        if (value == \"{value}\") return {q}::{variant};",
                q = e.qualified
            );
        }
        let _ = writeln!(
            buf,
            "        throw BamlError(\"unknown variant '\" + value + \"' for enum {fqn}\");\n    \
             }}\n}};",
            fqn = e.fqn
        );
    }

    for c in classes {
        let _ = writeln!(buf, "\ntemplate <>\nstruct codec<{q}> {{", q = c.qualified);
        // encode: InboundValue.class_value = 8 { fields = 2, class_ty = 3 }
        let _ = writeln!(
            buf,
            "    static void encode(detail::wire::Writer& value_msg, const {q}& v) {{\n        \
             detail::wire::Writer cls;",
            q = c.qualified
        );
        for (name, ty, wire_name) in &c.fields {
            let _ = writeln!(
                buf,
                "        {{\n            detail::wire::Writer entry;\n            \
                 entry.string_field(1, \"{wire_name}\");\n            \
                 detail::wire::Writer val;\n            \
                 codec<{ty}>::encode(val, v.{name});\n            \
                 entry.message_field(6, val);\n            \
                 cls.message_field(2, entry);\n        }}"
            );
        }
        let _ = writeln!(
            buf,
            "        detail::wire::Writer class_ty;\n        \
             class_ty.string_field(1, \"{fqn}\");\n        \
             cls.message_field(3, class_ty);\n        \
             value_msg.message_field(8, cls);\n    }}",
            fqn = c.fqn
        );
        // decode: strict field mapping (extra field or missing field = error,
        // pydantic extra="forbid" parity), FQN-checked for precise
        // variant-of-class dispatch.
        let _ = writeln!(
            buf,
            "    static {q} decode(const detail::OutboundValue& v) {{\n        \
             if (v.kind != detail::OutboundValue::Kind::Class ||\n            \
             (!v.name.empty() && v.name != \"{fqn}\")) {{\n            \
             detail::kind_mismatch(\"class {fqn}\", v);\n        }}\n        \
             {q} out{{}};",
            q = c.qualified,
            fqn = c.fqn
        );
        for (name, _, _) in &c.fields {
            let _ = writeln!(buf, "        bool saw_{name} = false;");
        }
        buf.push_str("        for (const auto& field : v.fields) {\n");
        let mut first = true;
        for (name, ty, wire_name) in &c.fields {
            let kw = if first { "if" } else { "} else if" };
            first = false;
            let _ = writeln!(
                buf,
                "            {kw} (field.first == \"{wire_name}\") {{\n                \
                 out.{name} = codec<{ty}>::decode(field.second);\n                \
                 saw_{name} = true;"
            );
        }
        if !c.fields.is_empty() {
            buf.push_str("            } else {\n");
        } else {
            buf.push_str("            {\n");
        }
        let _ = writeln!(
            buf,
            "                throw BamlError(\"unexpected field '\" + field.first + \"' on {fqn}\");\n            \
             }}\n        }}",
            fqn = c.fqn
        );
        for (name, _, wire_name) in &c.fields {
            let _ = writeln!(
                buf,
                "        if (!saw_{name}) {{\n            \
                 throw BamlError(\"missing field '{wire_name}' on {fqn}\");\n        }}",
                fqn = c.fqn
            );
        }
        buf.push_str("        return out;\n    }\n};\n");
    }

    buf.push_str("\n}  // namespace baml\n");
}

/// Emits one binding body: runtime init, self (for instance methods),
/// required args, set optional args, then the call.
fn render_body(
    buf: &mut String,
    f: &EmittedFn,
    async_variant: bool,
    self_class: Option<&EmittedClass>,
) {
    buf.push_str("    ::baml_sdk::detail::ensure_runtime();\n");
    buf.push_str("    ::baml::detail::ArgsEncoder args;\n");
    if let Some(class) = self_class {
        let _ = writeln!(
            buf,
            "    args.add_arg(\"self\", [&](::baml::detail::wire::Writer& w) {{ \
             ::baml::codec<{q}>::encode(w, *this); }});",
            q = class.qualified
        );
    }
    for (cpp_name, ty, wire_name) in &f.params {
        let _ = writeln!(
            buf,
            "    args.add_arg(\"{wire_name}\", [&](::baml::detail::wire::Writer& w) {{ \
             ::baml::codec<{ty}>::encode(w, {cpp_name}); }});"
        );
    }
    for (cpp_name, ty, wire_name) in &f.opt_params {
        let _ = writeln!(
            buf,
            "    if (opts.{cpp_name}.is_set()) {{\n        \
             args.add_arg(\"{wire_name}\", [&](::baml::detail::wire::Writer& w) {{ \
             ::baml::codec<{ty}>::encode(w, opts.{cpp_name}.value()); }});\n    }}"
        );
    }
    let call = if async_variant {
        "start_call"
    } else {
        "call_sync"
    };
    let _ = writeln!(
        buf,
        "    return ::baml::detail::{call}<{ret}>(\"{fqn}\", std::move(args));",
        ret = f.ret,
        fqn = f.fqn,
    );
}

fn render_bindings(
    classes: &[EmittedClass],
    fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>,
) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #include <baml_sdk.hpp>\n\n\
         #include <utility>\n\n\
         namespace baml_sdk {\n",
    );

    for c in classes {
        if c.static_methods.is_empty() && c.instance_methods.is_empty() {
            continue;
        }
        buf.push('\n');
        open_namespaces(&mut buf, &c.ns);
        for f in &c.static_methods {
            for async_variant in [false, true] {
                let pos = RenderPos::StaticDef { class: c };
                let _ = writeln!(buf, "\n{} {{", signature(f, async_variant, &pos));
                render_body(&mut buf, f, async_variant, None);
                buf.push_str("}\n");
            }
        }
        for f in &c.instance_methods {
            for async_variant in [false, true] {
                let pos = RenderPos::InstanceDef { class: c };
                let _ = writeln!(buf, "\n{} {{", signature(f, async_variant, &pos));
                render_body(&mut buf, f, async_variant, Some(c));
                buf.push_str("}\n");
            }
        }
        close_namespaces(&mut buf, &c.ns);
    }

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        open_namespaces(&mut buf, ns);
        for f in fns {
            for async_variant in [false, true] {
                let sig = signature(f, async_variant, &RenderPos::Free);
                // Free-function definitions must not repeat the default arg.
                let sig = sig.replace(" opts = {}", " opts");
                let _ = writeln!(buf, "\n{sig} {{");
                render_body(&mut buf, f, async_variant, None);
                buf.push_str("}\n");
            }
        }
        close_namespaces(&mut buf, ns);
    }

    buf.push_str("\n}  // namespace baml_sdk\n");
    buf
}

fn render_inlinedbaml(user_baml_files: &[UserBamlFile]) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit. Embedded BAML sources and\n\
         // lazy runtime initialization.\n\
         #include <map>\n\
         #include <mutex>\n\
         #include <string>\n\n\
         #include <baml/baml.hpp>\n\n\
         namespace baml_sdk {\n\
         namespace detail {\n\n\
         void ensure_runtime() {\n\
             static std::once_flag once;\n\
             std::call_once(once, [] {\n\
                 const std::map<std::string, std::string> files = {\n",
    );
    for (rel_path, content) in user_baml_files {
        let path = rel_path.to_string_lossy().replace('\\', "/");
        let _ = writeln!(
            buf,
            "            {{\"{path}\", std::string(R\"BAMLSRC({content})BAMLSRC\")}},"
        );
    }
    buf.push_str(
        "        };\n\
                 ::baml::initialize_runtime(\".\", files);\n\
             });\n\
         }\n\n\
         }  // namespace detail\n\
         }  // namespace baml_sdk\n",
    );
    buf
}
