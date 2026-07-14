//! C++ SDK emitter. Slice 1 of the bridge-cpp codegen spec: the single-header
//! layout, namespace routing, and free functions with basic types (required
//! arguments only). Classes, enums, methods, optional arguments, generics,
//! streaming, and companions land in later slices; symbols they gate on are
//! skipped and reported in a trailing header comment (no silent caps).
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
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{Function, Symbol, SymbolPool, Ty};
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
    let mut fns_by_namespace: BTreeMap<Vec<String>, Vec<EmittedFn>> = BTreeMap::new();
    let mut skipped: Vec<String> = Vec::new();

    let mut names: Vec<_> = pool.keys().collect();
    names.sort();

    for name in names {
        let symbol = &pool[name];
        let function = match symbol {
            Symbol::Function(function) => function,
            // Classes, enums, and aliases land in later slices.
            Symbol::Class(_) | Symbol::Enum(_) | Symbol::TypeAlias(_) => continue,
        };
        if name.pkg.as_str() != "user" {
            continue; // stdlib/vendor surfaces come with later slices
        }
        match emit_function(name, function) {
            Ok(emitted) => {
                let ns: Vec<String> = name
                    .namespace_path
                    .iter()
                    .map(|seg| sanitize(seg.as_str()))
                    .collect();
                fns_by_namespace.entry(ns).or_default().push(emitted);
            }
            Err(reason) => skipped.push(format!("{name}: {reason}")),
        }
    }

    let mut out = HashMap::new();
    out.insert(
        PathBuf::from("include/baml_sdk.hpp"),
        render_header(&fns_by_namespace, &skipped),
    );
    out.insert(
        PathBuf::from("src/bindings.cpp"),
        render_bindings(&fns_by_namespace),
    );
    out.insert(
        PathBuf::from("src/_inlinedbaml.cpp"),
        render_inlinedbaml(user_baml_files),
    );
    out
}

struct EmittedFn {
    cpp_name: String,
    fqn: String,
    ret: String,
    params: Vec<(String, String, String)>, // (cpp name, cpp type, wire name)
    doc: Option<String>,
    raises: Vec<String>,
}

fn emit_function(
    name: &baml_codegen_types::Name,
    function: &Function,
) -> Result<EmittedFn, String> {
    if !function.generic_params.is_empty() {
        return Err("generic functions land in a later slice".to_string());
    }
    if name.is_stream() || name.bare_name().contains('$') {
        return Err("companion functions land in a later slice".to_string());
    }
    let mut params = Vec::new();
    for arg in &function.arguments {
        if arg.default.is_some() {
            return Err("optional arguments land in a later slice".to_string());
        }
        let ty = translate_ty(&arg.ty)
            .ok_or_else(|| format!("argument `{}` has unsupported type {}", arg.name, arg.ty))?;
        params.push((sanitize(arg.name.as_str()), ty, arg.name.to_string()));
    }
    let ret = translate_return_ty(&function.return_type)
        .ok_or_else(|| format!("unsupported return type {}", function.return_type))?;

    let raises = match &function.throws {
        None => Vec::new(),
        Some(Ty::Union(items)) => items.iter().map(unqualified_leaf_name).collect(),
        Some(ty) => vec![unqualified_leaf_name(ty)],
    };

    Ok(EmittedFn {
        cpp_name: sanitize(name.bare_name()),
        fqn: name.to_string(),
        ret,
        params,
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

/// Slice-1 type table: primitives, containers, and null-normalized
/// optionals. Everything else returns None and the surrounding function is
/// skipped (reported, not silently dropped).
fn translate_ty(ty: &Ty) -> Option<String> {
    Some(match ty {
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
                baml_base::Literal::Bigint(_) => return None,
                baml_base::Literal::Float(_) => "double".to_string(),
                baml_base::Literal::String(_) => "std::string".to_string(),
                baml_base::Literal::Bool(_) => "bool".to_string(),
            }
        }
        Ty::List(inner) => format!("std::vector<{}>", translate_ty(inner)?),
        Ty::Map { key, value } => {
            if !matches!(key.as_ref(), Ty::String) {
                return None; // enum keys land with enum support
            }
            format!("std::map<std::string, {}>", translate_ty(value)?)
        }
        Ty::Union(items) => {
            // Null-normalization (spec D-unions v2): strip the null member,
            // wrap the rest in optional. Multi-member variants land with the
            // union codec in a later slice.
            let non_null: Vec<&Ty> = items.iter().filter(|t| !matches!(t, Ty::Null)).collect();
            let had_null = non_null.len() != items.len();
            match (had_null, non_null.as_slice()) {
                (true, [single]) => format!("std::optional<{}>", translate_ty(single)?),
                _ => return None,
            }
        }
        _ => return None,
    })
}

fn translate_return_ty(ty: &Ty) -> Option<String> {
    if matches!(ty, Ty::Unit) {
        return Some("void".to_string());
    }
    translate_ty(ty)
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

fn push_doc_comment(buf: &mut String, indent: &str, f: &EmittedFn) {
    if let Some(doc) = &f.doc {
        for line in doc.lines() {
            let _ = writeln!(buf, "{indent}/// {line}");
        }
    }
    if !f.raises.is_empty() {
        let _ = writeln!(buf, "{indent}/// Raises: {}", f.raises.join(", "));
    }
}

fn signature(f: &EmittedFn, async_variant: bool) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(name, ty, _)| format!("{} {}", by_value_or_cref(ty), name))
        .collect();
    let (ret, suffix) = if async_variant {
        (format!("::baml::Future<{}>", nonvoid(&f.ret)), "_async")
    } else {
        (f.ret.clone(), "")
    };
    format!("{ret} {}{suffix}({})", f.cpp_name, params.join(", "))
}

fn nonvoid(ret: &str) -> &str {
    if ret == "void" { "void" } else { ret }
}

fn by_value_or_cref(ty: &str) -> String {
    match ty {
        "int64_t" | "double" | "bool" | "std::monostate" => ty.to_string(),
        _ => format!("const {ty}&"),
    }
}

fn render_header(
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

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        for seg in ns {
            let _ = writeln!(buf, "namespace {seg} {{");
        }
        for f in fns {
            push_doc_comment(&mut buf, "", f);
            let _ = writeln!(buf, "{};", signature(f, false));
            let _ = writeln!(buf, "{};", signature(f, true));
        }
        for seg in ns.iter().rev() {
            let _ = writeln!(buf, "}}  // namespace {seg}");
        }
    }

    buf.push_str("\n}  // namespace baml_sdk\n");
    if !skipped.is_empty() {
        buf.push_str("\n// Symbols not yet emitted by this sdkgen_cpp slice:\n");
        for line in skipped {
            let _ = writeln!(buf, "//   {line}");
        }
    }
    buf.push_str("\n#endif  // BAML_SDK_HPP\n");
    buf
}

fn render_bindings(fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #include <baml_sdk.hpp>\n\n\
         #include <utility>\n\n\
         namespace baml_sdk {\n",
    );

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        for seg in ns {
            let _ = writeln!(buf, "namespace {seg} {{");
        }
        for f in fns {
            for async_variant in [false, true] {
                let _ = writeln!(buf, "\n{} {{", signature(f, async_variant));
                buf.push_str("    ::baml_sdk::detail::ensure_runtime();\n");
                buf.push_str("    ::baml::detail::ArgsEncoder args;\n");
                for (cpp_name, ty, wire_name) in &f.params {
                    let _ = writeln!(
                        buf,
                        "    args.add_arg(\"{wire_name}\", [&](::baml::detail::wire::Writer& w) {{ \
                         ::baml::codec<{ty}>::encode(w, {cpp_name}); }});"
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
                    ret = nonvoid(&f.ret),
                    fqn = f.fqn,
                );
                buf.push_str("}\n");
            }
        }
        for seg in ns.iter().rev() {
            let _ = writeln!(buf, "}}  // namespace {seg}");
        }
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
