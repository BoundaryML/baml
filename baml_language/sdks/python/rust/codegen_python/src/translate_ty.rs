//! Pure `Ty` -> Python type-expression translation for the phase-G3
//! emitter rewrite.
//!
//! Rule sources:
//! - `.humanlayer/tasks/clientpython/09b-codegen-rules.md` §6, §9
//! - `.humanlayer/tasks/clientpython/11e-phaseg3-ty-translator.md`

use baml_base::{Literal, MediaKind};
use baml_codegen_types::{Name, Ty};

use crate::{
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelfRef {
    pub(crate) routed_leaf: LeafPath,
    pub(crate) bare_name: String,
}

pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> String {
    match ty {
        Ty::Int => "int".to_string(),
        Ty::Float => "float".to_string(),
        Ty::String => "str".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Null => "None".to_string(),
        Ty::Literal(Literal::Int(value)) => format!("typing.Literal[{value}]"),
        Ty::Literal(Literal::String(value)) => {
            format!("typing.Literal[{}]", py_string(value))
        }
        Ty::Literal(Literal::Bool(true)) => "typing.Literal[True]".to_string(),
        Ty::Literal(Literal::Bool(false)) => "typing.Literal[False]".to_string(),
        // Python does not allow float parameters to typing.Literal.
        Ty::Literal(Literal::Float(_)) => "typing.Any".to_string(),
        Ty::Uint8Array => "bytes".to_string(),
        Ty::Media(MediaKind::Image) => media_ref("Image", ctx),
        Ty::Media(MediaKind::Audio) => media_ref("Audio", ctx),
        Ty::Media(MediaKind::Video) => media_ref("Video", ctx),
        Ty::Media(MediaKind::Pdf) => media_ref("Pdf", ctx),
        Ty::Media(MediaKind::Generic) => "typing.Any".to_string(),
        Ty::Class(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(|a| translate_ty(a, ctx)).collect();
            render_name_ref_or_self_ref(name, ctx, &arg_strs.join(", "))
        }
        Ty::TypeAlias(name) => render_name_ref_or_self_ref(name, ctx, ""),
        Ty::Enum(name) => {
            let head = render_name_ref(name, ctx);
            if should_defer_name_ref(ctx) {
                py_string(&head)
            } else {
                head
            }
        }
        Ty::TypeVar(name) => name.as_str().to_string(),
        Ty::Optional(inner) => format!("typing.Optional[{}]", translate_ty(inner, ctx)),
        Ty::List(inner) => format!("typing.List[{}]", translate_ty(inner, ctx)),
        Ty::Map { key, value } => {
            format!(
                "typing.Dict[{}, {}]",
                translate_ty(key, ctx),
                translate_ty(value, ctx)
            )
        }
        Ty::Union(items) => format!(
            "typing.Union[{}]",
            items
                .iter()
                .map(|item| translate_ty(item, ctx))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Ty::BuiltinUnknown => "typing.Any".to_string(),
        Ty::Callable { params, ret } => {
            let ret = translate_ty(ret, ctx);
            if params
                .iter()
                .any(|param| param.mode == baml_codegen_types::CodegenFunctionParamMode::Optional)
            {
                format!("typing.Callable[..., {ret}]")
            } else {
                format!(
                    "typing.Callable[[{}], {}]",
                    params
                        .iter()
                        .map(|param| translate_ty(&param.ty, ctx))
                        .collect::<Vec<_>>()
                        .join(", "),
                    ret
                )
            }
        }
        Ty::Unit => "None".to_string(),
        Ty::BamlOptions => "baml.Options".to_string(),
        // `$rust_type` fields in stdlib stubs (Response._body, SseStream._handle, …).
        // The host-language opaque-handle wrapper is `BamlPyHandle` from the
        // bridge runtime, imported as `_BamlPyHandle` to keep `baml` (the
        // local relative module) from shadowing it. The single-underscore
        // field name still triggers Pydantic v2's private-attribute handling
        // regardless of the annotation; `_decode_class` injects the value
        // into `__pydantic_private__` post-construction.
        Ty::RustType => "_BamlPyHandle".to_string(),
    }
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
            route_class_ref(name) == self_ref.routed_leaf && name.bare_name() == self_ref.bare_name
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
    let routed_leaf = route_class_ref(name);
    if routed_leaf == ctx.current_leaf || routed_leaf.segments.is_empty() {
        name.bare_name().to_string()
    } else {
        format!("{}.{}", routed_leaf.segments.join("."), name.bare_name())
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

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
            self_ref: Some(SelfRef {
                routed_leaf: leaf(self_segments),
                bare_name: bare_name.to_string(),
            }),
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
            self_ref: Some(SelfRef {
                routed_leaf: leaf(self_segments),
                bare_name: bare_name.to_string(),
            }),
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
            Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Null
            | Ty::Literal(_)
            | Ty::Uint8Array
            | Ty::Media(_)
            | Ty::Class(_, _)
            | Ty::Enum(_)
            | Ty::TypeAlias(_)
            | Ty::TypeVar(_)
            | Ty::Optional(_)
            | Ty::List(_)
            | Ty::Map { .. }
            | Ty::Union(_)
            | Ty::BuiltinUnknown
            | Ty::Callable { .. }
            | Ty::Unit
            | Ty::RustType
            | Ty::BamlOptions => {}
        }
    }

    #[test]
    fn translate_ty_covers_phase_g3_matrix() {
        let cases = vec![
            Case {
                label: "int",
                ty: Ty::Int,
                ctx: ctx(&["lorem"]),
                expected: "int",
            },
            Case {
                label: "float",
                ty: Ty::Float,
                ctx: ctx(&["lorem"]),
                expected: "float",
            },
            Case {
                label: "string",
                ty: Ty::String,
                ctx: ctx(&["lorem"]),
                expected: "str",
            },
            Case {
                label: "bool",
                ty: Ty::Bool,
                ctx: ctx(&["lorem"]),
                expected: "bool",
            },
            Case {
                label: "null",
                ty: Ty::Null,
                ctx: ctx(&["lorem"]),
                expected: "None",
            },
            Case {
                label: "uint8array",
                ty: Ty::Uint8Array,
                ctx: ctx(&["lorem"]),
                expected: "bytes",
            },
            Case {
                label: "builtin unknown",
                ty: Ty::BuiltinUnknown,
                ctx: ctx(&["lorem"]),
                expected: "typing.Any",
            },
            Case {
                label: "unit",
                ty: Ty::Unit,
                ctx: ctx(&["lorem"]),
                expected: "None",
            },
            Case {
                label: "baml options",
                ty: Ty::BamlOptions,
                ctx: ctx(&["lorem"]),
                expected: "baml.Options",
            },
            Case {
                label: "literal int",
                ty: Ty::Literal(Literal::Int(42)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[42]",
            },
            Case {
                label: "literal negative int",
                ty: Ty::Literal(Literal::Int(-1)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[-1]",
            },
            Case {
                label: "literal string",
                ty: Ty::Literal(Literal::String("draft".to_string())),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[\"draft\"]",
            },
            Case {
                label: "literal escaped string",
                ty: Ty::Literal(Literal::String("has \"quotes\"".to_string())),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[\"has \\\"quotes\\\"\"]",
            },
            Case {
                label: "literal bool true",
                ty: Ty::Literal(Literal::Bool(true)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[True]",
            },
            Case {
                label: "literal bool false",
                ty: Ty::Literal(Literal::Bool(false)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Literal[False]",
            },
            Case {
                label: "literal float fallback",
                ty: Ty::Literal(Literal::Float("3.14".to_string())),
                ctx: ctx(&["lorem"]),
                expected: "typing.Any",
            },
            Case {
                label: "media image",
                ty: Ty::Media(MediaKind::Image),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Image",
            },
            Case {
                label: "media audio",
                ty: Ty::Media(MediaKind::Audio),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Audio",
            },
            Case {
                label: "media video",
                ty: Ty::Media(MediaKind::Video),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Video",
            },
            Case {
                label: "media pdf",
                ty: Ty::Media(MediaKind::Pdf),
                ctx: ctx(&["lorem"]),
                expected: "baml.media.Pdf",
            },
            Case {
                label: "media generic fallback",
                ty: Ty::Media(MediaKind::Generic),
                ctx: ctx(&["lorem"]),
                expected: "typing.Any",
            },
            Case {
                label: "class same leaf root namespace",
                ty: Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "Resume",
            },
            Case {
                label: "class cross leaf root namespace",
                ty: Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["ipsum"]),
                expected: "lorem.Resume",
            },
            Case {
                label: "class same leaf root init",
                ty: Ty::Class(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&[]),
                expected: "Foo",
            },
            Case {
                label: "class root init from namespaced leaf",
                ty: Ty::Class(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "Foo",
            },
            Case {
                label: "class vendor cross leaf",
                ty: Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "vendor.aws.s3.Bucket",
            },
            Case {
                label: "class vendor same leaf",
                ty: Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["vendor", "aws", "s3"]),
                expected: "Bucket",
            },
            Case {
                label: "class vendor other vendor leaf",
                ty: Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["vendor", "aws", "ec2"]),
                expected: "vendor.aws.s3.Bucket",
            },
            Case {
                label: "class stdlib cross leaf",
                ty: Ty::Class(name("baml", &["http"], "Response"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "baml.http.Response",
            },
            Case {
                label: "class stdlib same leaf",
                ty: Ty::Class(name("baml", &["http"], "Response"), vec![]),
                ctx: ctx(&["baml", "http"]),
                expected: "Response",
            },
            Case {
                label: "class stream from non stream leaf",
                ty: Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]),
                ctx: ctx(&["lorem"]),
                expected: "stream_types.lorem.Resume",
            },
            Case {
                label: "class stream same leaf",
                ty: Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]),
                ctx: ctx(&["stream_types", "lorem"]),
                expected: "Resume",
            },
            Case {
                label: "class non stream from stream leaf",
                ty: Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["stream_types", "lorem"]),
                expected: "lorem.Resume",
            },
            Case {
                label: "enum same leaf",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["ipsum"]),
                expected: "Sentiment",
            },
            Case {
                label: "enum cross leaf",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["lorem"]),
                expected: "ipsum.Sentiment",
            },
            Case {
                label: "type alias same leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "StringList")),
                ctx: ctx(&["util"]),
                expected: "StringList",
            },
            Case {
                label: "type alias cross leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "StringList")),
                ctx: ctx(&["lorem"]),
                expected: "util.StringList",
            },
            Case {
                label: "optional string",
                ty: Ty::Optional(Box::new(Ty::String)),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[str]",
            },
            Case {
                label: "list int",
                ty: Ty::List(Box::new(Ty::Int)),
                ctx: ctx(&["lorem"]),
                expected: "typing.List[int]",
            },
            Case {
                label: "map string int",
                ty: Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(Ty::Int),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[str, int]",
            },
            Case {
                label: "map enum to class",
                ty: Ty::Map {
                    key: Box::new(Ty::Enum(name("user", &["ipsum"], "Sentiment"))),
                    value: Box::new(Ty::Class(name("user", &["lorem"], "Resume"), vec![])),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[ipsum.Sentiment, Resume]",
            },
            Case {
                label: "union int string",
                ty: Ty::Union(vec![Ty::Int, Ty::String]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[int, str]",
            },
            Case {
                label: "union int string bool",
                ty: Ty::Union(vec![Ty::Int, Ty::String, Ty::Bool]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[int, str, bool]",
            },
            Case {
                label: "optional list same leaf class",
                ty: Ty::Optional(Box::new(Ty::List(Box::new(Ty::Class(
                    name("user", &["lorem"], "Resume"),
                    vec![],
                ))))),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[typing.List[Resume]]",
            },
            Case {
                label: "list optional string",
                ty: Ty::List(Box::new(Ty::Optional(Box::new(Ty::String)))),
                ctx: ctx(&["lorem"]),
                expected: "typing.List[typing.Optional[str]]",
            },
            Case {
                label: "map vendor list",
                ty: Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(Ty::List(Box::new(Ty::Class(
                        name("aws", &["s3"], "Bucket"),
                        vec![],
                    )))),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[str, typing.List[vendor.aws.s3.Bucket]]",
            },
            Case {
                label: "callable two params",
                ty: Ty::Callable {
                    params: vec![callable_param(Ty::Int), callable_param(Ty::String)],
                    ret: Box::new(Ty::Bool),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[[int, str], bool]",
            },
            Case {
                label: "callable no params",
                ty: Ty::Callable {
                    params: vec![],
                    ret: Box::new(Ty::Unit),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[[], None]",
            },
            Case {
                label: "callable nested params",
                ty: Ty::Callable {
                    params: vec![callable_param(Ty::List(Box::new(Ty::Int)))],
                    ret: Box::new(Ty::Optional(Box::new(Ty::String))),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[[typing.List[int]], typing.Optional[str]]",
            },
            Case {
                label: "callable optional params",
                ty: Ty::Callable {
                    params: vec![
                        callable_param(Ty::String),
                        optional_callable_param("limit", Ty::Int),
                    ],
                    ret: Box::new(Ty::Bool),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Callable[..., bool]",
            },
            Case {
                label: "union stream and non stream classes",
                ty: Ty::Union(vec![
                    Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                    Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]),
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[Resume, stream_types.lorem.Resume]",
            },
            Case {
                label: "optional media",
                ty: Ty::Optional(Box::new(Ty::Media(MediaKind::Image))),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[baml.media.Image]",
            },
            Case {
                label: "recursive alias self ref",
                ty: Ty::TypeAlias(name("user", &["util"], "RecList")),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "\"RecList\"",
            },
            Case {
                label: "self-ref class no args",
                ty: Ty::Class(name("user", &["lorem"], "Node"), vec![]),
                ctx: ctx_with_self(&["lorem"], &["lorem"], "Node"),
                expected: "\"Node\"",
            },
            Case {
                label: "self-ref generic class wraps args inside quotes",
                ty: Ty::Class(name("user", &["lorem"], "Node"), vec![Ty::String]),
                ctx: ctx_with_self(&["lorem"], &["lorem"], "Node"),
                expected: "\"Node[str]\"",
            },
            Case {
                label: "self-ref generic class nested in list wraps args inside quotes",
                ty: Ty::List(Box::new(Ty::Class(
                    name("user", &["lorem"], "Node"),
                    vec![Ty::Int],
                ))),
                ctx: ctx_with_self(&["lorem"], &["lorem"], "Node"),
                expected: "typing.List[\"Node[int]\"]",
            },
            Case {
                label: "recursive alias inside list",
                ty: Ty::List(Box::new(Ty::TypeAlias(name("user", &["util"], "RecList")))),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "typing.List[\"RecList\"]",
            },
            Case {
                label: "recursive alias inside union",
                ty: Ty::Union(vec![
                    Ty::Int,
                    Ty::List(Box::new(Ty::TypeAlias(name("user", &["util"], "RecList")))),
                ]),
                ctx: ctx_with_self(&["util"], &["util"], "RecList"),
                expected: "typing.Union[int, typing.List[\"RecList\"]]",
            },
            Case {
                label: "recursive alias leaves other refs unquoted under self_ref-only",
                ty: Ty::List(Box::new(Ty::Class(
                    name("user", &["util"], "Other"),
                    vec![],
                ))),
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
                ty: Ty::List(Box::new(Ty::Class(
                    name("user", &["util"], "Other"),
                    vec![],
                ))),
                ctx: ctx_recursive_alias_body(&["util"], &["util"], "RecList"),
                expected: "typing.List[\"Other\"]",
            },
            Case {
                label: "recursive body quotes cross-leaf class as dotted forward-ref",
                ty: Ty::List(Box::new(Ty::Class(name("user", &["util"], "Bar"), vec![]))),
                ctx: ctx_recursive_alias_body(&["lorem"], &["lorem"], "RecList"),
                expected: "typing.List[\"util.Bar\"]",
            },
            Case {
                label: "recursive body quotes root-routed name",
                ty: Ty::Class(name("user", &[], "Foo"), vec![]),
                ctx: ctx_recursive_alias_body(&["lorem"], &["lorem"], "RecList"),
                expected: "\"Foo\"",
            },
            Case {
                label: "recursive body quotes cross-leaf enum",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx_recursive_alias_body(&["lorem"], &["lorem"], "RecList"),
                expected: "\"ipsum.Sentiment\"",
            },
            Case {
                label: "non recursive alias same leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "RecList")),
                ctx: ctx(&["util"]),
                expected: "RecList",
            },
            Case {
                label: "non recursive alias cross leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "RecList")),
                ctx: ctx(&["lorem"]),
                expected: "util.RecList",
            },
            Case {
                label: "optional stdlib class",
                ty: Ty::Optional(Box::new(Ty::Class(
                    name("baml", &["http"], "Response"),
                    vec![],
                ))),
                ctx: ctx(&["lorem"]),
                expected: "typing.Optional[baml.http.Response]",
            },
            Case {
                label: "list vendor class",
                ty: Ty::List(Box::new(Ty::Class(name("aws", &["s3"], "Bucket"), vec![]))),
                ctx: ctx(&["lorem"]),
                expected: "typing.List[vendor.aws.s3.Bucket]",
            },
            Case {
                label: "map enum to stream vendor class",
                ty: Ty::Map {
                    key: Box::new(Ty::Enum(name("user", &["ipsum"], "Sentiment"))),
                    value: Box::new(Ty::Class(name("aws", &["s3"], "Bucket$stream"), vec![])),
                },
                ctx: ctx(&["lorem"]),
                expected: "typing.Dict[ipsum.Sentiment, stream_types.vendor.aws.s3.Bucket]",
            },
            Case {
                label: "union across placements",
                ty: Ty::Union(vec![
                    Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                    Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                    Ty::Class(name("baml", &["http"], "Response"), vec![]),
                ]),
                ctx: ctx(&["lorem"]),
                expected: "typing.Union[Resume, vendor.aws.s3.Bucket, baml.http.Response]",
            },
            // Generics — `13a` §3.1, §3.2, §3.4.
            Case {
                label: "generic class same leaf concrete int",
                ty: Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int]),
                ctx: ctx(&["lorem"]),
                expected: "Box[int]",
            },
            Case {
                label: "generic class cross leaf concrete int",
                ty: Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int]),
                ctx: ctx(&["ipsum"]),
                expected: "lorem.Box[int]",
            },
            Case {
                label: "generic class with list arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::List(Box::new(Ty::Int))],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[typing.List[int]]",
            },
            Case {
                label: "generic class nested generic arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int])],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[Box[int]]",
            },
            Case {
                label: "generic class stream from non-stream leaf",
                ty: Ty::Class(name("user", &["lorem"], "Box$stream"), vec![Ty::Int]),
                ctx: ctx(&["lorem"]),
                expected: "stream_types.lorem.Box[int]",
            },
            Case {
                label: "generic class with typevar arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::TypeVar(baml_base::Name::new("T"))],
                ),
                ctx: ctx(&["lorem"]),
                expected: "Box[T]",
            },
            Case {
                label: "bare typevar",
                ty: Ty::TypeVar(baml_base::Name::new("T")),
                ctx: ctx(&["lorem"]),
                expected: "T",
            },
            Case {
                label: "map with typevar key and value",
                ty: Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(Ty::TypeVar(baml_base::Name::new("V"))),
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
