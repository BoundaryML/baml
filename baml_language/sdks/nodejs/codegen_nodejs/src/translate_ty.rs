//! BAML `Ty` → TypeScript type-expression translation. Pure function;
//! Phase 4 emitters call this from every type position (class fields,
//! function args, return types, type-alias bodies, method signatures).
//!
//! Rule sources:
//! - `00a-spec-codegen-mappings.md` §"Exhaustive TIR Ty conversions"
//! - `03-phase3-plan.md` §"BAML→TS Type Map Table"
//! - Python prior art: `codegen_python/src/translate_ty.rs`

// Phase 3 lands this module; Phase 4 will be the first non-test caller.
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
    /// Namespace leaves (`segments` non-empty) referenced by this
    /// expression. Rendered at the top of the leaf file as
    /// `import * as <alias> from "<rel>";`.
    pub(crate) imports: BTreeSet<LeafPath>,
    /// Bare names from the root leaf referenced by this expression.
    /// Only populated when the current leaf is non-root. Rendered at
    /// the top of the leaf file as `import { Foo } from "..";`.
    pub(crate) root_names: BTreeSet<String>,
}

impl TranslatedType {
    fn bare(expr: impl Into<String>) -> Self {
        Self {
            expr: expr.into(),
            imports: BTreeSet::new(),
            root_names: BTreeSet::new(),
        }
    }
}

pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> TranslatedType {
    match ty {
        Ty::Int => TranslatedType::bare("number"),
        Ty::Float => TranslatedType::bare("number"),
        Ty::String => TranslatedType::bare("string"),
        Ty::Bool => TranslatedType::bare("boolean"),
        Ty::Null => TranslatedType::bare("null"),
        Ty::Uint8Array => TranslatedType::bare("Uint8Array"),
        Ty::BuiltinUnknown => TranslatedType::bare("unknown"),
        Ty::Unit => TranslatedType::bare("null"),
        Ty::BamlOptions => TranslatedType::bare("baml.Options"),
        Ty::RustType => TranslatedType::bare("_BamlHandle"),
        Ty::Literal(Literal::Int(value)) => TranslatedType::bare(value.to_string()),
        Ty::Literal(Literal::String(value)) => TranslatedType::bare(ts_string(value)),
        Ty::Literal(Literal::Bool(true)) => TranslatedType::bare("true"),
        Ty::Literal(Literal::Bool(false)) => TranslatedType::bare("false"),
        Ty::Literal(Literal::Float(_)) => TranslatedType::bare("number"),
        Ty::Media(MediaKind::Image) => media_ref("Image"),
        Ty::Media(MediaKind::Audio) => media_ref("Audio"),
        Ty::Media(MediaKind::Video) => media_ref("Video"),
        Ty::Media(MediaKind::Pdf) => media_ref("Pdf"),
        Ty::Media(MediaKind::Generic) => TranslatedType::bare("unknown"),
        Ty::Class(name, args) => {
            let mut result = render_name_ref(name, ctx);
            if !args.is_empty() {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| {
                        let t = translate_ty(a, ctx);
                        result.imports.extend(t.imports);
                        result.root_names.extend(t.root_names);
                        t.expr
                    })
                    .collect();
                result.expr = format!("{}<{}>", result.expr, arg_strs.join(", "));
            }
            result
        }
        Ty::Enum(name) => render_name_ref(name, ctx),
        Ty::TypeAlias(name) => render_name_ref(name, ctx),
        Ty::TypeVar(name) => TranslatedType::bare(name.as_str().to_string()),
        Ty::Optional(inner) => {
            let inner = translate_ty(inner, ctx);
            TranslatedType {
                expr: format!("{} | null", inner.expr),
                imports: inner.imports,
                root_names: inner.root_names,
            }
        }
        Ty::List(inner) => {
            let inner = translate_ty(inner, ctx);
            TranslatedType {
                expr: format!("Array<{}>", inner.expr),
                imports: inner.imports,
                root_names: inner.root_names,
            }
        }
        Ty::Map { key, value } => {
            let key_t = translate_ty(key, ctx);
            let value_t = translate_ty(value, ctx);
            let mut imports = key_t.imports;
            imports.extend(value_t.imports);
            let mut root_names = key_t.root_names;
            root_names.extend(value_t.root_names);
            // For string-keyed maps, use the index-signature form
            // `{ [key: string]: V }` because TS rejects recursive type
            // aliases that flow through `Record<string, T>`. For
            // non-string keys (enums, literal unions) the index-signature
            // form is illegal (TS1337); fall back to `Record<K, V>` —
            // those keys never appear in recursive aliases.
            let expr = if matches!(**key, Ty::String) {
                format!("{{ [key: string]: {} }}", value_t.expr)
            } else {
                format!("Record<{}, {}>", key_t.expr, value_t.expr)
            };
            TranslatedType {
                expr,
                imports,
                root_names,
            }
        }
        Ty::Union(items) => {
            let mut imports = BTreeSet::new();
            let mut root_names = BTreeSet::new();
            let parts: Vec<String> = items
                .iter()
                .map(|item| {
                    let t = translate_ty(item, ctx);
                    imports.extend(t.imports);
                    root_names.extend(t.root_names);
                    t.expr
                })
                .collect();
            TranslatedType {
                expr: parts.join(" | "),
                imports,
                root_names,
            }
        }
        Ty::Callable { params, ret } => {
            let ret_t = translate_ty(ret, ctx);
            let mut imports = ret_t.imports;
            let mut root_names = ret_t.root_names;
            let any_optional = params
                .iter()
                .any(|p| p.mode == CodegenFunctionParamMode::Optional);
            let expr = if any_optional {
                format!("(...args: unknown[]) => {}", ret_t.expr)
            } else {
                let param_strs: Vec<String> = params
                    .iter()
                    .enumerate()
                    .map(|(idx, p)| {
                        let t = translate_ty(&p.ty, ctx);
                        imports.extend(t.imports);
                        root_names.extend(t.root_names);
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
            TranslatedType {
                expr,
                imports,
                root_names,
            }
        }
    }
}

fn media_ref(bare: &str) -> TranslatedType {
    let name = Name::new(
        baml_base::Name::new("baml"),
        vec![baml_base::Name::new("media")],
        baml_base::Name::new(bare),
    );
    // Media refs always come from `baml.media.*`, which is never the
    // current leaf (codegen wouldn't dispatch through translate_ty if it
    // were). Use the canonical dotted form + add the import.
    let routed = route_class_ref(&name);
    let dotted = routed.segments.join(".");
    let mut imports = BTreeSet::new();
    imports.insert(routed);
    TranslatedType {
        expr: format!("{dotted}.{bare}"),
        imports,
        root_names: BTreeSet::new(),
    }
}

fn render_name_ref(name: &Name, ctx: &TranslateCtx) -> TranslatedType {
    let routed = route_class_ref(name);
    if routed == ctx.current_leaf {
        TranslatedType::bare(name.bare_name().to_string())
    } else if routed.segments.is_empty() {
        // Root-leaf class/enum/alias referenced from a non-root leaf.
        // Emit the bare name and register a named root import so the
        // leaf renderer can add `import { Foo } from "../…";`.
        let mut root_names = BTreeSet::new();
        root_names.insert(name.bare_name().to_string());
        TranslatedType {
            expr: name.bare_name().to_string(),
            imports: BTreeSet::new(),
            root_names,
        }
    } else {
        let dotted = routed.segments.join(".");
        let mut imports = BTreeSet::new();
        imports.insert(routed);
        TranslatedType {
            expr: format!("{dotted}.{}", name.bare_name()),
            imports,
            root_names: BTreeSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::CallableParam;

    use super::*;

    struct Case<'a> {
        label: &'a str,
        ty: Ty,
        ctx: TranslateCtx,
        expected_expr: &'a str,
        expected_imports: Vec<Vec<&'a str>>,
    }

    fn leaf(segments: &[&str]) -> LeafPath {
        LeafPath {
            segments: segments.iter().map(|&s| s.to_string()).collect(),
        }
    }

    fn ctx(segments: &[&str]) -> TranslateCtx {
        TranslateCtx {
            current_leaf: leaf(segments),
        }
    }

    fn name(pkg: &str, ns: &[&str], bare: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(bare),
        )
    }

    fn cparam(ty: Ty) -> CallableParam {
        CallableParam {
            name: None,
            ty,
            mode: CodegenFunctionParamMode::Required,
        }
    }

    fn cparam_optional(n: &str, ty: Ty) -> CallableParam {
        CallableParam {
            name: Some(BaseName::new(n)),
            ty,
            mode: CodegenFunctionParamMode::Optional,
        }
    }

    fn check_exhaustive(ty: &Ty) {
        // Forces this test file to be updated when a new `Ty` variant
        // is added. Compile-time exhaustiveness check, not a runtime one.
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

    fn assert_ty(case: &Case) {
        check_exhaustive(&case.ty);
        let result = translate_ty(&case.ty, &case.ctx);
        assert_eq!(
            result.expr, case.expected_expr,
            "expr mismatch for case `{}`",
            case.label
        );
        let expected_imports: BTreeSet<LeafPath> = case
            .expected_imports
            .iter()
            .map(|segs| LeafPath {
                segments: segs.iter().map(|&s| s.to_string()).collect(),
            })
            .collect();
        assert_eq!(
            result.imports, expected_imports,
            "imports mismatch for case `{}`",
            case.label
        );
    }

    #[test]
    fn translate_ty_covers_phase3_matrix() {
        let cases: Vec<Case> = vec![
            // ── primitives & literals ──
            Case {
                label: "int",
                ty: Ty::Int,
                ctx: ctx(&["lorem"]),
                expected_expr: "number",
                expected_imports: vec![],
            },
            Case {
                label: "float",
                ty: Ty::Float,
                ctx: ctx(&["lorem"]),
                expected_expr: "number",
                expected_imports: vec![],
            },
            Case {
                label: "string",
                ty: Ty::String,
                ctx: ctx(&["lorem"]),
                expected_expr: "string",
                expected_imports: vec![],
            },
            Case {
                label: "bool",
                ty: Ty::Bool,
                ctx: ctx(&["lorem"]),
                expected_expr: "boolean",
                expected_imports: vec![],
            },
            Case {
                label: "null",
                ty: Ty::Null,
                ctx: ctx(&["lorem"]),
                expected_expr: "null",
                expected_imports: vec![],
            },
            Case {
                label: "uint8array",
                ty: Ty::Uint8Array,
                ctx: ctx(&["lorem"]),
                expected_expr: "Uint8Array",
                expected_imports: vec![],
            },
            Case {
                label: "builtin unknown",
                ty: Ty::BuiltinUnknown,
                ctx: ctx(&["lorem"]),
                expected_expr: "unknown",
                expected_imports: vec![],
            },
            Case {
                label: "unit",
                ty: Ty::Unit,
                ctx: ctx(&["lorem"]),
                expected_expr: "null",
                expected_imports: vec![],
            },
            Case {
                label: "baml options",
                ty: Ty::BamlOptions,
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.Options",
                expected_imports: vec![],
            },
            Case {
                label: "literal int",
                ty: Ty::Literal(Literal::Int(42)),
                ctx: ctx(&["lorem"]),
                expected_expr: "42",
                expected_imports: vec![],
            },
            Case {
                label: "literal negative int",
                ty: Ty::Literal(Literal::Int(-1)),
                ctx: ctx(&["lorem"]),
                expected_expr: "-1",
                expected_imports: vec![],
            },
            Case {
                label: "literal string",
                ty: Ty::Literal(Literal::String("draft".into())),
                ctx: ctx(&["lorem"]),
                expected_expr: "\"draft\"",
                expected_imports: vec![],
            },
            Case {
                label: "literal escaped string",
                ty: Ty::Literal(Literal::String("has \"quotes\"".into())),
                ctx: ctx(&["lorem"]),
                expected_expr: "\"has \\\"quotes\\\"\"",
                expected_imports: vec![],
            },
            Case {
                label: "literal bool true",
                ty: Ty::Literal(Literal::Bool(true)),
                ctx: ctx(&["lorem"]),
                expected_expr: "true",
                expected_imports: vec![],
            },
            Case {
                label: "literal bool false",
                ty: Ty::Literal(Literal::Bool(false)),
                ctx: ctx(&["lorem"]),
                expected_expr: "false",
                expected_imports: vec![],
            },
            Case {
                label: "literal float fallback",
                ty: Ty::Literal(Literal::Float("3.14".into())),
                ctx: ctx(&["lorem"]),
                expected_expr: "number",
                expected_imports: vec![],
            },
            // ── media ──
            Case {
                label: "media image",
                ty: Ty::Media(MediaKind::Image),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Image",
                expected_imports: vec![vec!["baml", "media"]],
            },
            Case {
                label: "media audio",
                ty: Ty::Media(MediaKind::Audio),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Audio",
                expected_imports: vec![vec!["baml", "media"]],
            },
            Case {
                label: "media video",
                ty: Ty::Media(MediaKind::Video),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Video",
                expected_imports: vec![vec!["baml", "media"]],
            },
            Case {
                label: "media pdf",
                ty: Ty::Media(MediaKind::Pdf),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Pdf",
                expected_imports: vec![vec!["baml", "media"]],
            },
            Case {
                label: "media generic fallback",
                ty: Ty::Media(MediaKind::Generic),
                ctx: ctx(&["lorem"]),
                expected_expr: "unknown",
                expected_imports: vec![],
            },
            // ── class / enum / type alias ──
            Case {
                label: "class same leaf root namespace",
                ty: Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Resume",
                expected_imports: vec![],
            },
            Case {
                label: "class cross leaf root namespace",
                ty: Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["ipsum"]),
                expected_expr: "lorem.Resume",
                expected_imports: vec![vec!["lorem"]],
            },
            Case {
                label: "class same leaf root init",
                ty: Ty::Class(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&[]),
                expected_expr: "Foo",
                expected_imports: vec![],
            },
            Case {
                label: "class root init from namespaced leaf",
                ty: Ty::Class(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Foo",
                expected_imports: vec![],
            },
            Case {
                label: "class vendor cross leaf",
                ty: Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "vendor.aws.s3.Bucket",
                expected_imports: vec![vec!["vendor", "aws", "s3"]],
            },
            Case {
                label: "class vendor same leaf",
                ty: Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["vendor", "aws", "s3"]),
                expected_expr: "Bucket",
                expected_imports: vec![],
            },
            Case {
                label: "class vendor other vendor leaf",
                ty: Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                ctx: ctx(&["vendor", "aws", "ec2"]),
                expected_expr: "vendor.aws.s3.Bucket",
                expected_imports: vec![vec!["vendor", "aws", "s3"]],
            },
            Case {
                label: "class stdlib cross leaf",
                ty: Ty::Class(name("baml", &["http"], "Response"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.http.Response",
                expected_imports: vec![vec!["baml", "http"]],
            },
            Case {
                label: "class stdlib same leaf",
                ty: Ty::Class(name("baml", &["http"], "Response"), vec![]),
                ctx: ctx(&["baml", "http"]),
                expected_expr: "Response",
                expected_imports: vec![],
            },
            Case {
                label: "class stream from non stream leaf",
                ty: Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "stream_types.lorem.Resume",
                expected_imports: vec![vec!["stream_types", "lorem"]],
            },
            Case {
                label: "class stream same leaf",
                ty: Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]),
                ctx: ctx(&["stream_types", "lorem"]),
                expected_expr: "Resume",
                expected_imports: vec![],
            },
            Case {
                label: "class non stream from stream leaf",
                ty: Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                ctx: ctx(&["stream_types", "lorem"]),
                expected_expr: "lorem.Resume",
                expected_imports: vec![vec!["lorem"]],
            },
            Case {
                label: "enum same leaf",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["ipsum"]),
                expected_expr: "Sentiment",
                expected_imports: vec![],
            },
            Case {
                label: "enum cross leaf",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["lorem"]),
                expected_expr: "ipsum.Sentiment",
                expected_imports: vec![vec!["ipsum"]],
            },
            Case {
                label: "type alias same leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "StringList")),
                ctx: ctx(&["util"]),
                expected_expr: "StringList",
                expected_imports: vec![],
            },
            Case {
                label: "type alias cross leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "StringList")),
                ctx: ctx(&["lorem"]),
                expected_expr: "util.StringList",
                expected_imports: vec![vec!["util"]],
            },
            // ── containers ──
            Case {
                label: "optional string",
                ty: Ty::Optional(Box::new(Ty::String)),
                ctx: ctx(&["lorem"]),
                expected_expr: "string | null",
                expected_imports: vec![],
            },
            Case {
                label: "list int",
                ty: Ty::List(Box::new(Ty::Int)),
                ctx: ctx(&["lorem"]),
                expected_expr: "Array<number>",
                expected_imports: vec![],
            },
            Case {
                label: "map string int",
                ty: Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(Ty::Int),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "{ [key: string]: number }",
                expected_imports: vec![],
            },
            Case {
                label: "map enum to class",
                ty: Ty::Map {
                    key: Box::new(Ty::Enum(name("user", &["ipsum"], "Sentiment"))),
                    value: Box::new(Ty::Class(name("user", &["lorem"], "Resume"), vec![])),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "Record<ipsum.Sentiment, Resume>",
                expected_imports: vec![vec!["ipsum"]],
            },
            Case {
                label: "union int string",
                ty: Ty::Union(vec![Ty::Int, Ty::String]),
                ctx: ctx(&["lorem"]),
                expected_expr: "number | string",
                expected_imports: vec![],
            },
            Case {
                label: "union int string bool",
                ty: Ty::Union(vec![Ty::Int, Ty::String, Ty::Bool]),
                ctx: ctx(&["lorem"]),
                expected_expr: "number | string | boolean",
                expected_imports: vec![],
            },
            Case {
                label: "optional list same leaf class",
                ty: Ty::Optional(Box::new(Ty::List(Box::new(Ty::Class(
                    name("user", &["lorem"], "Resume"),
                    vec![],
                ))))),
                ctx: ctx(&["lorem"]),
                expected_expr: "Array<Resume> | null",
                expected_imports: vec![],
            },
            Case {
                label: "list optional string",
                ty: Ty::List(Box::new(Ty::Optional(Box::new(Ty::String)))),
                ctx: ctx(&["lorem"]),
                expected_expr: "Array<string | null>",
                expected_imports: vec![],
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
                expected_expr: "{ [key: string]: Array<vendor.aws.s3.Bucket> }",
                expected_imports: vec![vec!["vendor", "aws", "s3"]],
            },
            Case {
                label: "optional media",
                ty: Ty::Optional(Box::new(Ty::Media(MediaKind::Image))),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.media.Image | null",
                expected_imports: vec![vec!["baml", "media"]],
            },
            Case {
                label: "optional stdlib class",
                ty: Ty::Optional(Box::new(Ty::Class(
                    name("baml", &["http"], "Response"),
                    vec![],
                ))),
                ctx: ctx(&["lorem"]),
                expected_expr: "baml.http.Response | null",
                expected_imports: vec![vec!["baml", "http"]],
            },
            Case {
                label: "list vendor class",
                ty: Ty::List(Box::new(Ty::Class(name("aws", &["s3"], "Bucket"), vec![]))),
                ctx: ctx(&["lorem"]),
                expected_expr: "Array<vendor.aws.s3.Bucket>",
                expected_imports: vec![vec!["vendor", "aws", "s3"]],
            },
            Case {
                label: "map enum to stream vendor class",
                ty: Ty::Map {
                    key: Box::new(Ty::Enum(name("user", &["ipsum"], "Sentiment"))),
                    value: Box::new(Ty::Class(name("aws", &["s3"], "Bucket$stream"), vec![])),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "Record<ipsum.Sentiment, stream_types.vendor.aws.s3.Bucket>",
                expected_imports: vec![vec!["ipsum"], vec!["stream_types", "vendor", "aws", "s3"]],
            },
            Case {
                label: "union across placements",
                ty: Ty::Union(vec![
                    Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                    Ty::Class(name("aws", &["s3"], "Bucket"), vec![]),
                    Ty::Class(name("baml", &["http"], "Response"), vec![]),
                ]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Resume | vendor.aws.s3.Bucket | baml.http.Response",
                expected_imports: vec![vec!["vendor", "aws", "s3"], vec!["baml", "http"]],
            },
            Case {
                label: "union stream and non stream classes",
                ty: Ty::Union(vec![
                    Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
                    Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]),
                ]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Resume | stream_types.lorem.Resume",
                expected_imports: vec![vec!["stream_types", "lorem"]],
            },
            // ── callable ──
            Case {
                label: "callable two params",
                ty: Ty::Callable {
                    params: vec![cparam(Ty::Int), cparam(Ty::String)],
                    ret: Box::new(Ty::Bool),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "(arg0: number, arg1: string) => boolean",
                expected_imports: vec![],
            },
            Case {
                label: "callable no params",
                ty: Ty::Callable {
                    params: vec![],
                    ret: Box::new(Ty::Unit),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "() => null",
                expected_imports: vec![],
            },
            Case {
                label: "callable nested params",
                ty: Ty::Callable {
                    params: vec![cparam(Ty::List(Box::new(Ty::Int)))],
                    ret: Box::new(Ty::Optional(Box::new(Ty::String))),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "(arg0: Array<number>) => string | null",
                expected_imports: vec![],
            },
            Case {
                label: "callable optional params",
                ty: Ty::Callable {
                    params: vec![cparam(Ty::String), cparam_optional("limit", Ty::Int)],
                    ret: Box::new(Ty::Bool),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "(...args: unknown[]) => boolean",
                expected_imports: vec![],
            },
            // ── generics ──
            Case {
                label: "generic class same leaf concrete int",
                ty: Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<number>",
                expected_imports: vec![],
            },
            Case {
                label: "generic class cross leaf concrete int",
                ty: Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int]),
                ctx: ctx(&["ipsum"]),
                expected_expr: "lorem.Box<number>",
                expected_imports: vec![vec!["lorem"]],
            },
            Case {
                label: "generic class with list arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::List(Box::new(Ty::Int))],
                ),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<Array<number>>",
                expected_imports: vec![],
            },
            Case {
                label: "generic class nested generic arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::Class(name("user", &["lorem"], "Box"), vec![Ty::Int])],
                ),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<Box<number>>",
                expected_imports: vec![],
            },
            Case {
                label: "generic class stream from non-stream leaf",
                ty: Ty::Class(name("user", &["lorem"], "Box$stream"), vec![Ty::Int]),
                ctx: ctx(&["lorem"]),
                expected_expr: "stream_types.lorem.Box<number>",
                expected_imports: vec![vec!["stream_types", "lorem"]],
            },
            Case {
                label: "generic class with typevar arg",
                ty: Ty::Class(
                    name("user", &["lorem"], "Box"),
                    vec![Ty::TypeVar(BaseName::new("T"))],
                ),
                ctx: ctx(&["lorem"]),
                expected_expr: "Box<T>",
                expected_imports: vec![],
            },
            Case {
                label: "bare typevar",
                ty: Ty::TypeVar(BaseName::new("T")),
                ctx: ctx(&["lorem"]),
                expected_expr: "T",
                expected_imports: vec![],
            },
            Case {
                label: "map with typevar key and value",
                ty: Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(Ty::TypeVar(BaseName::new("V"))),
                },
                ctx: ctx(&["lorem"]),
                expected_expr: "{ [key: string]: V }",
                expected_imports: vec![],
            },
            // ── recursive / self-ref (TS handles natively, no quoting) ──
            Case {
                label: "recursive alias self ref",
                ty: Ty::TypeAlias(name("user", &["util"], "RecList")),
                ctx: ctx(&["util"]),
                expected_expr: "RecList",
                expected_imports: vec![],
            },
            Case {
                label: "self-ref class no args",
                ty: Ty::Class(name("user", &["lorem"], "Node"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Node",
                expected_imports: vec![],
            },
            Case {
                label: "self-ref generic class — no quoting in TS",
                ty: Ty::Class(name("user", &["lorem"], "Node"), vec![Ty::String]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Node<string>",
                expected_imports: vec![],
            },
            Case {
                label: "self-ref generic class nested in list — no quoting in TS",
                ty: Ty::List(Box::new(Ty::Class(
                    name("user", &["lorem"], "Node"),
                    vec![Ty::Int],
                ))),
                ctx: ctx(&["lorem"]),
                expected_expr: "Array<Node<number>>",
                expected_imports: vec![],
            },
            Case {
                label: "recursive alias inside list — no quoting in TS",
                ty: Ty::List(Box::new(Ty::TypeAlias(name("user", &["util"], "RecList")))),
                ctx: ctx(&["util"]),
                expected_expr: "Array<RecList>",
                expected_imports: vec![],
            },
            Case {
                label: "recursive alias inside union — no quoting in TS",
                ty: Ty::Union(vec![
                    Ty::Int,
                    Ty::List(Box::new(Ty::TypeAlias(name("user", &["util"], "RecList")))),
                ]),
                ctx: ctx(&["util"]),
                expected_expr: "number | Array<RecList>",
                expected_imports: vec![],
            },
            Case {
                label: "recursive body same-leaf sibling — no quoting in TS",
                ty: Ty::List(Box::new(Ty::Class(
                    name("user", &["util"], "Other"),
                    vec![],
                ))),
                ctx: ctx(&["util"]),
                expected_expr: "Array<Other>",
                expected_imports: vec![],
            },
            Case {
                label: "recursive body cross-leaf class — no forward-ref in TS",
                ty: Ty::List(Box::new(Ty::Class(name("user", &["util"], "Bar"), vec![]))),
                ctx: ctx(&["lorem"]),
                expected_expr: "Array<util.Bar>",
                expected_imports: vec![vec!["util"]],
            },
            Case {
                label: "recursive body root-routed name — no forward-ref in TS",
                ty: Ty::Class(name("user", &[], "Foo"), vec![]),
                ctx: ctx(&["lorem"]),
                expected_expr: "Foo",
                expected_imports: vec![],
            },
            Case {
                label: "recursive body cross-leaf enum — no forward-ref in TS",
                ty: Ty::Enum(name("user", &["ipsum"], "Sentiment")),
                ctx: ctx(&["lorem"]),
                expected_expr: "ipsum.Sentiment",
                expected_imports: vec![vec!["ipsum"]],
            },
            Case {
                label: "non recursive alias same leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "RecList")),
                ctx: ctx(&["util"]),
                expected_expr: "RecList",
                expected_imports: vec![],
            },
            Case {
                label: "non recursive alias cross leaf",
                ty: Ty::TypeAlias(name("user", &["util"], "RecList")),
                ctx: ctx(&["lorem"]),
                expected_expr: "util.RecList",
                expected_imports: vec![vec!["util"]],
            },
        ];

        for case in &cases {
            assert_ty(case);
        }
    }

    #[test]
    fn root_class_from_namespaced_leaf_collects_root_name() {
        // Regression: previously emitted `Foo` bare with no import, so
        // tsc errored `Cannot find name 'Foo'` after Phase 4 added the
        // `as (f: Foo) => Foo` typed assertion to function bindings.
        let ty = Ty::Class(name("user", &[], "Foo"), vec![]);
        let result = translate_ty(&ty, &ctx(&["lorem"]));
        assert_eq!(result.expr, "Foo");
        assert!(result.imports.is_empty(), "{:?}", result.imports);
        let expected: BTreeSet<String> = ["Foo".to_string()].into();
        assert_eq!(result.root_names, expected);
    }

    #[test]
    fn root_class_from_root_leaf_does_not_collect_root_name() {
        let ty = Ty::Class(name("user", &[], "Foo"), vec![]);
        let result = translate_ty(&ty, &ctx(&[]));
        assert_eq!(result.expr, "Foo");
        assert!(result.imports.is_empty());
        assert!(result.root_names.is_empty());
    }
}
