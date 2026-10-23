//! Per-symbol Swift renderers: classes → Equatable/Sendable structs
//! with generated `BamlEncodable`/`BamlDecodable` conformances, enums →
//! `String`-raw enums, type aliases → `typealias`, functions →
//! sync/async body pairs calling `BamlRuntime`.

use std::fmt::Write as _;

use baml_sdkgen_types::{
    Class, Enum, Function, Name, Symbol, Ty, TypeAlias, is_synthetic_effect_param,
};

use crate::{
    escape_ident,
    translate_ty::{
        TranslateCtx, arm_slot, generic_param_slot, optional_arg_inner_slot, own_slot, slot_expr,
        translate_optional_arg_inner, translate_ty, union_arms,
    },
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

pub(crate) fn render_type_alias(
    alias: &TypeAlias,
    key: &Name,
    ctx: &TranslateCtx,
) -> Option<String> {
    // Swift `typealias` cannot be recursive; recursive aliases wait
    // for a boxed representation (they're all unions today anyway).
    if alias.recursive {
        return None;
    }
    let target = translate_ty(&alias.resolves_to, ctx)?;
    let name = escape_ident(key.name().as_str());
    Some(format!("public typealias {name} = {target}\n"))
}

pub(crate) fn render_enum(enum_: &Enum, key: &Name) -> String {
    let name = escape_ident(key.name().as_str());
    let fqn = key.to_string();
    let doc = enum_
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();
    let mut out = format!(
        "{doc}public nonisolated enum {name}: Swift.String, Equatable, Hashable, Sendable, CaseIterable, \
         BamlTyped, BamlLeafEncodable, BamlDecodable {{\n"
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
         \tpublic static var _bamlSlot: BamlSlot<Self> {{ ._leaf(.enumType(\"{fqn}\")) }}\n\n\
         \tpublic func _bamlEncode(as _: BamlSlot<Self>) -> BamlInboundValue {{\n\
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
    /// `$rust_type` field (opaque engine handle).
    pub is_rust: bool,
    /// The field's [`slot_expr`], its type variables spelled as the class
    /// slot's arguments ([`class_argument_local`]).
    pub slot: String,
}

/// The local holding the slot of a class's generic parameter `index` inside
/// its `_bamlEncode`.
pub(crate) fn class_argument_local(index: usize) -> String {
    format!("_bamlArgument{index}")
}

pub(crate) fn render_class(
    class: &Class,
    key: &Name,
    fields: &[RenderedField],
    methods: &[String],
) -> String {
    let name = escape_ident(key.name().as_str());
    let fqn = key.to_string();
    let doc = class
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();

    // Generic classes: `class Wrapper<T>` → `struct Wrapper<T:
    // BamlCodableValue>`. An instance states the class with every type
    // argument, taken from the slot it fills.
    let generics = if class.generic_params.is_empty() {
        String::new()
    } else {
        format!(
            "<{}>",
            class
                .generic_params
                .iter()
                .map(|p| format!("{}: BamlCodableValue", escape_ident(p.as_str())))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    // A stdlib media wrapper is the media type itself on the wire: a node
    // whose payload implies `image`/`audio`/..., never a class value.
    let media = baml_base::MediaKind::from_wrapper_class_name(&fqn);
    let leaf_conformance = if media.is_some() {
        ", BamlLeafEncodable"
    } else {
        ""
    };

    let mut out = format!(
        "{doc}public struct {name}{generics}: Equatable, Sendable, BamlTyped{leaf_conformance}, BamlDecodable {{\n"
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
    let _ = writeln!(out, "\n\tpublic nonisolated init({params}) {{");
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

    let _ = write!(
        out,
        "\n\tpublic nonisolated static var _bamlArmIdentity: Swift.String? {{ \"{fqn}\" }}\n"
    );
    if let Some(kind) = media {
        let data = fields.iter().find(|f| f.is_rust);
        debug_assert!(
            fields.len() == 1 && data.is_some(),
            "media wrapper `{fqn}` must hold exactly its `$rust_type` payload"
        );
        let data = data.map_or_else(|| "nil".to_string(), |f| format!("self.{}", f.name));
        let kind = match kind {
            baml_base::MediaKind::Image => "image",
            baml_base::MediaKind::Audio => "audio",
            baml_base::MediaKind::Video => "video",
            baml_base::MediaKind::Pdf => "pdf",
            baml_base::MediaKind::Generic => "generic",
        };
        let _ = write!(
            out,
            "\n\tpublic nonisolated static var _bamlSlot: BamlSlot<Self> {{ ._leaf(.media(.{kind})) }}\n\n\
             \tpublic nonisolated func _bamlEncode(as _: BamlSlot<Self>) -> BamlInboundValue {{\n\
             \t\t.baml_media({data})\n\
             \t}}\n"
        );
    } else {
        let params = &class.generic_params;
        // `._classType` with no arguments for a non-generic class.
        let mut natural = String::new();
        for p in params {
            let _ = write!(natural, ", {}", own_slot(&escape_ident(p.as_str())));
        }
        let _ = write!(
            out,
            "\n\tpublic nonisolated static var _bamlSlot: BamlSlot<Self> {{ ._classType(\"{fqn}\"{natural}) }}\n"
        );
        let mut body = String::new();
        for (i, p) in params.iter().enumerate() {
            let _ = writeln!(
                body,
                "\t\tlet {}: BamlSlot<{}> = slot._classArgument({i})",
                class_argument_local(i),
                escape_ident(p.as_str())
            );
        }
        // `self.` so a field named like a local or the `slot` parameter
        // still reads the field.
        let mut field_list = String::new();
        for f in fields {
            let _ = writeln!(
                field_list,
                "\t\t\tBamlField(\"{}\", self.{}, as: {}),",
                f.name.trim_matches('`'),
                f.name,
                f.slot
            );
        }
        let _ = write!(
            out,
            "\n\tpublic nonisolated func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {{\n\
             {body}\t\treturn try .baml_class(slot, [\n{field_list}\t\t])\n\
             \t}}\n"
        );
    }

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
    // Media shape: a class whose ONLY field is a `$rust_type` handle
    // arrives from the engine as a bare tagged handle (ADT_MEDIA_*),
    // not a class value — decode accepts both forms.
    let media_fallback = if fields.len() == 1 && fields[0].is_rust {
        format!(
            "\t\tif let handle = try? BamlHandle._bamlDecode(v) {{\n\
             \t\t\treturn {name}({}: handle)\n\
             \t\t}}\n",
            fields[0].name
        )
    } else {
        String::new()
    };

    if fields.is_empty() {
        let _ = write!(
            out,
            "\n\tpublic nonisolated static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
             \t\t_ = try v.classFields()\n\
             \t\treturn {name}()\n\
             \t}}\n"
        );
    } else {
        let _ = write!(
            out,
            "\n\tpublic nonisolated static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
             {media_fallback}\t\tlet fields = try v.classFields()\n\
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

/// An opaque class (`reflect.Package`, …): the engine's instance, held by
/// reference. The Swift type stores only the handle it arrived as and sends
/// that back as itself; a host never builds one (no public initializer) nor
/// reads its declared fields, which are the runtime's own. Equal values are
/// the same instance.
pub(crate) fn render_opaque_class(class: &Class, key: &Name, methods: &[String]) -> String {
    let fqn = key.to_string();
    // A generic opaque class would have to state its type arguments, which
    // the handle alone does not carry; no standard-library class is one.
    debug_assert!(
        class.generic_params.is_empty(),
        "opaque class `{fqn}` is generic"
    );
    let name = escape_ident(key.name().as_str());
    let doc = class
        .docstring
        .as_deref()
        .map(render_docstring)
        .unwrap_or_default();
    let mut out = format!(
        "{doc}public struct {name}: Equatable, Sendable, BamlTyped, BamlLeafEncodable, BamlDecodable {{\n\
         \tprivate let _bamlHandle: BamlHandle\n\n\
         \tprivate nonisolated init(_bamlHandle: BamlHandle) {{\n\
         \t\tself._bamlHandle = _bamlHandle\n\
         \t}}\n\n\
         \tpublic nonisolated static var _bamlArmIdentity: Swift.String? {{ \"{fqn}\" }}\n\n\
         \tpublic nonisolated static var _bamlSlot: BamlSlot<Self> {{ ._leaf(.classType(\"{fqn}\")) }}\n\n\
         \tpublic nonisolated func _bamlEncode(as slot: BamlSlot<Self>) -> BamlInboundValue {{\n\
         \t\t_bamlHandle._bamlEncode(as: ._leaf(slot.type))\n\
         \t}}\n\n\
         \tpublic nonisolated static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {{\n\
         \t\tSelf(_bamlHandle: try BamlHandle._bamlDecodeOpaque(v, of: \"{fqn}\"))\n\
         \t}}\n"
    );
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
/// convention Python implements via the descriptor protocol). A method
/// carries its class's generic parameters, which its Swift struct binds.
#[derive(Clone, Copy)]
pub(crate) enum FnKind<'a> {
    Free,
    Static { class_params: &'a [baml_base::Name] },
    Instance { class_params: &'a [baml_base::Name] },
}

/// Render one callable as a sync + async pair, or `None` if any part
/// of its signature is outside the supported subset. `fqn` for a
/// method is `<class FQN>.<method name>`.
pub(crate) fn render_callable(
    fqn: &str,
    binding_name: &str,
    function: &Function,
    kind: FnKind<'_>,
    ctx: &TranslateCtx,
) -> Option<String> {
    // Items before statements (clippy::items_after_statements).
    enum Param {
        Required {
            name: String,
            ty: String,
            slot: String,
        },
        Optional {
            name: String,
            inner: String,
            inner_slot: String,
        },
        /// A host callable: `ty` is the closure type; `wrapper` is a
        /// body-prelude statement building the erased `BamlHostCallable`
        /// into the local `local`.
        Callable {
            name: String,
            ty: String,
            local: String,
            wrapper: String,
        },
    }

    // The caller allocates a collision-safe Swift name for this scope.
    // The wire FQN keeps `@` verbatim; an `@stream` companion remains an
    // ordinary function returning `ai.stream.Stream<T>` (→ BamlStream).
    let bare = binding_name;

    // A returned callable's Swift type is declared beside the function, outside
    // the scope of any generic parameter, so it cannot mention one.
    if returned_callable_type_var(&function.return_type).is_some() {
        return None;
    }

    let generic_sig = if function.generic_params.is_empty() {
        String::new()
    } else {
        format!(
            "<{}>",
            function
                .generic_params
                .iter()
                .map(|p| format!("{}: BamlCodableValue", escape_ident(p.as_str())))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    // A Swift generic always knows its `T`, while the engine binds a type
    // parameter from the values only where one pins it: not in
    // `one_type_arg<T>()`, nor in `tag_or_value<T>(x: T | string | null)` with
    // a `string`. So every type parameter of the callee rides the wire as an
    // explicit type argument, the one the arguments are encoded under. The
    // class's come first, the wire's De Bruijn order: all of them for an
    // instance method, whose receiver is stated with them, and the ones a
    // static method mentions.
    let class_params: Vec<&baml_base::Name> = match kind {
        FnKind::Free => Vec::new(),
        FnKind::Static { class_params } => class_params
            .iter()
            .filter(|param| callable_mentions_type_var(function, param.as_str()))
            .collect(),
        FnKind::Instance { class_params } => class_params.iter().collect(),
    };
    let type_args = class_params
        .into_iter()
        .chain(&function.generic_params)
        .map(|param| {
            format!(
                "BamlTypeArgument({:?}, {}.type)",
                param.as_str(),
                own_slot(&escape_ident(param.as_str()))
            )
        })
        .collect::<Vec<_>>();
    let type_args_arg = if type_args.is_empty() {
        String::new()
    } else {
        format!(", typeArgs: [{}]", type_args.join(", "))
    };
    // A type parameter no required argument's Swift type mentions cannot be
    // inferred by Swift (`parse_as<T>(source: string) -> T`, or a `throws E`
    // of a callback), so the caller names it with a metatype parameter
    // labelled like it: `T: Int.self`.
    let explicit_type_params = function
        .generic_params
        .iter()
        .filter(|param| {
            !function
                .arguments
                .iter()
                .any(|arg| arg.default.is_none() && swift_type_mentions(&arg.ty, param.as_str()))
        })
        .map(|param| {
            let swift = escape_ident(param.as_str());
            format!("{swift} _: {swift}.Type")
        })
        .collect::<Vec<_>>();

    // A callable parameter's body locals are named by its position under a
    // prefix no parameter name starts with: no local shadows a parameter,
    // and two parameters' locals differ whatever their names (`f` and
    // `f_returns` included).
    let callable_local_prefix = {
        let mut prefix = "_baml_p".to_string();
        while function
            .arguments
            .iter()
            .any(|arg| arg.name.as_str().starts_with(&prefix))
        {
            prefix.push('_');
        }
        prefix
    };
    let mut params = Vec::new();
    for (position, arg) in function.arguments.iter().enumerate() {
        let name = escape_ident(arg.name.as_str());
        if arg.default.is_some() {
            params.push(Param::Optional {
                name,
                inner: translate_optional_arg_inner(&arg.ty, ctx)?,
                inner_slot: optional_arg_inner_slot(&arg.ty, ctx, &generic_param_slot)?,
            });
        } else if let Ty::Function {
            params: cparams,
            ret,
            throws,
        } = &arg.ty
        {
            let local = format!("{callable_local_prefix}{position}");
            let (ty, wrapper) = render_callable_param(&name, &local, cparams, ret, throws, ctx)?;
            params.push(Param::Callable {
                name,
                ty,
                local,
                wrapper,
            });
        } else {
            params.push(Param::Required {
                name,
                ty: translate_ty(&arg.ty, ctx)?,
                slot: slot_expr(&arg.ty, ctx, &generic_param_slot)?,
            });
        }
    }

    let returned_callable = if let Ty::Function {
        params: callable_params,
        ret,
        ..
    } = &function.return_type
    {
        Some(render_returned_callable(
            callable_params,
            ret,
            ctx,
            &format!("_BamlReturned_{:016x}", {
                use std::hash::{Hash, Hasher};
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                fqn.hash(&mut hash);
                hash.finish()
            }),
        )?)
    } else {
        None
    };
    let ret = match &function.return_type {
        // `never` (diverging: panic/exit) spells as a void function —
        // the call only ever returns by throwing (Python maps both
        // void and never to `None` the same way).
        Ty::Never => None,
        ret if ret.is_unit() => None,
        Ty::Function { .. } => Some(returned_callable.as_ref()?.0.clone()),
        other => Some(translate_ty(other, ctx)?),
    };

    let application_param_list = params
        .iter()
        .map(|p| match p {
            Param::Required { name, ty, .. } => format!("{name}: {ty}"),
            Param::Optional { name, inner, .. } => {
                format!("{name}: BamlOptional<{inner}> = .unset")
            }
            Param::Callable { name, ty, .. } => format!("{name}: {ty}"),
        })
        .chain(explicit_type_params)
        .collect::<Vec<_>>()
        .join(", ");

    let unused_name = |base: &str| {
        let mut name = base.to_string();
        while function
            .arguments
            .iter()
            .any(|arg| arg.name.as_str() == name)
        {
            name.push('_');
        }
        name
    };
    let control_name = unused_name("baml");
    // The kwargs local must not shadow a parameter of the same name.
    let args_name = unused_name("args");
    let param_list = if application_param_list.is_empty() {
        format!("{control_name}: Baml.BamlOptions = .init()")
    } else {
        format!("{application_param_list}, {control_name}: Baml.BamlOptions = .init()")
    };
    // Required args inline into the array literal; optional slots
    // append conditionally (`.unset` omits the kwarg, Python-style).
    // Instance methods pass the receiver as required kwarg 0.
    let mut required_pair_list: Vec<String> = Vec::new();
    if matches!(kind, FnKind::Instance { .. }) {
        required_pair_list.push("BamlArgument(\"self\", self, as: BamlSlot<Self>.own)".to_string());
    }
    required_pair_list.extend(params.iter().filter_map(|p| match p {
        Param::Required { name, slot, .. } => Some(format!(
            "BamlArgument(\"{}\", {name}, as: {slot})",
            name.trim_matches('`')
        )),
        Param::Callable { name, local, .. } => Some(format!(
            "BamlArgument(\"{}\", {local})",
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
        let _ = writeln!(
            args_setup,
            "\tvar {args_name}: [BamlArgument] = [{required_pairs}]"
        );
        for p in &params {
            if let Param::Optional {
                name, inner_slot, ..
            } = p
            {
                let _ = writeln!(
                    args_setup,
                    "\t{name}._appendIfSet(\"{}\", as: {inner_slot}, to: &{args_name})",
                    name.trim_matches('`')
                );
            }
        }
    }
    let args_expr = if has_optionals {
        args_name
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
            let _ = writeln!(doc, "/// - Throws: {}", names.join(", "));
        }
    }

    let mut out = String::new();
    let static_kw = if matches!(kind, FnKind::Instance { .. }) {
        ""
    } else {
        "static "
    };
    if fqn == "trace.current_context" {
        let ret_ty = ret.as_ref()?;
        return Some(format!(
            "{doc}public {static_kw}func {fn_name}({param_list}) throws -> {ret_ty} {{ _ = Baml._initialized; return try BamlRuntime.shared.currentTraceContext() }}\n\n{doc}public {static_kw}func {async_name}({param_list}) async throws -> {ret_ty} {{ _ = Baml._initialized; return try BamlRuntime.shared.currentTraceContext() }}\n"
        ));
    }
    match &ret {
        Some(ret_ty) => {
            if let Some((_, closure, definition)) = &returned_callable {
                let _ = write!(
                    out,
                    "{definition}\n{doc}public {static_kw}func {fn_name}{generic_sig}({param_list}) throws -> {ret_ty} {{\n\
                     \t_ = Baml._initialized\n\
                     {args_setup}\tlet _raw = try BamlRuntime.shared.callRawSync(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered())\n\
                     \tlet _function = try BamlFunctionHandle.decode(_raw)\n\
                     {closure}\n\
                     }}\n\n\
                     {doc}public {static_kw}func {async_name}{generic_sig}({param_list}) async throws -> {ret_ty} {{\n\
                     \t_ = Baml._initialized\n\
                     {args_setup}\tlet _raw = try await BamlRuntime.shared.callRaw(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered())\n\
                     \tlet _function = try BamlFunctionHandle.decode(_raw)\n\
                     {closure}\n\
                     }}\n"
                );
            } else {
                let _ = write!(
                    out,
                    "{doc}public {static_kw}func {fn_name}{generic_sig}({param_list}) throws -> {ret_ty} {{\n\
                     \t_ = Baml._initialized\n\
                     {args_setup}\treturn try BamlRuntime.shared.callSync(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered())\n\
                     }}\n\n\
                     {doc}public {static_kw}func {async_name}{generic_sig}({param_list}) async throws -> {ret_ty} {{\n\
                     \t_ = Baml._initialized\n\
                     {args_setup}\treturn try await BamlRuntime.shared.call(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered())\n\
                     }}\n"
                );
            }
        }
        None => {
            let _ = write!(
                out,
                "{doc}public {static_kw}func {fn_name}{generic_sig}({param_list}) throws {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\ttry BamlRuntime.shared.callSyncVoid(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered())\n\
                 }}\n\n\
                 {doc}public {static_kw}func {async_name}{generic_sig}({param_list}) async throws {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\ttry await BamlRuntime.shared.callVoid(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered())\n\
                 }}\n"
            );
        }
    }
    Some(out)
}

/// Does `ty` mention the type variable `name` anywhere, a callable's
/// signature included?
pub(crate) fn mentions_type_var(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::TypeVar(v) => v.as_str() == name,
        _ => baml_sdkgen_types::any_type_child(ty, |child| mentions_type_var(child, name)),
    }
}

/// Does the Swift type of a value of `ty` mention the type variable `name`?
/// A closure's Swift type has its parameters and result but no throws type,
/// so a variable only a callable's throws type mentions is not inferable by
/// Swift.
fn swift_type_mentions(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::TypeVar(v) => v.as_str() == name,
        Ty::Function { params, ret, .. } => {
            params
                .iter()
                .any(|param| swift_type_mentions(&param.ty, name))
                || swift_type_mentions(ret, name)
        }
        _ => baml_sdkgen_types::any_type_child(ty, |child| swift_type_mentions(child, name)),
    }
}

/// The first generic parameter a returned callable's Swift type (its
/// parameters and result) mentions, or `None` when `return_type` is not a
/// callable or mentions none. The throws type is not part of the Swift type,
/// and a synthetic effect parameter is no Swift generic parameter.
pub(crate) fn returned_callable_type_var(return_type: &Ty) -> Option<String> {
    fn first(ty: &Ty) -> Option<String> {
        if let Ty::TypeVar(param) = ty
            && !is_synthetic_effect_param(param.name())
        {
            return Some(param.as_str().to_string());
        }
        let mut found = None;
        baml_sdkgen_types::any_type_child(ty, |child| {
            found = first(child);
            found.is_some()
        });
        found
    }
    let Ty::Function { params, ret, .. } = return_type else {
        return None;
    };
    params
        .iter()
        .map(|param| &param.ty)
        .chain([&**ret])
        .find_map(first)
}

/// Does `function`'s signature (its arguments, return type or throws type)
/// mention the type variable `name`?
fn callable_mentions_type_var(function: &Function, name: &str) -> bool {
    function
        .arguments
        .iter()
        .map(|arg| &arg.ty)
        .chain([&function.return_type])
        .chain(&function.throws)
        .any(|ty| mentions_type_var(ty, name))
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
/// `indirect enum` under the USER'S name with the exact `BamlUnionN`
/// surface: positional cases, type-directed inits, accessors, `match`,
/// `value(as:)`/`holds`/`anyValue`, and the same metadata-first codec.
///
/// Its slot is the alias by name, the one alias that exists at run time:
/// a `RecList[]` states `RecList[]`. Each case encodes under its arm's slot,
/// which the alias's declaration fixes. A null-bearing alias (stdlib
/// `json`) is referenced as `Name?`, whose `nil` is the alias's own null, so
/// `Name?` states the alias too.
pub(crate) fn render_recursive_union_alias(
    key: &Name,
    arms: &[Ty],
    nullable: bool,
    ctx: &TranslateCtx,
) -> Option<String> {
    let name = escape_ident(key.name().as_str());
    let fqn = key.to_string();
    let arms = union_arms(arms, ctx)?;
    let n = arms.len();
    if n < 2 {
        return None;
    }
    let arm_tys: Vec<&str> = arms.iter().map(|arm| arm.swift.as_str()).collect();
    let arm_slots = arms
        .iter()
        .map(|arm| arm_slot(arm, ctx, &generic_param_slot))
        .collect::<Option<Vec<_>>>()?;

    let mut out = format!(
        "/// Recursive union alias — nominal stand-in for BamlUnion{n} (a\n\
         /// `typealias` can't self-reference); same surface, same codec.\n\
         public nonisolated indirect enum {name}: Equatable, Sendable, BamlTyped, BamlLeafEncodable, BamlDecodable {{\n"
    );
    for (i, ty) in arm_tys.iter().enumerate() {
        let _ = writeln!(out, "\tcase t{i}({ty})");
    }
    out.push('\n');
    // Cases of one Swift type (`"a"[] | "b"[]`) have no type-directed
    // init: the type cannot say which case, so only the positional case can.
    for (i, ty) in arm_tys.iter().enumerate() {
        if arm_tys.iter().filter(|other| *other == ty).count() == 1 {
            let _ = writeln!(
                out,
                "\tpublic init(_ value: {ty}) {{ self = .t{i}(value) }}"
            );
        }
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
    out.push_str(
        "\tpublic func holds<T>(_ type: T.Type) -> Swift.Bool { value(as: type) != nil }\n\n",
    );

    let match_params = arm_tys
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("t{i} onT{i}: ({ty}) throws -> R"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "\tpublic func match<R>({match_params}) rethrows -> R {{"
    );
    out.push_str("\t\tswitch self {\n");
    for i in 0..n {
        let _ = writeln!(out, "\t\tcase .t{i}(let v): return try onT{i}(v)");
    }
    out.push_str("\t\t}\n\t}\n\n");

    let _ = writeln!(
        out,
        "\tpublic static var _bamlSlot: BamlSlot<Self> {{ ._leaf(.typeAlias(\"{fqn}\")) }}\n"
    );
    if nullable {
        let _ = writeln!(
            out,
            "\tpublic static var _bamlOptionalSlot: BamlSlot<Self?> {{ BamlSlots.nullable(_bamlSlot) }}\n"
        );
    }
    out.push_str(
        "\tpublic func _bamlEncode(as _: BamlSlot<Self>) throws -> BamlInboundValue {\n\t\tswitch self {\n",
    );
    for (i, slot) in arm_slots.iter().enumerate() {
        let _ = writeln!(
            out,
            "\t\tcase .t{i}(let v): return try {slot}._bamlEncode(v)"
        );
    }
    out.push_str("\t\t}\n\t}\n\n");

    let _ = writeln!(
        out,
        "\tpublic static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{"
    );
    out.push_str("\t\tif let selected = try v.unionSelectedType() {\n");
    for (i, ty) in arm_tys.iter().enumerate() {
        let _ = writeln!(
            out,
            "\t\t\tif {ty}._bamlDecodeSelects(selected) {{ return .t{i}(try {ty}._bamlDecode(v)) }}"
        );
    }
    let _ = writeln!(
        out,
        "\t\t\tthrow BamlDecodeError.typeMismatch(expected: \"{name}\", got: \"selected type not present in host union\")"
    );
    out.push_str("\t\t}\n");
    out.push_str("\t\tif let fqn = v.wireClassFQN() {\n");
    for (i, arm) in arms.iter().enumerate() {
        for member in &arm.members {
            if let Ty::Class(class_name, _) = member {
                let _ = writeln!(
                    out,
                    "\t\t\tif fqn == \"{class_name}\" {{ return .t{i}(try {}._bamlDecode(v)) }}",
                    arm.swift
                );
            }
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
        Ty::Class(name, _) | Ty::Enum(name) | Ty::TypeAlias(name) => {
            vec![format!("`{}`", name.name())]
        }
        Ty::Union(members) => members.iter().flat_map(thrown_leaf_names).collect(),
        _ => Vec::new(),
    }
}

/// Closure type + erased-wrapper prelude for one host-callable
/// parameter. The closure is `async throws` uniformly (sync closures
/// coerce), and the wrapper maps the engine's supplied-args payload
/// onto the closure's positional/optional parameters.
///
/// The wrapper states the declared function type beside the callable's
/// handle, spelled from the declaration. Every position is exact: the
/// return and throws types are covariant, so a wider statement there (an
/// `unknown` return under `() -> int`) is refused, and a statement that
/// only narrows a parameter would refuse arguments the declaration admits.
///
/// The prelude declares `local` (the wrapper) and `{local}_returns` (its
/// return slot); the caller picks a `local` that no other local or
/// parameter of the function can coincide with.
fn render_callable_param(
    name: &str,
    local: &str,
    cparams: &[baml_sdkgen_types::CallableParam],
    ret: &Ty,
    throws: &Ty,
    ctx: &TranslateCtx,
) -> Option<(String, String)> {
    let mut sig_parts: Vec<String> = Vec::new();
    let mut invoke_args: Vec<String> = Vec::new();
    let mut param_descriptors: Vec<String> = Vec::new();
    let mut positional = 0usize;
    for cp in cparams {
        let declared = slot_expr(&cp.ty, ctx, &generic_param_slot)?;
        match cp.mode {
            baml_sdkgen_types::CodegenFunctionParamMode::Required => {
                param_descriptors.push(format!(
                    "BamlFunctionParamDescriptor(type: ({declared}).type)"
                ));
                sig_parts.push(translate_ty(&cp.ty, ctx)?);
                invoke_args.push(format!("try _args.required({positional})"));
                positional += 1;
            }
            baml_sdkgen_types::CodegenFunctionParamMode::Optional => {
                let inner = translate_optional_arg_inner(&cp.ty, ctx)?;
                let arg_name = cp.name.as_ref()?.as_str();
                param_descriptors.push(format!(
                    "BamlFunctionParamDescriptor(name: \"{arg_name}\", type: ({declared}).type, optional: true)"
                ));
                sig_parts.push(format!("BamlOptional<{inner}>"));
                invoke_args.push(format!("try _args.optional(\"{arg_name}\")"));
            }
        }
    }
    let invoke = invoke_args.join(", ");
    let returns_local = format!("{local}_returns");
    // (closure result type, stated return type, prelude, wrapper body)
    let (ret_ty, returns, prelude, wrapper_body) = match ret {
        // `never` returns no value: a closure that can only throw (or trap).
        Ty::Never => (
            "Swift.Never".to_string(),
            ".never".to_string(),
            String::new(),
            format!("try await {name}({invoke})"),
        ),
        ret if ret.is_unit() => (
            "Swift.Void".to_string(),
            ".null".to_string(),
            String::new(),
            format!("try await {name}({invoke})\n\t\treturn try BamlNull()._bamlEncode()"),
        ),
        other => (
            translate_ty(other, ctx)?,
            format!("{returns_local}.type"),
            format!(
                "\tlet {returns_local} = {}\n",
                slot_expr(other, ctx, &generic_param_slot)?
            ),
            format!("try await {name}({invoke})._bamlEncode(as: {returns_local})"),
        ),
    };
    let closure_ty = format!(
        "@escaping @Sendable ({}) async throws -> {ret_ty}",
        sig_parts.join(", ")
    );
    // A throws clause the declaration left open is an implicit effect
    // parameter: the host states it `unknown`, as a host callable may
    // throw anything.
    let throws_descriptor = match throws {
        Ty::TypeVar(param) if is_synthetic_effect_param(param.name()) => ".unknown".to_string(),
        Ty::Never => ".never".to_string(),
        other => format!("({}).type", slot_expr(other, ctx, &generic_param_slot)?),
    };
    let function_type = format!(
        "BamlTypeDescriptor.function(params: [{}], returns: {returns}, throws: {throws_descriptor})",
        param_descriptors.join(", ")
    );
    let wrapper = format!(
        "{prelude}\tlet {local} = BamlHostCallable(functionType: {function_type}) {{ _args in\n\t\t{wrapper_body}\n\t}}\n"
    );
    Some((closure_ty, wrapper))
}

fn render_returned_callable(
    params: &[baml_sdkgen_types::CallableParam],
    ret: &Ty,
    ctx: &TranslateCtx,
    name: &str,
) -> Option<(String, String, String)> {
    let mut signature = Vec::new();
    let mut required = Vec::new();
    let mut optional = Vec::new();
    for (index, param) in params.iter().enumerate() {
        let local = format!("_arg{index}");
        let wire_name = param.name.as_ref()?.as_str();
        match param.mode {
            baml_sdkgen_types::CodegenFunctionParamMode::Required => {
                signature.push(format!("_ {local}: {}", translate_ty(&param.ty, ctx)?));
                required.push(format!(
                    "BamlArgument(\"{wire_name}\", {local}, as: {})",
                    slot_expr(&param.ty, ctx, &generic_param_slot)?
                ));
            }
            baml_sdkgen_types::CodegenFunctionParamMode::Optional => {
                signature.push(format!(
                    "_ {local}: BamlOptional<{}> = .unset",
                    translate_optional_arg_inner(&param.ty, ctx)?
                ));
                optional.push(format!(
                    "        {local}._appendIfSet(\"{wire_name}\", as: {}, to: &args)\n",
                    optional_arg_inner_slot(&param.ty, ctx, &generic_param_slot)?
                ));
            }
        }
    }
    signature.push("baml: Baml.BamlOptions = .init()".into());
    // A unit or `never` result carries nothing back to the host.
    let returns_nothing = ret.is_unit() || matches!(ret, Ty::Never);
    let ret_ty = if returns_nothing {
        "Swift.Void".into()
    } else {
        translate_ty(ret, ctx)?
    };
    let declarations = signature.join(", ");
    let setup = format!(
        "        {} args: [BamlArgument] = [{}]\n{}",
        if optional.is_empty() { "let" } else { "var" },
        required.join(", "),
        optional.join("")
    );
    let decode = if returns_nothing {
        "        _ = result; return ()".into()
    } else {
        format!("        return try {ret_ty}._bamlDecode(result)")
    };
    let definition = format!(
        "public struct {name}: Sendable {{\n    private let function: BamlFunctionHandle\n    fileprivate init(_ function: BamlFunctionHandle) {{ self.function = function }}\n    public func callAsFunction({declarations}) async throws -> {ret_ty} {{\n{setup}        let result = try await function.callRaw(args: args, baml: try baml.lowered())\n{decode}\n    }}\n    public func callSync({declarations}) throws -> {ret_ty} {{\n{setup}        let result = try function.callRawSync(args: args, baml: try baml.lowered())\n{decode}\n    }}\n}}\n"
    );
    Some((
        name.into(),
        format!("\treturn {name}(_function)"),
        definition,
    ))
}

#[cfg(test)]
mod shared_traversal_tests {
    use super::*;

    #[test]
    fn type_var_mentions_traverse_containers_and_callable_signatures() {
        let variable = || Ty::TypeVar(baml_sdkgen_types::ParamTy::new(0, "T".into()));
        assert!(mentions_type_var(&Ty::List(Box::new(variable())), "T"));
        assert!(!mentions_type_var(&Ty::List(Box::new(variable())), "U"));
        let callable = Ty::Function {
            params: Box::new([]),
            ret: Box::new(variable()),
            throws: Box::new(Ty::Never),
        };
        assert!(mentions_type_var(&callable, "T"));
        let throwing = Ty::Function {
            params: Box::new([]),
            ret: Box::new(Ty::Int),
            throws: Box::new(variable()),
        };
        assert!(mentions_type_var(&throwing, "T"));
        assert!(!swift_type_mentions(&throwing, "T"));
        assert!(swift_type_mentions(&callable, "T"));
        assert_eq!(returned_callable_type_var(&callable).as_deref(), Some("T"));
        assert_eq!(
            returned_callable_type_var(&Ty::List(Box::new(variable()))),
            None
        );
    }
}
