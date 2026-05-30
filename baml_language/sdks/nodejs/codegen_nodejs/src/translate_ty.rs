//! BAML `Ty` → TypeScript type-expression translation. Pure function;
//! Phase 4 emitters call this from every type position (class fields,
//! function args, return types, type-alias bodies, method signatures).
//!
//! Rule sources:
//! - `00a-spec-codegen-mappings.md` §"Exhaustive Ty conversions"
//! - Python prior art: `codegen_python/src/translate_ty.rs`
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
//! Phase 4 wires this into the emitters; until then the public surface is
//! exercised only by the unit tests, hence the module-level allow.
#![allow(dead_code)]

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
        Ty::BamlOptions => TranslatedType::bare("baml.Options"),
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

        Ty::Optional(inner) => {
            let inner = translate_ty(inner, ctx);
            TranslatedType {
                expr: format!("{} | null", inner.expr),
                imports: inner.imports,
            }
        }
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
            TranslatedType {
                expr: format!("Record<{}, {}>", key.expr, value.expr),
                imports,
            }
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

        Ty::Callable { params, ret } => {
            let ret_t = translate_ty(ret, ctx);
            let mut imports = ret_t.imports;
            let any_optional = params
                .iter()
                .any(|p| p.mode == CodegenFunctionParamMode::Optional);
            let expr = if any_optional {
                // TS can't express per-param optionality in a function type
                // without naming each param; collapse to a rest-args type
                // (mirrors Python's `Callable[..., R]` fallback).
                format!("(...args: unknown[]) => {}", ret_t.expr)
            } else {
                let param_strs: Vec<String> = params
                    .iter()
                    .enumerate()
                    .map(|(idx, p)| {
                        let t = translate_ty(&p.ty, ctx);
                        imports.extend(t.imports);
                        let arg_name = p
                            .name
                            .as_ref()
                            .map(|n| n.as_str().to_string())
                            .unwrap_or_else(|| format!("arg{idx}"));
                        format!("{arg_name}: {}", t.expr)
                    })
                    .collect();
                format!("({}) => {}", param_strs.join(", "), ret_t.expr)
            };
            TranslatedType { expr, imports }
        }
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

/// Render a class/enum/alias name reference. Same leaf or root-routed →
/// bare name, no import. Cross-leaf → root-relative dotted path
/// (`lorem.Resume`, `vendor.aws.s3.Bucket`, `stream_types.lorem.Resume`)
/// plus the routed `LeafPath` in `imports`.
fn render_name_ref(name: &Name, ctx: &TranslateCtx) -> TranslatedType {
    let routed = route_class_ref(name);
    if routed == ctx.current_leaf || routed.segments.is_empty() {
        TranslatedType::bare(name.bare_name().to_string())
    } else {
        let dotted = routed.segments.join(".");
        let mut imports = BTreeSet::new();
        imports.insert(routed);
        TranslatedType {
            expr: format!("{dotted}.{}", name.bare_name()),
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
            | Ty::Optional(_)
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
                expected_imports: &[],
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
                ty: Ty::Optional(boxed(Ty::String)),
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
                ty: Ty::List(boxed(Ty::Optional(boxed(Ty::String)))),
                ctx: ctx(&[]),
                expected_expr: "(string | null)[]",
                expected_imports: &[],
            },
            Case {
                label: "optional_list_string",
                ty: Ty::Optional(boxed(Ty::List(boxed(Ty::String)))),
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
                expected_expr: "Record<string, number>",
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
            Case {
                label: "stream_class_from_user",
                ty: cls("user", &["lorem"], "Resume$stream"),
                ctx: ctx(&["lorem"]),
                expected_expr: "stream_types.lorem.Resume",
                expected_imports: &[&["stream_types", "lorem"]],
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
                expected_expr: "Record<string, V>",
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
                label: "callable_optional_fallback",
                ty: Ty::Callable {
                    params: vec![optional_callable_param("x", Ty::Int)],
                    ret: boxed(Ty::Bool),
                },
                ctx: ctx(&[]),
                expected_expr: "(...args: unknown[]) => boolean",
                expected_imports: &[],
            },
            Case {
                label: "callable_generic_arg",
                ty: Ty::Callable {
                    params: vec![callable_param(Ty::List(boxed(Ty::Int)))],
                    ret: boxed(Ty::Optional(boxed(Ty::String))),
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
}
