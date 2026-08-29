//! Pure `Ty` -> Python type-expression translation for the phase-G3
//! emitter rewrite.
//!
//! Rule sources:
//! - `.humanlayer/tasks/clientpython/09b-codegen-rules.md` §6, §9
//! - `.humanlayer/tasks/clientpython/11e-phaseg3-ty-translator.md`

use std::collections::BTreeMap;

use baml_base::{Literal, MediaKind, qualified_name::AI_STREAM_STREAM};
use baml_codegen_types::{Name, Ty};
use indexmap::IndexMap;

use crate::{
    names::PythonNames,
    py_string,
    routing::{LeafPath, route_class_ref},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranslateCtx {
    pub(crate) current_leaf: LeafPath,
    pub(crate) self_ref: Option<SelfRef>,
    /// 18c: set when emitting the body of a recursive
    /// `typing_extensions.TypeAliasType` alias. The RHS of a
    /// `TypeAliasType(...)` call evaluates eagerly at module load, so
    /// every named reference in the body — same-leaf, cross-leaf,
    /// root-routed — must be emitted as a string forward-ref to avoid
    /// `NameError`. Same-leaf hoisting (recursive aliases land above
    /// the rest of the leaf) and `TYPE_CHECKING`-guarded cross-leaf
    /// imports both leave the names absent at line-eval time; the
    /// quoted form defers resolution until pydantic walks the alias
    /// later.
    pub(crate) defer_name_refs: bool,
    /// `.pyi`-only: maps each optional-argument `Ty::Function` in the leaf to
    /// the name of the `typing.Protocol` emitted for it. A function *type*
    /// (`typing.Callable[[…], R]`) cannot express per-parameter optionality,
    /// so a callback with optional params is rendered as a named Protocol
    /// whose `__call__` carries the precise signature; the type expression for
    /// such a callable is just that Protocol's name. Absent (`None`) in the
    /// runtime `.py` path, where callable types fall back to
    /// `typing.Callable[..., R]` (Protocol classes are stub-only).
    pub(crate) callback_protocols: Option<std::rc::Rc<IndexMap<Ty, String>>>,
    /// Rewrite source `ai.stream.Stream<T, F>` to the underlying host
    /// `_BamlStream` type. `Stream` is a host re-export rather than a normal
    /// generated class, so retaining the source spelling in annotations is
    /// not valid Python codegen. Its synthesized `Stream$stream` companion is
    /// deliberately excluded: that is a real Pydantic partial-state class.
    pub(crate) type_stream_accessors: bool,
    /// Stub-only: include the generated terminal marker in the raw-next stream
    /// type argument so `next()` is typed as `T | Done`. Runtime annotations
    /// keep only `T`; the `.pyi` is the authoritative public typing surface.
    pub(crate) include_stream_done: bool,
    /// Shared declaration/module projection. Translator-only unit tests may
    /// leave this unset to exercise the identity fallback.
    pub(crate) names: Option<std::rc::Rc<PythonNames>>,
    /// Raw `TypeVar` spelling -> projected Python spelling in this scope.
    pub(crate) type_var_names: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfRef {
    pub(crate) routed_leaf: LeafPath,
    pub(crate) bare_name: String,
}

pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> String {
    match ty {
        Ty::Int { .. } => "int".to_string(),
        Ty::Bigint { .. } => "int".to_string(),
        Ty::Float { .. } => "float".to_string(),
        Ty::String { .. } => "str".to_string(),
        Ty::Bool { .. } => "bool".to_string(),
        Ty::Null { .. } => "None".to_string(),
        Ty::Literal(Literal::Int(value), ..) => format!("typing.Literal[{value}]"),
        Ty::Literal(Literal::Bigint(value), ..) => format!("typing.Literal[{value}]"),
        Ty::Literal(Literal::String(value), ..) => {
            format!("typing.Literal[{}]", py_string(value))
        }
        Ty::Literal(Literal::Bool(true), ..) => "typing.Literal[True]".to_string(),
        Ty::Literal(Literal::Bool(false), ..) => "typing.Literal[False]".to_string(),
        // Python does not allow float parameters to typing.Literal.
        Ty::Literal(Literal::Float(_), ..) => "typing.Any".to_string(),
        Ty::Uint8Array { .. } => "bytes".to_string(),
        Ty::Media(MediaKind::Image, _) => media_ref("Image", ctx),
        Ty::Media(MediaKind::Audio, _) => media_ref("Audio", ctx),
        Ty::Media(MediaKind::Video, _) => media_ref("Video", ctx),
        Ty::Media(MediaKind::Pdf, _) => media_ref("Pdf", ctx),
        Ty::Media(MediaKind::Generic, _) => "typing.Any".to_string(),
        Ty::Class(name, args, _) => {
            if ctx.type_stream_accessors
                && is_ai_stream_type(name)
                && let [stream, final_value] = args.as_slice()
            {
                let stream_type = translate_ty(stream, ctx);
                let yield_type = translate_stream_yield_ty(stream, ctx);
                let next_type = if ctx.include_stream_done {
                    let done = if ctx.current_leaf.segments == ["ai", "stream"] {
                        "Done"
                    } else {
                        "_BamlStreamDone"
                    };
                    format!("typing.Union[{stream_type}, {done}]")
                } else {
                    stream_type
                };
                return format!(
                    "_BamlStream[{}, {}, {}]",
                    next_type,
                    yield_type,
                    translate_ty(final_value, ctx),
                );
            }
            let arg_strs: Vec<String> = args.iter().map(|a| translate_ty(a, ctx)).collect();
            render_name_ref_or_self_ref(name, ctx, &arg_strs.join(", "))
        }
        Ty::TypeAlias(name, _) => render_name_ref_or_self_ref(name, ctx, ""),
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => {
            let head = render_name_ref(name, ctx);
            if should_defer_name_ref(ctx) {
                py_string(&head)
            } else {
                head
            }
        }
        Ty::TypeVar(name, _) => ctx
            .type_var_names
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| name.as_str().to_string()),
        Ty::List(inner, _) => format!("typing.List[{}]", translate_ty(inner, ctx)),
        Ty::Map { key, value, .. } => {
            format!(
                "typing.Dict[{}, {}]",
                translate_ty(key, ctx),
                translate_ty(value, ctx)
            )
        }
        Ty::Union(items, _) => {
            // `T | null` (a single non-null member plus null) is optionality —
            // emit idiomatic `typing.Optional[T]`. Multi-member nullable unions
            // fall through to `typing.Union[A, B, None]` (Null → "None").
            let non_null: Vec<&Ty> = items
                .iter()
                .filter(|t| !matches!(t, Ty::Null { .. }))
                .collect();
            if non_null.len() == 1 && non_null.len() < items.len() {
                format!("typing.Optional[{}]", translate_ty(non_null[0], ctx))
            } else {
                format!(
                    "typing.Union[{}]",
                    items
                        .iter()
                        .map(|item| translate_ty(item, ctx))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        Ty::Unknown { .. }
        | Ty::Interface(..)
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Future(..) => "typing.Any".to_string(),
        Ty::Function { params, ret, .. } => {
            let has_optional = params
                .iter()
                .any(|param| param.mode == baml_codegen_types::CodegenFunctionParamMode::Optional);
            if has_optional {
                // A callback with optional params is emitted as a named
                // `typing.Protocol` (see `TranslateCtx::callback_protocols`):
                // the type expression here is just that Protocol's name.
                if let Some(name) = ctx.callback_protocols.as_ref().and_then(|map| map.get(ty)) {
                    return name.clone();
                }
                // Runtime `.py` path (no Protocol map): `typing.Callable` can't
                // express per-param optionality, so widen the arg list.
                format!("typing.Callable[..., {}]", translate_ty(ret, ctx))
            } else {
                format!(
                    "typing.Callable[[{}], {}]",
                    params
                        .iter()
                        .map(|param| translate_ty(&param.ty, ctx))
                        .collect::<Vec<_>>()
                        .join(", "),
                    translate_ty(ret, ctx)
                )
            }
        }
        Ty::Void { .. } | Ty::Never { .. } => "None".to_string(),
        // `$rust_type` fields in stdlib stubs (Response._body, SseStream._handle, …).
        // The host-language opaque-handle wrapper is `BamlPyHandle` from the
        // bridge runtime, imported as `_BamlPyHandle` to keep `baml` (the
        // local relative module) from shadowing it. The single-underscore
        // field name still triggers Pydantic v2's private-attribute handling
        // regardless of the annotation; `_decode_class` injects the value
        // into `__pydantic_private__` post-construction.
        Ty::RustType { .. } => "_BamlPyHandle".to_string(),
    }
}

/// Async iteration consumes the raw `next()` protocol: `Done` terminates the
/// iterator and top-level null partials are skipped. Render the value that can
/// actually reach the loop body rather than reusing the broader next type.
fn translate_stream_yield_ty(ty: &Ty, ctx: &TranslateCtx) -> String {
    match ty {
        Ty::Null { .. } => "typing_extensions.Never".to_string(),
        Ty::Union(items, _) => {
            let non_null = items
                .iter()
                .filter(|item| !matches!(item, Ty::Null { .. }))
                .collect::<Vec<_>>();
            match non_null.as_slice() {
                [] => "typing_extensions.Never".to_string(),
                [only] => translate_ty(only, ctx),
                many => format!(
                    "typing.Union[{}]",
                    many.iter()
                        .map(|item| translate_ty(item, ctx))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        _ => translate_ty(ty, ctx),
    }
}

pub(crate) fn is_ai_stream_type(name: &Name) -> bool {
    name.to_string() == AI_STREAM_STREAM
}

fn render_name_ref_or_self_ref(name: &Name, ctx: &TranslateCtx, generic_args: &str) -> String {
    let head = render_name_ref(name, ctx);
    let full = if generic_args.is_empty() {
        head
    } else {
        format!("{head}[{generic_args}]")
    };
    if should_quote_self_ref(name, ctx) || should_defer_name_ref(ctx) {
        py_string(&full)
    } else {
        full
    }
}

fn should_quote_self_ref(name: &Name, ctx: &TranslateCtx) -> bool {
    match &ctx.self_ref {
        Some(self_ref) => {
            routed_leaf(name, ctx) == self_ref.routed_leaf
                && projected_bare_name(name, ctx) == self_ref.bare_name
        }
        None => false,
    }
}

/// 18c: in a recursive-alias body, every named reference is emitted as
/// a string forward-ref. The body is the RHS of a `TypeAliasType(...)`
/// call which evaluates eagerly at module load, but the names it can
/// touch are unavailable then:
///
/// - same-leaf names: the recursive alias is hoisted to the top of
///   the leaf, so the class/alias/enum being referenced may not yet
///   be defined when this line runs.
/// - cross-leaf and root-routed names: their imports live under
///   `if typing.TYPE_CHECKING:` (false at runtime), so the symbols
///   aren't present in the module's runtime globals.
///
/// Quoting them as forward-ref strings defers resolution to pydantic's
/// schema-build pass, which walks the alias lazily.
fn should_defer_name_ref(ctx: &TranslateCtx) -> bool {
    ctx.defer_name_refs
}

fn media_ref(bare: &str, ctx: &TranslateCtx) -> String {
    let name = Name::new(
        baml_base::Name::new("baml"),
        vec![baml_base::Name::new("media")],
        baml_base::Name::new(bare),
    );
    render_name_ref(&name, ctx)
}

fn render_name_ref(name: &Name, ctx: &TranslateCtx) -> String {
    let routed_leaf = routed_leaf(name, ctx);
    let bare_name = projected_bare_name(name, ctx);
    if routed_leaf == ctx.current_leaf || routed_leaf.segments.is_empty() {
        bare_name
    } else {
        format!("{}.{}", routed_leaf.segments.join("."), bare_name)
    }
}

fn routed_leaf(name: &Name, ctx: &TranslateCtx) -> LeafPath {
    ctx.names.as_ref().map_or_else(
        || route_class_ref(name),
        |names| names.route_class_ref(name),
    )
}

fn projected_bare_name(name: &Name, ctx: &TranslateCtx) -> String {
    ctx.names.as_ref().map_or_else(
        || name.bare_name().to_string(),
        |names| names.symbol(name).into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use pretty_assertions::assert_eq;

    use super::*;

    struct Case {
        label: &'static str,
        ty: Ty,
        ctx: TranslateCtx,
        expected: &'static str,
    }

    fn leaf(segments: &[&str]) -> LeafPath {
        LeafPath {
            segments: segments.iter().map(ToString::to_string).collect(),
        }
    }

    fn ctx(segments: &[&str]) -> TranslateCtx {
        TranslateCtx {
            current_leaf: leaf(segments),
            self_ref: None,
            defer_name_refs: false,
            callback_protocols: None,
            type_stream_accessors: false,
            include_stream_done: false,
            names: None,
            type_var_names: BTreeMap::new(),
        }
    }

    fn ctx_with_self(
        current_segments: &[&str],
        self_segments: &[&str],
        bare_name: &str,
    ) -> TranslateCtx {
        TranslateCtx {
            current_leaf: leaf(current_segments),
            defer_name_refs: false,
            callback_protocols: None,
            type_stream_accessors: false,
            include_stream_done: false,
            self_ref: Some(SelfRef {
                routed_leaf: leaf(self_segments),
                bare_name: bare_name.to_string(),
            }),
            names: None,
            type_var_names: BTreeMap::new(),
        }
    }

    /// Mirrors how `render_type_alias` builds the ctx for a recursive
    /// alias's RHS: `defer_name_refs` is on, `self_ref` is set, and
    /// every named leaf in the body becomes a string forward-ref.
    fn ctx_recursive_alias_body(
        current_segments: &[&str],
        self_segments: &[&str],
        bare_name: &str,
    ) -> TranslateCtx {
        TranslateCtx {
            current_leaf: leaf(current_segments),
            defer_name_refs: true,
            callback_protocols: None,
            type_stream_accessors: false,
            include_stream_done: false,
            self_ref: Some(SelfRef {
                routed_leaf: leaf(self_segments),
                bare_name: bare_name.to_string(),
            }),
            names: None,
            type_var_names: BTreeMap::new(),
        }
    }

    fn name(pkg: &str, namespace_path: &[&str], bare_name: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            namespace_path
                .iter()
                .map(|segment| BaseName::new(*segment))
                .collect(),
            BaseName::new(bare_name),
        )
    }
    fn class_ty(name: Name, args: Vec<Ty>) -> Ty {
        Ty::Class(name, args, baml_base::TyAttr::EMPTY)
    }
    fn enum_ty(name: Name) -> Ty {
        Ty::Enum(name, baml_base::TyAttr::EMPTY)
    }
    fn alias_ty(name: Name) -> Ty {
        Ty::TypeAlias(name, baml_base::TyAttr::EMPTY)
    }
    fn type_var(name: BaseName) -> Ty {
        Ty::TypeVar(
            baml_codegen_types::ParamTy::new(0, name),
            baml_base::TyAttr::EMPTY,
        )
    }
    fn list(inner: Box<Ty>) -> Ty {
        Ty::List(inner, baml_base::TyAttr::EMPTY)
    }
    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members, baml_base::TyAttr::EMPTY)
    }
    fn media(kind: MediaKind) -> Ty {
        Ty::Media(kind, baml_base::TyAttr::EMPTY)
    }
    fn literal(value: Literal) -> Ty {
        Ty::Literal(
            value,
            baml_codegen_types::Freshness::Regular,
            baml_base::TyAttr::EMPTY,
        )
    }
    fn baml_options() -> Ty {
        class_ty(name("baml", &[], "Options"), Vec::new())
    }
    fn callable(params: Vec<baml_codegen_types::CallableParam>, ret: Box<Ty>) -> Ty {
        Ty::Function {
            params,
            ret,
            throws: Box::new(Ty::Never {
                attr: baml_base::TyAttr::EMPTY,
            }),
            attr: baml_base::TyAttr::EMPTY,
        }
    }

    fn callable_param(ty: Ty) -> baml_codegen_types::CallableParam {
        baml_codegen_types::CallableParam {
            name: None,
            ty,
            mode: baml_codegen_types::CodegenFunctionParamMode::Required,
        }
    }

    fn optional_callable_param(name: &str, ty: Ty) -> baml_codegen_types::CallableParam {
        baml_codegen_types::CallableParam {
            name: Some(BaseName::new(name)),
            ty,
            mode: baml_codegen_types::CodegenFunctionParamMode::Optional,
        }
    }

    fn assert_ty(case: &Case) {
        check_exhaustive(&case.ty);
        assert_eq!(
            translate_ty(&case.ty, &case.ctx),
            case.expected,
            "mismatch for case {}",
            case.label
        );
    }

    fn check_exhaustive(ty: &Ty) {
        match ty {
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Literal(..)
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Class(..)
            | Ty::Interface(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::List(..)
            | Ty::Map { .. }
            | Ty::Union(..)
            | Ty::Unknown { .. }
            | Ty::Function { .. }
            | Ty::Future(..)
            | Ty::Void { .. }
            | Ty::Never { .. }
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. } => {}
        }
    }

    #[test]
    fn translate_ty_covers_phase_g3_matrix() {
        let cases = vec![
            Case {
                label: "int",
                ty: Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "int",
            },
            Case {
                label: "float",
                ty: Ty::Float {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "float",
            },
            Case {
                label: "string",
                ty: Ty::String {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "str",
            },
            Case {
                label: "bool",
                ty: Ty::Bool {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "bool",
            },
            Case {
                label: "null",
                ty: Ty::Null {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "None",
            },
            Case {
                label: "uint8array",
                ty: Ty::Uint8Array {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "bytes",
            },
            Case {
                label: "unknown",
                ty: Ty::Unknown {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Any",
            },
            Case {
                label: "unit",
                ty: Ty::Void {
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "None",
            },
            Case {
                label: "baml options",
                ty: baml_options(),
                ctx: ctx(&["lorem"]),
                expected: "baml.Options",
            },
            Case {
                label: "literal int",
                ty: literal(Literal::Int(42)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[42]",
            },
            Case {
                label: "literal negative int",
                ty: literal(Literal::Int(-1)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[-1]",
            },
            Case {
                label: "literal string",
                ty: literal(Literal::String("draft".to_string())),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[\"draft\"]",
            },
            Case {
                label: "literal escaped string",
                ty: literal(Literal::String("has \"quotes\"".to_string())),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[\"has \\\"quotes\\\"\"]",
            },
            Case {
                label: "literal bool true",
                ty: literal(Literal::Bool(true)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[True]",
            },
            Case {
                label: "literal bool false",
                ty: literal(Literal::Bool(false)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[False]",
            },
            Case {
                label: "literal float fallback",
                ty: literal(Literal::Float("3.14".to_string())),
                ctx: ctx(&["lorem"]),
                expected: "typing.Any",
            },
            Case {
                label: "media image",
                ty: media(MediaKind::Image),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Image",
            },
            Case {
                label: "media audio",
                ty: media(MediaKind::Audio),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Audio",
            },
            Case {
                label: "media video",
                ty: media(MediaKind::Video),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Video",
            },
            Case {
                label: "media pdf",
                ty: media(MediaKind::Pdf),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Pdf",
            },
            Case {
                label: "media generic fallback",
                ty: media(MediaKind::Generic),
                ctx: ctx(&["lorem"]),
                expected: "typing.Any",
            },
            Case {
                label: "class same leaf root namespace",
                ty: class_ty(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "Resume",
            },
            Case {
                label: "class cross leaf root namespace",
                ty: class_ty(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["ipsum"]),
                expected: "lorem.Resume",
            },
            Case {
                label: "class same leaf root init",
                ty: class_ty(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&[]),
                expected: "Foo",
            },
            Case {
                label: "class root init from namespaced leaf",
                ty: class_ty(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "Foo",
            },
            Case {
                label: "class vendor cross leaf",
                ty: class_ty(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "vendor.aws.s3.Bucket",
            },
            Case {
                label: "class vendor same leaf",
                ty: class_ty(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["vendor", "aws", "s3"]),
                expected: "Bucket",
            },
            Case {
                label: "class vendor other vendor leaf",
                ty: class_ty(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["vendor", "aws", "ec2"]),
                expected: "vendor.aws.s3.Bucket",
            },
            Case {
                label: "class stdlib cross leaf",
                ty: class_ty(name("baml", &["http"], "Response"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "baml.http.Response",
            },
            Case {
                label: "class stdlib same leaf",
                ty: class_ty(name("baml", &["http"], "Response"), vec![]),
                ctx: ctx(&["baml", "http"]),
                expected: "Response",
            },
            Case {
                label: "class stream from non stream leaf",
                ty: class_ty(name("user", &["lorem"], "Resume$stream"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "stream_types.lorem.Resume",
            },
            Case {
                label: "class stream same leaf",
                ty: class_ty(name("user", &["lorem"], "Resume$stream"), vec![]),
                ctx: ctx(&["stream_types", "lorem"]),
                expected: "Resume",
            },
            Case {
                label: "class non stream from stream leaf",
                ty: class_ty(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["stream_types", "lorem"]),
                expected: "lorem.Resume",
            },
            Case {
                label: "enum same leaf",
                ty: enum_ty(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["ipsum"]),
                expected: "Sentiment",
            },
            Case {
                label: "enum cross leaf",
                ty: enum_ty(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["lorem"]),
                expected: "ipsum.Sentiment",
            },
            Case {
                label: "type alias same leaf",
                ty: alias_ty(name("user", &["util"], "StringList")),
                ctx: ctx(&["util"]),
                expected: "StringList",
            },
            Case {
                label: "type alias cross leaf",
                ty: alias_ty(name("user", &["util"], "StringList")),
                ctx: ctx(&["lorem"]),
                expected: "util.StringList",
            },
            Case {
                label: "optional string",
                ty: union(vec![
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[str]",
            },
            Case {
                label: "list int",
                ty: list(Box::new(Ty::Int {
                    attr: baml_base::TyAttr::EMPTY,
                })),
                ctx: ctx(&["lorem"]),
                expected: "typing.List[int]",
            },
            Case {
                label: "map string int",
                ty: Ty::Map {
                    key: Box::new(Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                    value: Box::new(Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[str, int]",
            },
            Case {
                label: "map enum to class",
                ty: Ty::Map {
                    key: Box::new(enum_ty(name("user", &["ipsum"], "Sentiment"))),
                    value: Box::new(class_ty(name("user", &["lorem"], "Resume"), vec![])),
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[ipsum.Sentiment, Resume]",
            },
            Case {
                label: "union int string",
                ty: union(vec![
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[int, str]",
            },
            Case {
                label: "union int string bool",
                ty: union(vec![
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    Ty::Bool {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[int, str, bool]",
            },
            Case {
                label: "optional list same leaf class",
                ty: union(vec![
                    list(Box::new(class_ty(
                        name("user", &["lorem"], "Resume"),
                        vec![],
                    ))),
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[typing.List[Resume]]",
            },
            Case {
                label: "list optional string",
                ty: list(Box::new(union(vec![
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]))),
                ctx: ctx(&["lorem"]),
                expected: "typing.List[typing.Optional[str]]",
            },
            Case {
                label: "map vendor list",
                ty: Ty::Map {
                    key: Box::new(Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                    value: Box::new(list(Box::new(class_ty(
                        name("aws", &["s3"], "Bucket"),
                        vec![],
                    )))),
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[str, typing.List[vendor.aws.s3.Bucket]]",
            },
            Case {
                label: "callable two params",
                ty: callable(
                    vec![
                        callable_param(Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        }),
                        callable_param(Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        }),
                    ],
                    Box::new(Ty::Bool {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                ),
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[[int, str], bool]",
            },
            Case {
                label: "callable no params",
                ty: callable(
                    vec![],
                    Box::new(Ty::Void {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                ),
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[[], None]",
            },
            Case {
                label: "callable nested params",
                ty: callable(
                    vec![callable_param(list(Box::new(Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    })))],
                    Box::new(union(vec![
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                        Ty::Null {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    ])),
                ),
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[[typing.List[int]], typing.Optional[str]]",
            },
            Case {
                label: "callable optional params",
                ty: callable(
                    vec![
                        callable_param(Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        }),
                        optional_callable_param(
                            "limit",
                            Ty::Int {
                                attr: baml_base::TyAttr::EMPTY,
                            },
                        ),
                    ],
                    Box::new(Ty::Bool {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                ),
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[..., bool]",
            },
            Case {
                label: "union stream and non stream classes",
                ty: union(vec![
                    class_ty(name("user", &["lorem"], "Resume"), vec![]),
                    class_ty(name("user", &["lorem"], "Resume$stream"), vec![]),
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[Resume, stream_types.lorem.Resume]",
            },
            Case {
                label: "optional media",
                ty: union(vec![
                    media(MediaKind::Image),
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[baml.media.Image]",
            },
            Case {
                label: "recursive alias self ref",
                ty: alias_ty(name("user", &["util"], "RecList")),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "\"RecList\"",
            },
            Case {
                label: "self-ref class no args",
                ty: class_ty(name("user", &["lorem"], "Node"), vec![]),
                ctx: ctx_with_self(&["lorem"], &["lorem"], "Node"),
                expected: "\"Node\"",
            },
            Case {
                label: "self-ref generic class wraps args inside quotes",
                ty: class_ty(
                    name("user", &["lorem"], "Node"),
                    vec![Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    }],
                ),
                ctx: ctx_with_self(&["lorem"], &["lorem"], "Node"),
                expected: "\"Node[str]\"",
            },
            Case {
                label: "self-ref generic class nested in list wraps args inside quotes",
                ty: list(Box::new(class_ty(
                    name("user", &["lorem"], "Node"),
                    vec![Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    }],
                ))),
                ctx: ctx_with_self(&["lorem"], &["lorem"], "Node"),
                expected: "typing.List[\"Node[int]\"]",
            },
            Case {
                label: "recursive alias inside list",
                ty: list(Box::new(alias_ty(name("user", &["util"], "RecList")))),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "typing.List[\"RecList\"]",
            },
            Case {
                label: "recursive alias inside union",
                ty: union(vec![
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                    list(Box::new(alias_ty(name("user", &["util"], "RecList")))),
                ]),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "typing.Union[int, typing.List[\"RecList\"]]",
            },
            Case {
                label: "recursive alias leaves other refs unquoted under self_ref-only",
                ty: list(Box::new(class_ty(name("user", &["util"], "Other"), vec![]))),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "typing.List[Other]",
            },
            // 18c: real recursive-alias bodies set `defer_name_refs`,
            // which forces every named leaf to be a string forward-ref
            // — including same-leaf siblings, cross-leaf names, and
            // root-routed names. The RHS of `TypeAliasType(...)`
            // evaluates eagerly at module load and these names aren't
            // in scope then (hoisting + TYPE_CHECKING guards).
            Case {
                label: "recursive body quotes same-leaf sibling",
                ty: list(Box::new(class_ty(name("user", &["util"], "Other"), vec![]))),
                ctx: ctx_recursive_alias_body(&["util"], &["util"], "RecList"),
                expected: "typing.List[\"Other\"]",
            },
            Case {
                label: "recursive body quotes cross-leaf class as dotted forward-ref",
                ty: list(Box::new(class_ty(name("user", &["util"], "Bar"), vec![]))),
                ctx: ctx_recursive_alias_body(&["lorem"], &["lorem"], "RecList"),
                expected: "typing.List[\"util.Bar\"]",
            },
            Case {
                label: "recursive body quotes root-routed name",
                ty: class_ty(name("user", &[], "Foo"), vec![]),
                ctx: ctx_recursive_alias_body(&["lorem"], &["lorem"], "RecList"),
                expected: "\"Foo\"",
            },
            Case {
                label: "recursive body quotes cross-leaf enum",
                ty: enum_ty(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx_recursive_alias_body(&["lorem"], &["lorem"], "RecList"),
                expected: "\"ipsum.Sentiment\"",
            },
            Case {
                label: "non recursive alias same leaf",
                ty: alias_ty(name("user", &["util"], "RecList")),
                ctx: ctx(&["util"]),
                expected: "RecList",
            },
            Case {
                label: "non recursive alias cross leaf",
                ty: alias_ty(name("user", &["util"], "RecList")),
                ctx: ctx(&["lorem"]),
                expected: "util.RecList",
            },
            Case {
                label: "optional stdlib class",
                ty: union(vec![
                    class_ty(name("baml", &["http"], "Response"), vec![]),
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[baml.http.Response]",
            },
            Case {
                label: "list vendor class",
                ty: list(Box::new(class_ty(name("aws", &["s3"], "Bucket"), vec![]))),
                ctx: ctx(&["lorem"]),
                expected: "typing.List[vendor.aws.s3.Bucket]",
            },
            Case {
                label: "map enum to stream vendor class",
                ty: Ty::Map {
                    key: Box::new(enum_ty(name("user", &["ipsum"], "Sentiment"))),
                    value: Box::new(class_ty(name("aws", &["s3"], "Bucket$stream"), vec![])),
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[ipsum.Sentiment, stream_types.vendor.aws.s3.Bucket]",
            },
            Case {
                label: "union across placements",
                ty: union(vec![
                    class_ty(name("user", &["lorem"], "Resume"), vec![]),
                    class_ty(name("aws", &["s3"], "Bucket"), vec![]),
                    class_ty(name("baml", &["http"], "Response"), vec![]),
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[Resume, vendor.aws.s3.Bucket, baml.http.Response]",
            },
            // Generics — `13a` §3.1, §3.2, §3.4.
            Case {
                label: "generic class same leaf concrete int",
                ty: class_ty(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    }],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[int]",
            },
            Case {
                label: "generic class cross leaf concrete int",
                ty: class_ty(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    }],
                ),
                ctx: ctx(&["ipsum"]),
                expected: "lorem.Box[int]",
            },
            Case {
                label: "generic class with list arg",
                ty: class_ty(
                    name("user", &["lorem"], "Box"),
                    vec![list(Box::new(Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    }))],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[typing.List[int]]",
            },
            Case {
                label: "generic class nested generic arg",
                ty: class_ty(
                    name("user", &["lorem"], "Box"),
                    vec![class_ty(
                        name("user", &["lorem"], "Box"),
                        vec![Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        }],
                    )],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[Box[int]]",
            },
            Case {
                label: "generic class stream from non-stream leaf",
                ty: class_ty(
                    name("user", &["lorem"], "Box$stream"),
                    vec![Ty::Int {
                        attr: baml_base::TyAttr::EMPTY,
                    }],
                ),
                ctx: ctx(&["lorem"]),
                expected: "stream_types.lorem.Box[int]",
            },
            Case {
                label: "generic class with typevar arg",
                ty: class_ty(
                    name("user", &["lorem"], "Box"),
                    vec![type_var(baml_base::Name::new("T"))],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[T]",
            },
            Case {
                label: "bare typevar",
                ty: type_var(baml_base::Name::new("T")),
                ctx: ctx(&["lorem"]),
                expected: "T",
            },
            Case {
                label: "map with typevar key and value",
                ty: Ty::Map {
                    key: Box::new(Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    }),
                    value: Box::new(type_var(baml_base::Name::new("V"))),
                    attr: baml_base::TyAttr::EMPTY,
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[str, V]",
            },
        ];

        for case in &cases {
            assert_ty(case);
        }
    }
}
