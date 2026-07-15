//! Per-symbol Swift renderers: classes → Equatable/Sendable structs
//! with generated `BamlEncodable`/`BamlDecodable` conformances, enums →
//! `String`-raw enums, type aliases → `typealias`, functions →
//! sync/async body pairs calling `BamlRuntime`.

use std::fmt::Write as _;

use baml_codegen_types::{Class, Enum, Function, Name, Symbol, Ty, TypeAlias};

use crate::{
    escape_ident,
    translate_ty::{TranslateCtx, translate_optional_arg_inner, translate_ty},
};

pub(crate) fn render_docstring(doc: &str) -> String {
    let mut out = String::new();
    for line in doc.lines() {
        let _ = writeln!(out, "/// {line}");
    }
    out
}

/// Sort key groups: aliases, enums, classes, then functions — each
/// alphabetical. Keeps generated files stable and readable.
pub(crate) fn sort_key(symbol: &Symbol, bare: &str) -> String {
    let group = match symbol {
        Symbol::TypeAlias(_) => '0',
        Symbol::Enum(_) => '1',
        Symbol::Class(_) => '2',
        Symbol::Function(_) => '3',
    };
    format!("{group}:{bare}")
}

pub(crate) fn render_type_alias(alias: &TypeAlias, ctx: &TranslateCtx) -> Option<String> {
    // Swift `typealias` cannot be recursive; recursive aliases wait
    // for a boxed representation (they're all unions today anyway).
    if alias.recursive {
        return None;
    }
    let target = translate_ty(&alias.resolves_to, ctx)?;
    let name = escape_ident(alias.name.name.as_str());
    Some(format!("public typealias {name} = {target}\n"))
}

pub(crate) fn render_enum(enum_: &Enum, key: &Name) -> String {
    let name = escape_ident(enum_.name.name.as_str());
    let fqn = key.to_string();
    let doc = enum_
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();
    let mut out = format!(
        "{doc}public enum {name}: String, Equatable, Hashable, Sendable, CaseIterable, \
         BamlEncodable, BamlDecodable {{\n"
    );
    for variant in &enum_.variants {
        if let Some(vdoc) = variant.docstring.as_deref() {
            out.push_str(&indent_lines(&render_docstring(vdoc), 1));
        }
        let case_name = escape_ident(variant.name.as_str());
        if variant.value == variant.name.as_str() {
            let _ = writeln!(out, "\tcase {case_name}");
        } else {
            let _ = writeln!(out, "\tcase {case_name} = {:?}", variant.value);
        }
    }
    let _ = write!(
        out,
        "\n\tpublic func _bamlEncode() -> BamlInboundValue {{\n\
         \t\t.baml_enum(\"{fqn}\", rawValue)\n\
         \t}}\n\n\
         \tpublic static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
         \t\tlet variant = try v.enumVariant()\n\
         \t\tguard let member = {name}(rawValue: variant) else {{\n\
         \t\t\tthrow BamlDecodeError.typeMismatch(expected: \"{fqn} variant\", got: variant)\n\
         \t\t}}\n\
         \t\treturn member\n\
         \t}}\n\
         }}\n"
    );
    out
}

pub(crate) struct RenderedField {
    pub name: String,
    pub ty: String,
    pub boxed: bool,
    pub doc: Option<String>,
}

pub(crate) fn render_class(
    class: &Class,
    key: &Name,
    fields: &[RenderedField],
) -> String {
    let name = escape_ident(class.name.name.as_str());
    let fqn = key.to_string();
    let doc = class
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();

    let mut out = format!(
        "{doc}public struct {name}: Equatable, Sendable, BamlEncodable, BamlDecodable {{\n"
    );
    for field in fields {
        if let Some(fdoc) = &field.doc {
            out.push_str(&indent_lines(&render_docstring(fdoc), 1));
        }
        let wrapper = if field.boxed { "@BamlIndirect " } else { "" };
        let _ = writeln!(out, "\t{wrapper}public var {}: {}", field.name, field.ty);
    }

    // Explicit memberwise init (the synthesized one is internal, and
    // boxed fields need wrapper construction).
    let params = fields
        .iter()
        .map(|f| format!("{}: {}", f.name, f.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "\n\tpublic init({params}) {{");
    for field in fields {
        let fname = &field.name;
        if field.boxed {
            let _ = writeln!(
                out,
                "\t\tself._{} = BamlIndirect(wrappedValue: {fname})",
                fname.trim_matches('`')
            );
        } else {
            let _ = writeln!(out, "\t\tself.{fname} = {fname}");
        }
    }
    out.push_str("\t}\n");

    // Encode: shape-driven walk of the fields, FQN baked in.
    let field_pairs = fields
        .iter()
        .map(|f| format!("(\"{}\", {})", f.name.trim_matches('`'), f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = write!(
        out,
        "\n\tpublic func _bamlEncode() -> BamlInboundValue {{\n\
         \t\t.baml_class(\"{fqn}\", [{field_pairs}])\n\
         \t}}\n"
    );

    // Decode: field-dict walk; missing fields decode as null.
    let decode_args = fields
        .iter()
        .map(|f| {
            format!(
                "\t\t\t{}: try fields._baml(\"{}\")",
                f.name,
                f.name.trim_matches('`')
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    if fields.is_empty() {
        let _ = write!(
            out,
            "\n\tpublic static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
             \t\t_ = try v.classFields()\n\
             \t\treturn {name}()\n\
             \t}}\n\
             }}\n"
        );
    } else {
        let _ = write!(
            out,
            "\n\tpublic static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
             \t\tlet fields = try v.classFields()\n\
             \t\treturn {name}(\n{decode_args}\n\t\t)\n\
             \t}}\n\
             }}\n"
        );
    }
    out
}

/// Render one free function as a sync + async pair, or `None` if any
/// part of its signature is outside the supported subset.
pub(crate) fn render_function(key: &Name, function: &Function, ctx: &TranslateCtx) -> Option<String> {
    let bare = function.name.as_str();
    // `$stream` / `$build_request` companions come with their own
    // phases; `$` is not a Swift identifier character anyway.
    if bare.contains('$') || !function.generic_params.is_empty() {
        return None;
    }

    enum Param {
        Required { name: String, ty: String },
        Optional { name: String, inner: String },
    }

    let mut params = Vec::new();
    for arg in &function.arguments {
        let name = escape_ident(arg.name.as_str());
        if arg.default.is_some() {
            params.push(Param::Optional {
                name,
                inner: translate_optional_arg_inner(&arg.ty, ctx)?,
            });
        } else {
            params.push(Param::Required {
                name,
                ty: translate_ty(&arg.ty, ctx)?,
            });
        }
    }

    let ret = match &function.return_type {
        Ty::Unit => None,
        other => Some(translate_ty(other, ctx)?),
    };

    let fqn = key.to_string();
    let param_list = params
        .iter()
        .map(|p| match p {
            Param::Required { name, ty } => format!("{name}: {ty}"),
            Param::Optional { name, inner } => {
                format!("{name}: BamlOptional<{inner}> = .unset")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Required args inline into the array literal; optional slots
    // append conditionally (`.unset` omits the kwarg, Python-style).
    let required_pairs = params
        .iter()
        .filter_map(|p| match p {
            Param::Required { name, .. } => {
                Some(format!("(\"{}\", {name})", name.trim_matches('`')))
            }
            Param::Optional { .. } => None,
        })
        .collect::<Vec<_>>()
        .join(", ");
    let has_optionals = params.iter().any(|p| matches!(p, Param::Optional { .. }));
    let mut args_setup = if has_optionals {
        let mut setup = format!(
            "\tvar args: [(String, (any BamlEncodable)?)] = [{required_pairs}]\n"
        );
        for p in &params {
            if let Param::Optional { name, .. } = p {
                let _ = writeln!(
                    setup,
                    "\t{name}._appendIfSet(\"{}\", to: &args)",
                    name.trim_matches('`')
                );
            }
        }
        setup
    } else {
        String::new()
    };
    let args_expr = if has_optionals {
        "args".to_string()
    } else {
        format!("[{required_pairs}]")
    };
    if !args_setup.is_empty() {
        args_setup.push('\n');
    }

    let fn_name = escape_ident(bare);
    let async_name = escape_ident(&format!("{bare}_async"));
    let doc = function
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();

    let mut out = String::new();
    match &ret {
        Some(ret_ty) => {
            let _ = write!(
                out,
                "{doc}public static func {fn_name}({param_list}) throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\treturn try BamlRuntime.shared.callSync(\"{fqn}\", args: {args_expr})\n\
                 }}\n\n\
                 {doc}public static func {async_name}({param_list}) async throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\treturn try await BamlRuntime.shared.call(\"{fqn}\", args: {args_expr})\n\
                 }}\n"
            );
        }
        None => {
            let _ = write!(
                out,
                "{doc}public static func {fn_name}({param_list}) throws {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\ttry BamlRuntime.shared.callSyncVoid(\"{fqn}\", args: {args_expr})\n\
                 }}\n\n\
                 {doc}public static func {async_name}({param_list}) async throws {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\ttry await BamlRuntime.shared.callVoid(\"{fqn}\", args: {args_expr})\n\
                 }}\n"
            );
        }
    }
    Some(out)
}

pub(crate) fn indent_lines(block: &str, depth: usize) -> String {
    let tab = "\t".repeat(depth);
    let mut out = String::new();
    for line in block.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "{tab}{line}");
        }
    }
    out
}
