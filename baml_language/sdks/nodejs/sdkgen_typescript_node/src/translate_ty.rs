//! BAML `Ty` → TypeScript type-expression translation. Pure function;
//! Phase 4 emitters call this from every type position (class fields,
//! function args, return types, type-alias bodies, method signatures).
//!
//! Rule sources:
//! - `00a-spec-codegen-mappings.md` §"Exhaustive Ty conversions"
//! - Python prior art: `sdkgen_python_pydantic2/src/translate_ty.rs`
//!
//! Returns a `TranslatedType { expr, imports }`: `expr` is a TS type
//! expression; `imports` is the set of cross-leaf `LeafPath`s the expr
//! references as a **root-relative dotted path** (e.g. `lorem.Resume`).
//! Phase 4 materializes these as a single root-namespace import per leaf
//! (`import type * as <rootns> from ".."`) and prefixes the dotted path.
//!
//! TypeScript resolves forward/recursive references in class bodies and
//! type aliases natively, so — unlike the Python port — there is no
//! self-ref quoting or `defer_name_refs`.
//!
use std::collections::BTreeSet;

use baml_base::{Literal, MediaKind};
use baml_codegen_types::{CodegenFunctionParamMode, Name, Ty};

use crate::{
    routing::{LeafPath, route_class_ref},
    ts_string,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranslateCtx {
    pub(crate) current_leaf: LeafPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TranslatedType {
    pub(crate) expr: String,
    pub(crate) imports: BTreeSet<LeafPath>,
}

impl TranslatedType {
    fn bare(expr: impl Into<String>) -> Self {
        Self {
            expr: expr.into(),
            imports: BTreeSet::new(),
        }
    }
}

pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> TranslatedType {
    match ty {
        Ty::Int => TranslatedType::bare("number"),
        Ty::Bigint => TranslatedType::bare("bigint"),
        Ty::Float => TranslatedType::bare("number"),
        Ty::String => TranslatedType::bare("string"),
        Ty::Bool => TranslatedType::bare("boolean"),
        Ty::Null => TranslatedType::bare("null"),
        Ty::Uint8Array => TranslatedType::bare("Uint8Array"),
        Ty::BuiltinUnknown => TranslatedType::bare("unknown"),
        Ty::Unit => TranslatedType::bare("null"),
        Ty::BamlOptions => {
            // `baml.Options` lives in the `baml` namespace; record the import.
            let mut imports = BTreeSet::new();
            imports.insert(LeafPath {
                segments: vec!["baml".to_string()],
            });
            TranslatedType {
                expr: "baml.Options".to_string(),
                imports,
            }
        }
        // `_BamlHandle` is the runtime opaque-handle type; Phase 4 emits the
        // `import type { BamlHandle as _BamlHandle }` when this token appears.
        Ty::RustType => TranslatedType::bare("_BamlHandle"),

        Ty::Literal(Literal::Int(value)) => TranslatedType::bare(format!("{value}")),
        // `bigint` literal types use the `n` suffix in TypeScript: `42n`.
        Ty::Literal(Literal::Bigint(value)) => TranslatedType::bare(format!("{value}n")),
        Ty::Literal(Literal::String(value)) => TranslatedType::bare(ts_string(value)),
        Ty::Literal(Literal::Bool(true)) => TranslatedType::bare("true"),
        Ty::Literal(Literal::Bool(false)) => TranslatedType::bare("false"),
        // Float literals have no TS literal-type form; widen to `number`.
        Ty::Literal(Literal::Float(_)) => TranslatedType::bare("number"),

        Ty::Media(MediaKind::Image) => media_ref("Image", ctx),
        Ty::Media(MediaKind::Audio) => media_ref("Audio", ctx),
        Ty::Media(MediaKind::Video) => media_ref("Video", ctx),
        Ty::Media(MediaKind::Pdf) => media_ref("Pdf", ctx),
        Ty::Media(MediaKind::Generic) => TranslatedType::bare("unknown"),

        Ty::List(inner) => {
            let inner = translate_ty(inner, ctx);
            // Postfix `[]` binds tighter than `|`, so a union/optional element
            // must be parenthesized: `(string | null)[]`.
            let elem = if inner.expr.contains(" | ") {
                format!("({})", inner.expr)
            } else {
                inner.expr
            };
            TranslatedType {
                expr: format!("{elem}[]"),
                imports: inner.imports,
            }
        }
        Ty::Map { key, value } => {
            let key = translate_ty(key, ctx);
            let value = translate_ty(value, ctx);
            let mut imports = key.imports;
            imports.extend(value.imports);
            // Use an inline index/mapped type rather than `Record<K, V>`: a
            // `Record` (itself a mapped-type alias) does NOT defer recursion,
            // so a recursive alias like `type Json = … | Map<string, Json>`
            // is rejected by tsc (TS2456). An inline `{ [key: string]: V }`
            // does defer. Map keys are always `string` or an enum (validated
            // upstream); enum keys become a partial mapped type.
            let expr = if key.expr == "string" {
                format!("{{ [key: string]: {} }}", value.expr)
            } else {
                format!("{{ [key in {}]?: {} }}", key.expr, value.expr)
            };
            TranslatedType { expr, imports }
        }
        Ty::Union(items) => {
            let mut imports = BTreeSet::new();
            let parts: Vec<String> = items
                .iter()
                .map(|item| {
                    let t = translate_ty(item, ctx);
                    imports.extend(t.imports);
                    t.expr
                })
                .collect();
            TranslatedType {
                expr: parts.join(" | "),
                imports,
            }
        }

        Ty::Class(name, args) => {
            let mut result = render_name_ref(name, ctx);
            if !args.is_empty() {
                let mut arg_imports = BTreeSet::new();
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| {
                        let t = translate_ty(a, ctx);
                        arg_imports.extend(t.imports);
                        t.expr
                    })
                    .collect();
                result.expr = format!("{}<{}>", result.expr, arg_strs.join(", "));
                result.imports.extend(arg_imports);
            }
            result
        }
        Ty::Enum(name) => render_name_ref(name, ctx),
        Ty::TypeAlias(name) => render_name_ref(name, ctx),
        Ty::TypeVar(name) => TranslatedType::bare(name.as_str().to_string()),

        Ty::Callable { params, ret } => translate_callable(params, ret, ctx, false),
    }
}

/// Translate a type used for an argument supplied by the Node host. A direct
/// callable argument may return either its declared BAML result or a thenable
/// of that result: the async bridge awaits host callbacks before resuming the
/// BAML task. Ordinary value positions intentionally keep the exact callable
/// type, so a BAML function that *returns* a closure is not advertised as an
/// async host callback.
pub(crate) fn translate_host_input_ty(ty: &Ty, ctx: &TranslateCtx) -> TranslatedType {
    match ty {
        Ty::Callable { params, ret } => translate_callable(params, ret, ctx, true),
        _ => translate_ty(ty, ctx),
    }
}

fn translate_callable(
    params: &[baml_codegen_types::CallableParam],
    ret: &Ty,
    ctx: &TranslateCtx,
    accepts_promise_like: bool,
) -> TranslatedType {
    let ret_t = translate_ty(ret, ctx);
    let mut imports = ret_t.imports;
    // Required params stay positional; optional params are grouped into a
    // trailing `$opts?: { name?: T | undefined; … } | undefined` object. The
    // engine invokes callbacks positionally; the bridge dispatch adapter folds
    // optional values back into this object before calling user code.
    let mut positional: Vec<String> = Vec::new();
    let mut opt_fields: Vec<String> = Vec::new();
    for (idx, p) in params.iter().enumerate() {
        let t = translate_ty(&p.ty, ctx);
        imports.extend(t.imports);
        let arg_name = p
            .name
            .as_ref()
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| format!("arg{idx}"));
        if p.mode == CodegenFunctionParamMode::Optional {
            opt_fields.push(format!(
                "{}?: {} | undefined",
                crate::leaf::option_field_name(&arg_name),
                t.expr
            ));
        } else {
            positional.push(format!(
                "{}: {}",
                crate::leaf::safe_param_name(&arg_name),
                t.expr
            ));
        }
    }
    if !opt_fields.is_empty() {
        positional.push(format!(
            "$opts?: {{ {} }} | undefined",
            opt_fields.join("; ")
        ));
    }

    let ret_expr = if accepts_promise_like {
        let direct = if ret_t.expr.contains("=>") {
            format!("({})", ret_t.expr)
        } else {
            ret_t.expr.clone()
        };
        format!("{direct} | PromiseLike<{}>", ret_t.expr)
    } else {
        ret_t.expr
    };
    TranslatedType {
        expr: format!("({}) => {ret_expr}", positional.join(", ")),
        imports,
    }
}

fn media_ref(bare: &str, ctx: &TranslateCtx) -> TranslatedType {
    let name = Name::new(
        baml_base::Name::new("baml"),
        vec![baml_base::Name::new("media")],
        baml_base::Name::new(bare),
    );
    render_name_ref(&name, ctx)
}

/// The namespace alias Phase 4 binds to the package root, used to reach
/// root-namespace symbols from a non-root leaf (`_bamlRoot.Foo`). Phase 4
/// emits `import type * as _bamlRoot from "<rel-to-root>"` whenever the
/// root `LeafPath` (empty segments) appears in `imports`.
pub(crate) const ROOT_ALIAS: &str = "_bamlRoot";

/// Render a class/enum/alias name reference.
///
/// The emitted identifier is the BAML name verbatim — including any
/// `$stream` suffix (spec2: `$` is a valid TS identifier char, and stream
/// companions live beside their base type rather than in a `stream_types/`
/// namespace).
///
/// - Same leaf → bare name, no import.
/// - Root-namespace symbol referenced from a non-root leaf → `_bamlRoot.Name`
///   plus the root `LeafPath` (empty segments) in `imports`.
/// - Otherwise → root-relative dotted path (`lorem.Resume`,
///   `vendor.aws.s3.Bucket`, `lorem.Resume$stream`) plus the routed
///   `LeafPath` in `imports`.
fn render_name_ref(name: &Name, ctx: &TranslateCtx) -> TranslatedType {
    let routed = route_class_ref(name);
    let ident = name.name.as_str();
    if routed == ctx.current_leaf {
        return TranslatedType::bare(ident.to_string());
    }
    let mut imports = BTreeSet::new();
    if routed.segments.is_empty() {
        // Root-namespace symbol from a non-root leaf (the same-leaf case is
        // caught above, so `ctx.current_leaf` is non-root here).
        imports.insert(routed);
        TranslatedType {
            expr: format!("{ROOT_ALIAS}.{ident}"),
            imports,
        }
    } else {
        let dotted = routed.segments.join(".");
        imports.insert(routed);
        TranslatedType {
            expr: format!("{dotted}.{ident}"),
            imports,
        }
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
        expected_expr: &'static str,
        expected_imports: &'static [&'static [&'static str]],
    }

    fn leaf(segments: &[&str]) -> LeafPath {
        LeafPath {
            segments: segments.iter().map(ToString::to_string).collect(),
        }
    }
    fn ctx(segments: &[&str]) -> TranslateCtx {
        TranslateCtx {
            current_leaf: leaf(segments),
        }
    }
    fn name(pkg: &str, namespace_path: &[&str], bare_name: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            namespace_path.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(bare_name),
        )
    }
    fn callable_param(ty: Ty) -> baml_codegen_types::CallableParam {
        baml_codegen_types::CallableParam {
            name: None,
            ty,
            mode: CodegenFunctionParamMode::Required,
        }
    }
    fn optional_callable_param(name: &str, ty: Ty) -> baml_codegen_types::CallableParam {
        baml_codegen_types::CallableParam {
            name: Some(BaseName::new(name)),
            ty,
            mode: CodegenFunctionParamMode::Optional,
        }
    }

    /// Forces this test file to be updated whenever a `Ty` variant is added.
    fn check_exhaustive(ty: &Ty) {
        match ty {
            Ty::Int
            | Ty::Bigint
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
            | Ty::List(_)
            | Ty::Map { .. }
            | Ty::Union(_)
            | Ty::BuiltinUnknown
            | Ty::Callable { .. }
            | Ty::Unit
            | Ty::BamlOptions
            | Ty::RustType => {}
        }
    }

    fn assert_ty(case: &Case) {
        check_exhaustive(&case.ty);
        let result = translate_ty(&case.ty, &case.ctx);
        assert_eq!(
            result.expr, case.expected_expr,
            "expr mismatch for case {}",
            case.label
        );
        let expected_imports: BTreeSet<LeafPath> = case
            .expected_imports
            .iter()
            .map(|segs| LeafPath {
                segments: segs.iter().map(ToString::to_string).collect(),
            })
            .collect();
        assert_eq!(
            result.imports, expected_imports,
            "imports mismatch for case {}",
            case.label
        );
    }

    fn boxed(ty: Ty) -> Box<Ty> {
        Box::new(ty)
    }

    #[test]
    fn translate_ty_matrix() {
        let cls = |pkg, ns: &[&str], n| Ty::Class(name(pkg, ns, n), vec![]);
        let cases: Vec<Case> = vec![
            // ── Primitives ──
            Case {
                label: "int",
                ty: Ty::Int,
                ctx: ctx(&[]),
                expected_expr: "number",
                expected_imports: &[],
            },
            Case {
                label: "bigint",
                ty: Ty::Bigint,
                ctx: ctx(&[]),
                expected_expr: "bigint",
                expected_imports: &[],
            },
            Case {
                label: "float",
                ty: Ty::Float,
                ctx: ctx(&[]),
                expected_expr: "number",
                expected_imports: &[],
            },
            Case {
                label: "string",
                ty: Ty::String,
                ctx: ctx(&[]),
                expected_expr: "string",
                expected_imports: &[],
            },
            Case {
                label: "bool",
                ty: Ty::Bool,
                ctx: ctx(&[]),
                expected_expr: "boolean",
                expected_imports: &[],
            },
            Case {
                label: "null",
                ty: Ty::Null,
                ctx: ctx(&[]),
                expected_expr: "null",
                expected_imports: &[],
            },
            Case {
                label: "uint8array",
                ty: Ty::Uint8Array,
                ctx: ctx(&[]),
                expected_expr: "Uint8Array",
                expected_imports: &[],
            },
            Case {
                label: "builtin_unknown",
                ty: Ty::BuiltinUnknown,
                ctx: ctx(&[]),
                expected_expr: "unknown",
                expected_imports: &[],
            },
            Case {
                label: "unit",
                ty: Ty::Unit,
                ctx: ctx(&[]),
                expected_expr: "null",
                expected_imports: &[],
            },
            Case {
                label: "baml_options",
                ty: Ty::BamlOptions,
                ctx: ctx(&[]),
                expected_expr: "baml.Options",
                expected_imports: &[&["baml"]],
            },
            Case {
                label: "rust_type",
                ty: Ty::RustType,
                ctx: ctx(&[]),
                expected_expr: "_BamlHandle",
                expected_imports: &[],
            },
            // ── Literals ──
            Case {
                label: "lit_int",
                ty: Ty::Literal(Literal::Int(42)),
                ctx: ctx(&[]),
                expected_expr: "42",
                expected_imports: &[],
            },
            Case {
                label: "lit_bigint",
                ty: Ty::Literal(Literal::Bigint(42i64.into())),
                ctx: ctx(&[]),
                expected_expr: "42n",
                expected_imports: &[],
            },
            Case {
                label: "lit_string",
                ty: Ty::Literal(Literal::String("hi \"x\"".into())),
                ctx: ctx(&[]),
                expected_expr: "\"hi \\\"x\\\"\"",
                expected_imports: &[],
            },
            Case {
                label: "lit_true",
                ty: Ty::Literal(Literal::Bool(true)),
                ctx: ctx(&[]),
                expected_expr: "true",
                expected_imports: &[],
            },
            Case {
                label: "lit_false",
                ty: Ty::Literal(Literal::Bool(false)),
                ctx: ctx(&[]),
                expected_expr: "false",
                expected_imports: &[],
            },
            Case {
                label: "lit_float",
                ty: Ty::Literal(Literal::Float("3.14".into())),
                ctx: ctx(&[]),
                expected_expr: "number",
                expected_imports: &[],
            },
            // ── Media ──
            Case {
                label: "media_image_from_user",
                ty: Ty::Media(MediaKind::Image),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Image",
                expected_imports: &[&["baml", "media"]],
            },
            Case {
                label: "media_audio_from_user",
                ty: Ty::Media(MediaKind::Audio),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Audio",
                expected_imports: &[&["baml", "media"]],
            },
            Case {
                label: "media_video_from_user",
                ty: Ty::Media(MediaKind::Video),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Video",
                expected_imports: &[&["baml", "media"]],
            },
            Case {
                label: "media_pdf_from_user",
                ty: Ty::Media(MediaKind::Pdf),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Pdf",
                expected_imports: &[&["baml", "media"]],
            },
            Case {
                label: "media_image_same_leaf",
                ty: Ty::Media(MediaKind::Image),
                ctx: ctx(&["baml", "media"]),
                expected_expr: "Image",
                expected_imports: &[],
            },
            Case {
                label: "media_generic",
                ty: Ty::Media(MediaKind::Generic),
                ctx: ctx(&[]),
                expected_expr: "unknown",
                expected_imports: &[],
            },
            // ── Containers ──
            Case {
                label: "optional_string",
                ty: Ty::Union(vec![Ty::String, Ty::Null]),
                ctx: ctx(&[]),
                expected_expr: "string | null",
                expected_imports: &[],
            },
            Case {
                label: "list_int",
                ty: Ty::List(boxed(Ty::Int)),
                ctx: ctx(&[]),
                expected_expr: "number[]",
                expected_imports: &[],
            },
            Case {
                label: "list_optional_string",
                ty: Ty::List(boxed(Ty::Union(vec![Ty::String, Ty::Null]))),
                ctx: ctx(&[]),
                expected_expr: "(string | null)[]",
                expected_imports: &[],
            },
            Case {
                label: "optional_list_string",
                ty: Ty::Union(vec![Ty::List(boxed(Ty::String)), Ty::Null]),
                ctx: ctx(&[]),
                expected_expr: "string[] | null",
                expected_imports: &[],
            },
            Case {
                label: "map_string_int",
                ty: Ty::Map {
                    key: boxed(Ty::String),
                    value: boxed(Ty::Int),
                },
                ctx: ctx(&[]),
                expected_expr: "{ [key: string]: number }",
                expected_imports: &[],
            },
            Case {
                label: "union_three",
                ty: Ty::Union(vec![Ty::Int, Ty::String, Ty::Bool]),
                ctx: ctx(&[]),
                expected_expr: "number | string | boolean",
                expected_imports: &[],
            },
            // ── Name refs (same / cross leaf) ──
            Case {
                label: "class_same_leaf",
                ty: cls("user", &["lorem"], "Resume"),
                ctx: ctx(&["lorem"]),
                expected_expr: "Resume",
                expected_imports: &[],
            },
            Case {
                label: "class_cross_leaf",
                ty: cls("user", &["lorem"], "Resume"),
                ctx: ctx(&["ipsum"]),
                expected_expr: "lorem.Resume",
                expected_imports: &[&["lorem"]],
            },
            Case {
                label: "class_root_from_leaf",
                ty: cls("user", &[], "Foo"),
                ctx: ctx(&["lorem"]),
                expected_expr: "_bamlRoot.Foo",
                expected_imports: &[&[]],
            },
            Case {
                label: "class_root_from_root",
                ty: cls("user", &[], "Foo"),
                ctx: ctx(&[]),
                expected_expr: "Foo",
                expected_imports: &[],
            },
            Case {
                label: "enum_cross_leaf",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["lorem"]),
                expected_expr: "ipsum.Sentiment",
                expected_imports: &[&["ipsum"]],
            },
            Case {
                label: "alias_cross_leaf",
                ty: Ty::TypeAlias(name("user", &["aliases"], "Json")),
                ctx: ctx(&["lorem"]),
                expected_expr: "aliases.Json",
                expected_imports: &[&["aliases"]],
            },
            Case {
                label: "vendor_class",
                ty: cls("aws", &["s3"], "Bucket"),
                ctx: ctx(&["lorem"]),
                expected_expr: "vendor.aws.s3.Bucket",
                expected_imports: &[&["vendor", "aws", "s3"]],
            },
            Case {
                label: "baml_class",
                ty: cls("baml", &["http"], "Response"),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.http.Response",
                expected_imports: &[&["baml", "http"]],
            },
            // spec2: a `$stream` companion lives beside its base type, so
            // from another leaf it reads `lorem.Resume$stream` (not
            // `stream_types.lorem.Resume`), importing the `lorem` leaf.
            Case {
                label: "stream_class_cross_leaf",
                ty: cls("user", &["lorem"], "Resume$stream"),
                ctx: ctx(&["ipsum"]),
                expected_expr: "lorem.Resume$stream",
                expected_imports: &[&["lorem"]],
            },
            // From within its own leaf, a `$stream` companion is a bare
            // same-leaf reference with no import.
            Case {
                label: "stream_class_same_leaf",
                ty: cls("user", &["lorem"], "Resume$stream"),
                ctx: ctx(&["lorem"]),
                expected_expr: "Resume$stream",
                expected_imports: &[],
            },
            // A root-namespace `$stream` companion referenced from a leaf
            // resolves through the root alias, suffix preserved.
            Case {
                label: "stream_class_root_from_leaf",
                ty: cls("user", &[], "Foo$stream"),
                ctx: ctx(&["lorem"]),
                expected_expr: "_bamlRoot.Foo$stream",
                expected_imports: &[&[]],
            },
            Case {
                label: "typevar",
                ty: Ty::TypeVar(BaseName::new("T")),
                ctx: ctx(&[]),
                expected_expr: "T",
                expected_imports: &[],
            },
            // ── Generics ──
            Case {
                label: "generic_same_leaf",
                ty: Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<number>",
                expected_imports: &[],
            },
            Case {
                label: "generic_cross_leaf",
                ty: Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int]),
                ctx: ctx(&["ipsum"]),
                expected_expr: "lorem.Box<number>",
                expected_imports: &[&["lorem"]],
            },
            Case {
                label: "generic_list_arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::List(boxed(Ty::Int))],
                ),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<number[]>",
                expected_imports: &[],
            },
            Case {
                label: "generic_nested",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int])],
                ),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<Box<number>>",
                expected_imports: &[],
            },
            Case {
                label: "generic_typevar_arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::TypeVar(BaseName::new("T"))],
                ),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<T>",
                expected_imports: &[],
            },
            Case {
                label: "map_typevar_value",
                ty: Ty::Map {
                    key: boxed(Ty::String),
                    value: boxed(Ty::TypeVar(BaseName::new("V"))),
                },
                ctx: ctx(&[]),
                expected_expr: "{ [key: string]: V }",
                expected_imports: &[],
            },
            // ── Callable ──
            Case {
                label: "callable_zero",
                ty: Ty::Callable {
                    params: vec![],
                    ret: boxed(Ty::Bool),
                },
                ctx: ctx(&[]),
                expected_expr: "() => boolean",
                expected_imports: &[],
            },
            Case {
                label: "callable_required",
                ty: Ty::Callable {
                    params: vec![callable_param(Ty::Int), callable_param(Ty::String)],
                    ret: boxed(Ty::Bool),
                },
                ctx: ctx(&[]),
                expected_expr: "(arg0: number, arg1: string) => boolean",
                expected_imports: &[],
            },
            Case {
                label: "callable_optional_only",
                ty: Ty::Callable {
                    params: vec![optional_callable_param("x", Ty::Int)],
                    ret: boxed(Ty::Bool),
                },
                ctx: ctx(&[]),
                expected_expr: "($opts?: { x?: number | undefined } | undefined) => boolean",
                expected_imports: &[],
            },
            Case {
                label: "callable_required_and_optional",
                ty: Ty::Callable {
                    params: vec![
                        baml_codegen_types::CallableParam {
                            name: Some(BaseName::new("x")),
                            ty: Ty::Int,
                            mode: CodegenFunctionParamMode::Required,
                        },
                        optional_callable_param("y", Ty::Int),
                    ],
                    ret: boxed(Ty::String),
                },
                ctx: ctx(&[]),
                expected_expr: "(x: number, $opts?: { y?: number | undefined } | undefined) => string",
                expected_imports: &[],
            },
            Case {
                label: "callable_generic_arg",
                ty: Ty::Callable {
                    params: vec![callable_param(Ty::List(boxed(Ty::Int)))],
                    ret: boxed(Ty::Union(vec![Ty::String, Ty::Null])),
                },
                ctx: ctx(&[]),
                expected_expr: "(arg0: number[]) => string | null",
                expected_imports: &[],
            },
        ];

        for case in &cases {
            assert_ty(case);
        }
    }

    #[test]
    fn host_input_callable_accepts_promise_like_only_at_the_call_boundary() {
        let callable = Ty::Callable {
            params: vec![callable_param(Ty::Int)],
            ret: boxed(Ty::Union(vec![Ty::String, Ty::Null])),
        };
        let context = ctx(&[]);

        assert_eq!(
            translate_ty(&callable, &context).expr,
            "(arg0: number) => string | null"
        );
        assert_eq!(
            translate_host_input_ty(&callable, &context).expr,
            "(arg0: number) => string | null | PromiseLike<string | null>"
        );
    }
}
