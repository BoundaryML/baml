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
        TranslateCtx, arm_slot, generic_param_slot, normalize_union, optional_arg_inner_slot,
        own_slot, slot_expr, translate_optional_arg_inner, translate_ty, union_arms,
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
/// its `_bamlEncode` and `_bamlDecode(_:as:)`.
pub(crate) fn class_argument_local(index: usize) -> String {
    format!("_bamlArgument{index}")
}

/// The declarations of the [`class_argument_local`]s some field's slot
/// reads, each the class slot's argument at its position.
fn class_argument_locals(class: &Class, fields: &[RenderedField]) -> String {
    let mut locals = String::new();
    for (i, p) in class.generic_params.iter().enumerate() {
        let local = class_argument_local(i);
        // A local is read where it is followed by anything but a digit
        // (`_bamlArgument1` is not read by `_bamlArgument10`).
        let read = fields.iter().any(|field| {
            field.slot.match_indices(&local).any(|(at, _)| {
                !field.slot[at + local.len()..].starts_with(|c: char| c.is_ascii_digit())
            })
        });
        if read {
            let _ = writeln!(
                locals,
                "\t\tlet {local}: BamlSlot<{}> = slot._classArgument({i})",
                escape_ident(p.as_str())
            );
        }
    }
    locals
}

/// The Swift struct of a class crossing by its fields, or, given `media`,
/// of the class crossing as that media together with its one field, its
/// `$rust_type` payload.
pub(crate) fn render_class(
    class: &Class,
    key: &Name,
    fields: &[RenderedField],
    methods: &[String],
    media: Option<(baml_base::MediaKind, &RenderedField)>,
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
    if let Some((kind, data)) = media {
        let data = format!("self.{}", data.name);
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
             \tpublic nonisolated func _bamlEncode(as _: BamlSlot<Self>) throws -> BamlInboundValue {{\n\
             \t\ttry .baml_media({data})\n\
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
        let body = class_argument_locals(class, fields);
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

    // Decode: field-dict walk; missing fields decode as null. Each field
    // decodes as a node of its declared type, the class's type arguments
    // taken from the slot the instance fills, as on encode.
    let decode_args = fields
        .iter()
        .map(|f| {
            format!(
                "\t\t\t{}: try fields._baml(\"{}\", as: {})",
                f.name,
                f.name.trim_matches('`'),
                f.slot
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
        // A media wrapper's slot is the media type, a leaf with no class
        // arguments.
        let arguments = if media.is_none() {
            class_argument_locals(class, fields)
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\n\tpublic nonisolated static func _bamlDecode(_ v: BamlOutboundValue) throws -> {name} {{\n\
             \t\ttry _bamlDecode(v, as: _bamlSlot)\n\
             \t}}\n\n\
             \tpublic nonisolated static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> {name} {{\n\
             {media_fallback}{arguments}\t\tlet fields = try v.classFields()\n\
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
pub(crate) fn render_opaque_class(
    class: &Class,
    key: &Name,
    methods: &[String],
) -> Result<String, String> {
    let fqn = key.to_string();
    // A generic opaque class would have to state its type arguments, which
    // the handle alone does not carry; no standard-library class is one.
    if !class.generic_params.is_empty() {
        return Err(
            "an opaque class cannot be generic: its handle carries no type arguments".into(),
        );
    }
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
         \tpublic nonisolated func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {{\n\
         \t\ttry _bamlHandle._bamlEncode(as: ._leaf(slot.type))\n\
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
    Ok(out)
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
    // `name` is a parameter's Swift spelling, its label and wire name;
    // `binding` the name the body reads it by (see [`parameter_binding`]).
    enum Param {
        Required {
            name: String,
            binding: String,
            ty: String,
            slot: String,
        },
        Optional {
            name: String,
            binding: String,
            inner: String,
            inner_slot: String,
        },
        /// A host callable: `ty` is the closure type; `wrapper` is a
        /// body-prelude statement building the erased `BamlHostCallable`
        /// into the local `local`.
        Callable {
            name: String,
            binding: String,
            ty: String,
            local: String,
            wrapper: String,
        },
    }

    // The caller allocates a collision-safe Swift name for this scope.
    // The wire FQN keeps `@` verbatim; an `@stream` companion remains an
    // ordinary function returning `ai.stream.Stream<T>` (→ BamlStream).
    let bare = binding_name;

    // A returned closure's Swift type is declared beside the function, outside
    // the scope of any generic parameter, so it cannot mention one.
    if closure_type_var(&function.return_type).is_some() {
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
    // The struct of each BAML closure the host receives (a returned one, or one
    // passed to a callback), declared beside the function.
    let mut closure_types = String::new();
    let mut params = Vec::new();
    for (position, arg) in function.arguments.iter().enumerate() {
        let name = escape_ident(arg.name.as_str());
        let binding = parameter_binding(arg.name.as_str());
        if arg.default.is_some() {
            params.push(Param::Optional {
                name,
                binding,
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
            let (ty, wrapper, received_closures) = render_callable_param(
                &binding,
                &local,
                &format!("{fqn}({position})"),
                cparams,
                ret,
                throws,
                ctx,
            )?;
            closure_types.push_str(&received_closures);
            params.push(Param::Callable {
                name,
                binding,
                ty,
                local,
                wrapper,
            });
        } else {
            params.push(Param::Required {
                name,
                binding,
                ty: translate_ty(&arg.ty, ctx)?,
                slot: slot_expr(&arg.ty, ctx, &generic_param_slot)?,
            });
        }
    }

    // The result's Swift type and the slot it decodes under: its declared
    // type, which a Swift type can be coarser than.
    let ret = match &function.return_type {
        // `never` (diverging: panic/exit) spells as a void function —
        // the call only ever returns by throwing (Python maps both
        // void and never to `None` the same way).
        Ty::Never => None,
        ret if ret.is_unit() => None,
        Ty::Function {
            params: callable_params,
            ret,
            ..
        } => {
            let name = closure_type_name(fqn);
            closure_types.push_str(&render_closure_type(callable_params, ret, ctx, &name)?);
            let slot = format!(
                "BamlSlot<{name}>._callable({})",
                stated_type(&function.return_type, ctx)?
            );
            Some((name, slot))
        }
        other => Some((
            translate_ty(other, ctx)?,
            slot_expr(other, ctx, &generic_param_slot)?,
        )),
    };

    let application_param_list = params
        .iter()
        .map(|p| match p {
            Param::Required {
                name, binding, ty, ..
            }
            | Param::Callable {
                name, binding, ty, ..
            } => format!("{}: {ty}", parameter_declaration(name, binding)),
            Param::Optional {
                name,
                binding,
                inner,
                ..
            } => format!(
                "{}: BamlOptional<{inner}> = .unset",
                parameter_declaration(name, binding)
            ),
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
        Param::Required {
            name,
            binding,
            slot,
            ..
        } => Some(format!(
            "BamlArgument(\"{}\", {binding}, as: {slot})",
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
                name,
                binding,
                inner_slot,
                ..
            } = p
            {
                let _ = writeln!(
                    args_setup,
                    "\t{binding}._appendIfSet(\"{}\", as: {inner_slot}, to: &{args_name})",
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

    let mut out = closure_types;
    let static_kw = if matches!(kind, FnKind::Instance { .. }) {
        ""
    } else {
        "static "
    };
    if fqn == "trace.current_context" {
        let (ret_ty, _) = ret.as_ref()?;
        return Some(format!(
            "{doc}public {static_kw}func {fn_name}({param_list}) throws -> {ret_ty} {{ _ = Baml._initialized; return try BamlRuntime.shared.currentTraceContext() }}\n\n{doc}public {static_kw}func {async_name}({param_list}) async throws -> {ret_ty} {{ _ = Baml._initialized; return try BamlRuntime.shared.currentTraceContext() }}\n"
        ));
    }
    match &ret {
        Some((ret_ty, ret_slot)) => {
            let _ = write!(
                out,
                "{doc}public {static_kw}func {fn_name}{generic_sig}({param_list}) throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\treturn try BamlRuntime.shared.callSync(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered(), returns: {ret_slot})\n\
                 }}\n\n\
                 {doc}public {static_kw}func {async_name}{generic_sig}({param_list}) async throws -> {ret_ty} {{\n\
                 \t_ = Baml._initialized\n\
                 {args_setup}\treturn try await BamlRuntime.shared.call(\"{fqn}\", args: {args_expr}{type_args_arg}, baml: try {control_name}.lowered(), returns: {ret_slot})\n\
                 }}\n"
            );
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

/// The name a generated body reads the parameter `raw` by. The body names
/// the bridge's types (`BamlRuntime`, `BamlSlot`, …), the generated root
/// `Baml`, the `Swift` module and its own `_baml` locals unqualified, so a
/// parameter spelled like one of them is read by a name of its own; its
/// label, which callers write, stays its BAML name.
fn parameter_binding(raw: &str) -> String {
    let shadows = raw.starts_with("Baml")
        || raw.starts_with("_baml")
        || matches!(raw, "Swift" | "Foundation");
    if shadows {
        format!("_baml_arg_{raw}")
    } else {
        escape_ident(raw)
    }
}

/// A parameter's declaration: its label, and its binding where that differs.
fn parameter_declaration(name: &str, binding: &str) -> String {
    if name == binding {
        name.to_string()
    } else {
        format!("{name} {binding}")
    }
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

/// The first generic parameter the Swift type of a BAML closure of type
/// `closure` (its parameters and result) mentions, or `None` when `closure`
/// is not a callable or mentions none. The throws type is not part of the
/// Swift type, and a synthetic effect parameter is no Swift generic parameter.
pub(crate) fn closure_type_var(closure: &Ty) -> Option<String> {
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
    let Ty::Function { params, ret, .. } = closure else {
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
    // A case is the declared arm the engine selected, which the arm's Swift
    // type alone cannot tell (`"a"[] | "b"[]`); it decodes under that arm.
    for (i, (ty, slot)) in arm_tys.iter().zip(&arm_slots).enumerate() {
        let _ = writeln!(
            out,
            "\t\t\tif ({slot})._bamlSelects(selected) {{ return .t{i}(try {ty}._bamlDecode(v, as: {slot})) }}"
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
                    "\t\t\tif fqn == \"{class_name}\" {{ return .t{i}(try {}._bamlDecode(v, as: {})) }}",
                    arm.swift, arm_slots[i]
                );
            }
        }
    }
    out.push_str("\t\t}\n");
    for (i, (ty, slot)) in arm_tys.iter().zip(&arm_slots).enumerate() {
        let _ = writeln!(
            out,
            "\t\tif let value = try? {ty}._bamlDecode(v, as: {slot}) {{ return .t{i}(value) }}"
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
/// A parameter of the callable that is itself a function receives a BAML
/// closure, which arrives as the struct [`render_closure_type`] declares for
/// it, named after `site` and the parameter's index. Those declarations are
/// the third element, for the caller to place beside the function.
///
/// The prelude declares `local` (the wrapper) and `{local}_returns` (its
/// return slot); the caller picks a `local` that no other local or
/// parameter of the function can coincide with.
pub(crate) fn render_callable_param(
    name: &str,
    local: &str,
    site: &str,
    cparams: &[baml_sdkgen_types::CallableParam],
    ret: &Ty,
    throws: &Ty,
    ctx: &TranslateCtx,
) -> Option<(String, String, String)> {
    let mut sig_parts: Vec<String> = Vec::new();
    let mut invoke_args: Vec<String> = Vec::new();
    let mut received_closures = String::new();
    let mut positional = 0usize;
    for (index, cp) in cparams.iter().enumerate() {
        match cp.mode {
            baml_sdkgen_types::CodegenFunctionParamMode::Required => {
                let (swift, slot) = match &cp.ty {
                    Ty::Function { params, ret, .. } => {
                        // Declared beside the function, outside any generic
                        // parameter's scope, like a returned closure.
                        if closure_type_var(&cp.ty).is_some() {
                            return None;
                        }
                        let closure = closure_type_name(&format!("{site}.{index}"));
                        received_closures
                            .push_str(&render_closure_type(params, ret, ctx, &closure)?);
                        let slot = format!(
                            "BamlSlot<{closure}>._callable({})",
                            stated_type(&cp.ty, ctx)?
                        );
                        (closure, slot)
                    }
                    other => (
                        translate_ty(other, ctx)?,
                        slot_expr(other, ctx, &generic_param_slot)?,
                    ),
                };
                sig_parts.push(swift);
                invoke_args.push(format!("try _bamlArgs.required({positional}, as: {slot})"));
                positional += 1;
            }
            baml_sdkgen_types::CodegenFunctionParamMode::Optional => {
                let inner = translate_optional_arg_inner(&cp.ty, ctx)?;
                let inner_slot = optional_arg_inner_slot(&cp.ty, ctx, &generic_param_slot)?;
                let arg_name = cp.name.as_ref()?.as_str();
                sig_parts.push(format!("BamlOptional<{inner}>"));
                invoke_args.push(format!(
                    "try _bamlArgs.optional(\"{arg_name}\", as: {inner_slot})"
                ));
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
            format!(
                "try await {name}({invoke})\n\t\treturn try _bamlArgs.result(BamlNull(), as: BamlSlot<BamlNull>.own)"
            ),
        ),
        other => (
            translate_ty(other, ctx)?,
            format!("{returns_local}.type"),
            format!(
                "\tlet {returns_local} = {}\n",
                slot_expr(other, ctx, &generic_param_slot)?
            ),
            format!("return try _bamlArgs.result(try await {name}({invoke}), as: {returns_local})"),
        ),
    };
    let closure_ty = format!(
        "@escaping @Sendable ({}) async throws -> {ret_ty}",
        sig_parts.join(", ")
    );
    let function_type = function_type_descriptor(cparams, &returns, throws, ctx)?;
    let thrown = thrown_slots(throws, ctx)?;
    let wrapper = format!(
        "{prelude}\tlet {local} = BamlHostCallable(functionType: {function_type}, thrown: [{thrown}]) {{ _bamlArgs in\n\t\t{wrapper_body}\n\t}}\n"
    );
    Some((closure_ty, wrapper, received_closures))
}

/// The `BamlThrownSlot`s a host callable's thrown value is encoded under,
/// spelled from its declared throws type `throws`: the whole type first,
/// then each case of a union. None for an open throws type (a value states
/// its own type under `unknown`) or `never`.
fn thrown_slots(throws: &Ty, ctx: &TranslateCtx) -> Option<String> {
    let slots = match throws {
        Ty::TypeVar(param) if is_synthetic_effect_param(param.name()) => Vec::new(),
        Ty::Never => Vec::new(),
        Ty::Union(members) => {
            let (non_null, _) = normalize_union(members);
            let mut slots = vec![slot_expr(throws, ctx, &generic_param_slot)?];
            for arm in union_arms(&non_null, ctx)? {
                slots.push(arm_slot(&arm, ctx, &generic_param_slot)?);
            }
            slots
        }
        other => vec![slot_expr(other, ctx, &generic_param_slot)?],
    };
    Some(
        slots
            .iter()
            .map(|slot| format!("BamlThrownSlot({slot})"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// The `BamlTypeDescriptor` expression stating a function type with these
/// parameters and throws type, its return type stated by the expression
/// `returns`. A parameter is stated with its declared name: the engine binds
/// a call of a BAML closure by the names its stated type gives.
fn function_type_descriptor(
    cparams: &[baml_sdkgen_types::CallableParam],
    returns: &str,
    throws: &Ty,
    ctx: &TranslateCtx,
) -> Option<String> {
    let params = cparams
        .iter()
        .map(|cp| {
            let ty = stated_type(&cp.ty, ctx)?;
            let name = cp.name.as_deref();
            Some(match (cp.mode, name) {
                (baml_sdkgen_types::CodegenFunctionParamMode::Required, None) => {
                    format!("BamlFunctionParamDescriptor(type: {ty})")
                }
                (baml_sdkgen_types::CodegenFunctionParamMode::Required, Some(name)) => {
                    format!("BamlFunctionParamDescriptor(name: \"{name}\", type: {ty})")
                }
                // An optional parameter is passed by name, so it always has one.
                (baml_sdkgen_types::CodegenFunctionParamMode::Optional, None) => return None,
                (baml_sdkgen_types::CodegenFunctionParamMode::Optional, Some(name)) => format!(
                    "BamlFunctionParamDescriptor(name: \"{name}\", type: {ty}, optional: true)"
                ),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    // A throws clause the declaration left open is an implicit effect
    // parameter: the host states it `unknown`, as a host callable may
    // throw anything.
    let throws = match throws {
        Ty::TypeVar(param) if is_synthetic_effect_param(param.name()) => ".unknown".to_string(),
        Ty::Never => ".never".to_string(),
        other => stated_type(other, ctx)?,
    };
    Some(format!(
        "BamlTypeDescriptor.function(params: [{}], returns: {returns}, throws: {throws})",
        params.join(", ")
    ))
}

/// The `BamlTypeDescriptor` expression stating the declared type `ty`: a
/// function type's own, or the type its slot states.
pub(crate) fn stated_type(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    match ty {
        Ty::Function {
            params,
            ret,
            throws,
        } => {
            let returns = match &**ret {
                Ty::Never => ".never".to_string(),
                ret if ret.is_unit() => ".null".to_string(),
                other => stated_type(other, ctx)?,
            };
            function_type_descriptor(params, &returns, throws, ctx)
        }
        other => Some(format!(
            "({}).type",
            slot_expr(other, ctx, &generic_param_slot)?
        )),
    }
}

/// The Swift name of the struct for the BAML closures at `site`: a function's
/// FQN, extended with a path for a position inside its signature. The struct
/// is declared beside the function, so `site` keeps the name unique.
pub(crate) fn closure_type_name(site: &str) -> String {
    // FNV-1a: fixed by its definition, so a name is the same whatever
    // toolchain generates it.
    let hash = site.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("_BamlClosure_{hash:016x}")
}

/// The declaration of `name`, the struct a BAML closure with these
/// parameters and result arrives as in Swift, wherever the host receives one:
/// a function's result or a callback's argument. It decodes from the
/// closure's handle and calls back into BAML.
pub(crate) fn render_closure_type(
    params: &[baml_sdkgen_types::CallableParam],
    ret: &Ty,
    ctx: &TranslateCtx,
    name: &str,
) -> Option<String> {
    let mut signature = Vec::new();
    let mut required = Vec::new();
    let mut optional = Vec::new();
    for (index, param) in params.iter().enumerate() {
        let local = format!("_arg{index}");
        // A parameter the closure's type leaves nameless is bound by the
        // name the engine gives it, its position's.
        let wire_name = param
            .name
            .as_ref()
            .map_or_else(|| format!("arg{index}"), |name| name.as_str().to_string());
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
        format!(
            "        return try {ret_ty}._bamlDecode(result, as: {})",
            slot_expr(ret, ctx, &generic_param_slot)?
        )
    };
    Some(format!(
        "public struct {name}: Sendable, BamlReceivedCallable {{\n    private let function: BamlFunctionHandle\n    private init(_ function: BamlFunctionHandle) {{ self.function = function }}\n    public static func _bamlDecode(_ value: BamlOutboundValue) throws -> Self {{\n        Self(try BamlFunctionHandle.decode(value))\n    }}\n    public func callAsFunction({declarations}) async throws -> {ret_ty} {{\n{setup}        let result = try await function.callRaw(args: args, baml: try baml.lowered())\n{decode}\n    }}\n    public func callSync({declarations}) throws -> {ret_ty} {{\n{setup}        let result = try function.callRawSync(args: args, baml: try baml.lowered())\n{decode}\n    }}\n}}\n\n"
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
        assert_eq!(closure_type_var(&callable).as_deref(), Some("T"));
        assert_eq!(closure_type_var(&Ty::List(Box::new(variable()))), None);
    }
}
