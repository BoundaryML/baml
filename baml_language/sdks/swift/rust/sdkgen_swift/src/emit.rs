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
        "{doc}public enum {name}: Swift.String, Equatable, Hashable, Sendable, CaseIterable, \
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
        "\n\tpublic static var _bamlArmIdentity: Swift.String? {{ \"{fqn}\" }}\n\n\
         \tpublic func _bamlEncode() -> BamlInboundValue {{\n\
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
    methods: &[String],
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
        "\n\tpublic static var _bamlArmIdentity: Swift.String? {{ \"{fqn}\" }}\n\n\
         \tpublic func _bamlEncode() -> BamlInboundValue {{\n\
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
             \t}}\n"
        );
    } else {
        let _ = write!(
            out,
            "\n\tpublic static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
             \t\tlet fields = try v.classFields()\n\
             \t\treturn {name}(\n{decode_args}\n\t\t)\n\
             \t}}\n"
        );
    }

    // Static and instance methods (already rendered by
    // render_callable with the class-scoped FQN).
    for method in methods {
        out.push('\n');
        out.push_str(&indent_lines(method, 1));
    }
    out.push_str("}\n");
    out
}

/// How a callable is bound: a free function in a namespace, a static
/// method on a class, or an instance method (whose receiver rides as
/// required kwarg 0 under the name `self`, the cross-bridge
/// convention Python implements via the descriptor protocol).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FnKind {
    Free,
    Static,
    Instance,
}

/// Render one callable as a sync + async pair, or `None` if any part
/// of its signature is outside the supported subset. `fqn` for a
/// method is `<class FQN>.<method name>`.
pub(crate) fn render_callable(
    fqn: &str,
    function: &Function,
    kind: FnKind,
    ctx: &TranslateCtx,
) -> Option<String> {
    let bare = function.name.as_str();
    // `$stream` / `$build_request` companions come with their own
    // phases; `$` is not a Swift identifier character anyway.
    if bare.contains('$') || !function.generic_params.is_empty() {
        return None;
    }

    enum Param {
        Required { name: String, ty: String },
        Optional { name: String, inner: String },
        /// A host callable: `ty` is the closure type; `wrapper` is a
        /// body-prelude statement building the erased BamlHostCallable.
        Callable { name: String, ty: String, wrapper: String },
    }

    let mut params = Vec::new();
    for arg in &function.arguments {
        let name = escape_ident(arg.name.as_str());
        if arg.default.is_some() {
            params.push(Param::Optional {
                name,
                inner: translate_optional_arg_inner(&arg.ty, ctx)?,
            });
        } else if let Ty::Callable { params: cparams, ret } = &arg.ty {
            let (ty, wrapper) = render_callable_param(&name, cparams, ret, ctx)?;
            params.push(Param::Callable { name, ty, wrapper });
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

    let param_list = params
        .iter()
        .map(|p| match p {
            Param::Required { name, ty } => format!("{name}: {ty}"),
            Param::Optional { name, inner } => {
                format!("{name}: BamlOptional<{inner}> = .unset")
            }
            Param::Callable { name, ty, .. } => format!("{name}: {ty}"),
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Required args inline into the array literal; optional slots
    // append conditionally (`.unset` omits the kwarg, Python-style).
    // Instance methods pass the receiver as required kwarg 0.
    let mut required_pair_list: Vec<String> = Vec::new();
    if kind == FnKind::Instance {
        required_pair_list.push("(\"self\", self)".to_string());
    }
    required_pair_list.extend(params.iter().filter_map(|p| match p {
        Param::Required { name, .. } => {
            Some(format!("(\"{}\", {name})", name.trim_matches('`')))
        }
        Param::Callable { name, .. } => Some(format!(
            "(\"{}\", _baml_{})",
            name.trim_matches('`'),
            name.trim_matches('`')
        )),
        Param::Optional { .. } => None,
    }));
    let required_pairs = required_pair_list.join(", ");
    let has_optionals = params.iter().any(|p| matches!(p, Param::Optional { .. }));
    let mut args_setup = String::new();
    for p in &params {
        if let Param::Callable { wrapper, .. } = p {
            args_setup.push_str(wrapper);
        }
    }
    if has_optionals {
        let _ = write!(
            args_setup,
            "\tvar args: [(Swift.String, (any BamlEncodable)?)] = [{required_pairs}]\n"
        );
        for p in &params {
            if let Param::Optional { name, .. } = p {
                let _ = writeln!(
                    args_setup,
                    "\t{name}._appendIfSet(\"{}\", to: &args)",
                    name.trim_matches('`')
                );
            }
        }
    }
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
    let mut doc = function
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();
    // Thrown types are documented, never in the signature — the Swift
    // analog of Python's `Raises:` docstring block (declared `throws`
    // clauses and inferred contracts both land here).
    if let Some(thrown) = &function.throws {
        let names = thrown_leaf_names(thrown);
        if !names.is_empty() {
            doc.push_str(&format!("/// - Throws: {}\n", names.join(", ")));
        }
    }

    let mut out = String::new();
    let static_kw = if kind == FnKind::Instance { "" } else { "static " };
    match &ret {
        Some(ret_ty) => {
            let _ = write!(
                out,
                "{doc}public {static_kw}func {fn_name}({param_list}) throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\treturn try BamlRuntime.shared.callSync(\"{fqn}\", args: {args_expr})\n\
                 }}\n\n\
                 {doc}public {static_kw}func {async_name}({param_list}) async throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\treturn try await BamlRuntime.shared.call(\"{fqn}\", args: {args_expr})\n\
                 }}\n"
            );
        }
        None => {
            let _ = write!(
                out,
                "{doc}public {static_kw}func {fn_name}({param_list}) throws {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\ttry BamlRuntime.shared.callSyncVoid(\"{fqn}\", args: {args_expr})\n\
                 }}\n\n\
                 {doc}public {static_kw}func {async_name}({param_list}) async throws {{\n\
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


/// A recursive union alias (`type RecList = int | RecList[]`) can't be
/// a `typealias` (no self-reference), so it becomes a nominal
/// `indirect enum` under the USER'S name with the exact BamlUnionN
/// surface: positional cases, type-directed inits, accessors, `match`,
/// `value(as:)`/`holds`/`anyValue`, and the same metadata-first codec.
pub(crate) fn render_recursive_union_alias(
    key: &Name,
    arms: &[Ty],
    ctx: &TranslateCtx,
) -> Option<String> {
    let name = escape_ident(key.bare_name());
    let arm_tys: Vec<String> = {
        let mut tys = Vec::new();
        for arm in arms {
            let ty = translate_ty(arm, ctx)?;
            if !tys.contains(&ty) {
                tys.push(ty);
            }
        }
        tys
    };
    let n = arm_tys.len();
    if n < 2 {
        return None;
    }

    let mut out = format!(
        "/// Recursive union alias — nominal stand-in for BamlUnion{n} (a\n\
         /// `typealias` can't self-reference); same surface, same codec.\n\
         public indirect enum {name}: Equatable, Sendable, BamlEncodable, BamlDecodable {{\n"
    );
    for (i, ty) in arm_tys.iter().enumerate() {
        let _ = writeln!(out, "\tcase t{i}({ty})");
    }
    out.push('\n');
    for (i, ty) in arm_tys.iter().enumerate() {
        let _ = writeln!(out, "\tpublic init(_ value: {ty}) {{ self = .t{i}(value) }}");
    }
    out.push('\n');
    out.push_str("\tpublic var anyValue: Any {\n\t\tswitch self {\n");
    for i in 0..n {
        let _ = writeln!(out, "\t\tcase .t{i}(let v): return v");
    }
    out.push_str("\t\t}\n\t}\n\n");
    for (i, ty) in arm_tys.iter().enumerate() {
        let _ = writeln!(
            out,
            "\tpublic var t{i}: {ty}? {{ if case .t{i}(let v) = self {{ return v }} else {{ return nil }} }}"
        );
    }
    out.push('\n');
    out.push_str("\tpublic func value<T>(as type: T.Type) -> T? { anyValue as? T }\n");
    out.push_str("\tpublic func holds<T>(_ type: T.Type) -> Swift.Bool { value(as: type) != nil }\n\n");

    let match_params = arm_tys
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("t{i} onT{i}: ({ty}) throws -> R"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "\tpublic func match<R>({match_params}) rethrows -> R {{");
    out.push_str("\t\tswitch self {\n");
    for i in 0..n {
        let _ = writeln!(out, "\t\tcase .t{i}(let v): return try onT{i}(v)");
    }
    out.push_str("\t\t}\n\t}\n\n");

    out.push_str("\tpublic func _bamlEncode() -> BamlInboundValue {\n\t\tswitch self {\n");
    for i in 0..n {
        let _ = writeln!(out, "\t\tcase .t{i}(let v): return v._bamlEncode()");
    }
    out.push_str("\t\t}\n\t}\n\n");

    let _ = writeln!(
        out,
        "\tpublic static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{"
    );
    out.push_str("\t\tif let fqn = v.wireClassFQN() {\n");
    for (i, arm) in arms.iter().enumerate() {
        if let Ty::Class(class_name, _) = arm {
            let _ = writeln!(
                out,
                "\t\t\tif fqn == \"{class_name}\" {{ return .t{i}(try {}._bamlDecode(v)) }}",
                arm_tys[i]
            );
        }
    }
    out.push_str("\t\t}\n");
    for (i, ty) in arm_tys.iter().enumerate() {
        let _ = writeln!(
            out,
            "\t\tif let value = try? {ty}._bamlDecode(v) {{ return .t{i}(value) }}"
        );
    }
    let _ = write!(
        out,
        "\t\tthrow BamlDecodeError.typeMismatch(expected: \"{name}\", got: \"unmatched union value\")\n\
         \t}}\n\
         }}\n"
    );
    Some(out)
}



/// Unqualified leaf names of a throws contract, for doc rendering.
fn thrown_leaf_names(ty: &Ty) -> Vec<String> {
    match ty {
        Ty::Class(name, _) => vec![format!("`{}`", name.bare_name())],
        Ty::Enum(name) | Ty::TypeAlias(name) => vec![format!("`{}`", name.bare_name())],
        Ty::Union(members) => members.iter().flat_map(thrown_leaf_names).collect(),
        _ => Vec::new(),
    }
}


/// Closure type + erased-wrapper prelude for one host-callable
/// parameter. The closure is `async throws` uniformly (sync closures
/// coerce), and the wrapper maps the engine's supplied-args payload
/// onto the closure's positional/optional parameters.
fn render_callable_param(
    name: &str,
    cparams: &[baml_codegen_types::CallableParam],
    ret: &Ty,
    ctx: &TranslateCtx,
) -> Option<(String, String)> {
    let bare = name.trim_matches('`');
    let mut sig_parts: Vec<String> = Vec::new();
    let mut invoke_args: Vec<String> = Vec::new();
    let mut positional = 0usize;
    for cp in cparams {
        match cp.mode {
            baml_codegen_types::CodegenFunctionParamMode::Required => {
                sig_parts.push(translate_ty(&cp.ty, ctx)?);
                invoke_args.push(format!("try _args.required({positional})"));
                positional += 1;
            }
            baml_codegen_types::CodegenFunctionParamMode::Optional => {
                let inner = translate_optional_arg_inner(&cp.ty, ctx)?;
                sig_parts.push(format!("BamlOptional<{inner}>"));
                let arg_name = cp.name.as_ref()?.as_str();
                invoke_args.push(format!("try _args.optional(\"{arg_name}\")"));
            }
        }
    }
    let invoke = invoke_args.join(", ");
    let (ret_ty, wrapper_body) = match ret {
        Ty::Unit => (
            "Swift.Void".to_string(),
            format!(
                "try await {name}({invoke})\n\t\treturn BamlNull()._bamlEncode()"
            ),
        ),
        other => (
            translate_ty(other, ctx)?,
            format!("try await {name}({invoke})._bamlEncode()"),
        ),
    };
    let closure_ty = format!(
        "@escaping @Sendable ({}) async throws -> {ret_ty}",
        sig_parts.join(", ")
    );
    let wrapper = format!(
        "\tlet _baml_{bare} = BamlHostCallable {{ _args in\n\t\t{wrapper_body}\n\t}}\n"
    );
    Some((closure_ty, wrapper))
}
