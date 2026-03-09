use baml_base::{FileId, SourceFile, Span};
use baml_compiler_diagnostics::HirDiagnostic;
use baml_compiler_parser::{parse_errors, syntax_tree};
use baml_compiler_syntax::{
    SyntaxKind, TypeExpr,
    ast::{Attribute, BlockAttribute, ClassDef, EnumDef, TypeAliasDef},
};
use rowan::{TextRange, ast::AstNode as _};

/// Valid `@stream.*` type-level attributes (used in diagnostic hints).
const STREAM_TYPE_ATTRS: &[&str] = &[
    "stream.done",
    "stream.not_null",
    "stream.starts_as",
    "stream.type",
    "stream.with_state",
];

/// Valid `@@stream.*` block-level attributes (used in diagnostic hints).
const STREAM_BLOCK_ATTRS: &[&str] = &["stream.done", "stream.not_null"];

// ── local trait to unify Attribute / BlockAttribute span queries ──────────────

/// Abstracts the span operations shared by [`Attribute`] and [`BlockAttribute`].
///
/// Both types expose identical `full_name_range`, `name`, and `args_span`
/// methods but share no common trait in `baml_compiler_syntax`. This sealed
/// trait lets us write the span helpers once.
trait AttrSpans {
    fn full_name_range(&self) -> Option<TextRange>;
    fn name_token_range(&self) -> Option<TextRange>;
    fn args_span(&self) -> Option<TextRange>;

    fn best_span_range(&self) -> Option<TextRange> {
        self.args_span()
            .or_else(|| self.full_name_range())
            .or_else(|| self.name_token_range())
    }
}

impl AttrSpans for Attribute {
    fn full_name_range(&self) -> Option<TextRange> {
        self.full_name_range()
    }
    fn name_token_range(&self) -> Option<TextRange> {
        self.name().map(|t| t.text_range())
    }
    fn args_span(&self) -> Option<TextRange> {
        self.args_span()
    }
}

impl AttrSpans for BlockAttribute {
    fn full_name_range(&self) -> Option<TextRange> {
        self.full_name_range()
    }
    fn name_token_range(&self) -> Option<TextRange> {
        self.name().map(|t| t.text_range())
    }
    fn args_span(&self) -> Option<TextRange> {
        self.args_span()
    }
}

// ── span helpers (one implementation, works for both attribute kinds) ─────────

fn name_span<A: AttrSpans>(attr: &A, file_id: FileId) -> Option<Span> {
    attr.full_name_range()
        .or_else(|| attr.name_token_range())
        .map(|r| Span::new(file_id, r))
}

fn best_span<A: AttrSpans>(attr: &A, file_id: FileId) -> Option<Span> {
    attr.best_span_range().map(|r| Span::new(file_id, r))
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn ppir_stream_diagnostics(db: &dyn crate::Db, file: SourceFile) -> Vec<HirDiagnostic> {
    if file.is_virtual(db) || !parse_errors(db, file).is_empty() {
        return Vec::new();
    }

    let tree = syntax_tree(db, file);
    let file_id = file.file_id(db);
    let mut diagnostics = Vec::new();

    for child in tree.children() {
        match child.kind() {
            SyntaxKind::CLASS_DEF => {
                if let Some(def) = ClassDef::cast(child) {
                    validate_class(&def, file_id, &mut diagnostics);
                }
            }
            SyntaxKind::ENUM_DEF => {
                if let Some(def) = EnumDef::cast(child) {
                    validate_block_stream_attrs(def.block_attributes(), file_id, &mut diagnostics);
                }
            }
            SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(def) = TypeAliasDef::cast(child) {
                    if let Some(ty) = def.ty() {
                        validate_stream_type_expr(&ty, file_id, &mut diagnostics);
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

// ── per-definition validators ─────────────────────────────────────────────────

fn validate_class(def: &ClassDef, file_id: FileId, out: &mut Vec<HirDiagnostic>) {
    validate_block_stream_attrs(def.block_attributes(), file_id, out);
    for field in def.fields() {
        if let Some(ty) = field.ty() {
            validate_stream_type_expr(&ty, file_id, out);
        }
    }
}

fn validate_stream_type_expr(type_expr: &TypeExpr, file_id: FileId, out: &mut Vec<HirDiagnostic>) {
    let mut not_null_span: Option<Span> = None;
    let mut starts_as_span: Option<Span> = None;
    let mut done_span: Option<Span> = None;
    let mut type_span: Option<Span> = None;

    for attr in type_expr.attributes() {
        let Some(attr_name) = attr.full_name() else {
            continue;
        };
        let key = attr_name.as_str();

        if !key.starts_with("stream.") {
            continue;
        }

        if !STREAM_TYPE_ATTRS.contains(&key) {
            if let Some(span) = name_span(&attr, file_id) {
                out.push(HirDiagnostic::UnknownAttribute {
                    attr_name: key.to_owned(),
                    span,
                    valid_attributes: STREAM_TYPE_ATTRS,
                });
            }
            continue;
        }

        let span = name_span(&attr, file_id);

        match key {
            "stream.type" => {
                require_single_arg(&attr, key, file_id, out);
                if type_span.is_none() {
                    type_span = span;
                }
            }
            "stream.starts_as" => {
                require_single_arg(&attr, key, file_id, out);
                if starts_as_span.is_none() {
                    starts_as_span = span;
                }
            }
            "stream.not_null" => {
                forbid_args(&attr, key, file_id, out);
                if not_null_span.is_none() {
                    not_null_span = span;
                }
            }
            "stream.done" => {
                forbid_args(&attr, key, file_id, out);
                if done_span.is_none() {
                    done_span = span;
                }
            }
            "stream.with_state" => {
                forbid_args(&attr, key, file_id, out);
            }
            _ => {}
        }
    }

    if let (Some(first), Some(second)) = (not_null_span, starts_as_span) {
        out.push(HirDiagnostic::ConflictingStreamAttributes {
            first_attr: "stream.not_null",
            second_attr: "stream.starts_as",
            first_span: first,
            second_span: second,
        });
    }

    if let (Some(first), Some(second)) = (done_span, type_span) {
        out.push(HirDiagnostic::ConflictingStreamAttributes {
            first_attr: "stream.done",
            second_attr: "stream.type",
            first_span: first,
            second_span: second,
        });
    }
}

fn validate_block_stream_attrs(
    attrs: impl Iterator<Item = BlockAttribute>,
    file_id: FileId,
    out: &mut Vec<HirDiagnostic>,
) {
    for attr in attrs {
        let Some(attr_name) = attr.full_name() else {
            continue;
        };
        let key = attr_name.as_str();

        if !key.starts_with("stream.") {
            continue;
        }

        if !STREAM_BLOCK_ATTRS.contains(&key) {
            if let Some(span) = name_span(&attr, file_id) {
                out.push(HirDiagnostic::UnknownAttribute {
                    attr_name: key.to_owned(),
                    span,
                    valid_attributes: STREAM_BLOCK_ATTRS,
                });
            }
            continue;
        }

        if attr.has_args() {
            if let Some(span) = best_span(&attr, file_id) {
                out.push(HirDiagnostic::UnexpectedAttributeArg {
                    attr_name: key.to_owned(),
                    span,
                });
            }
        }
    }
}

// ── argument validators ───────────────────────────────────────────────────────

fn forbid_args(attr: &Attribute, name: &str, file_id: FileId, out: &mut Vec<HirDiagnostic>) {
    if attr.has_args() {
        if let Some(span) = best_span(attr, file_id) {
            out.push(HirDiagnostic::UnexpectedAttributeArg {
                attr_name: name.to_owned(),
                span,
            });
        }
    }
}

fn require_single_arg(attr: &Attribute, name: &str, file_id: FileId, out: &mut Vec<HirDiagnostic>) {
    if attr.arg_count() != 1 {
        if let Some(span) = best_span(attr, file_id) {
            out.push(HirDiagnostic::InvalidAttributeArg {
                attr_name: name.to_owned(),
                span,
                received: describe_args(attr),
            });
        }
    }
}

// ── arg description ───────────────────────────────────────────────────────────

fn describe_args(attr: &Attribute) -> String {
    match attr.arg_count() {
        0 => "no arguments".to_owned(),
        1 => attr
            .args()
            .next()
            .map(|arg| match arg.kind() {
                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                    format!("`{}`", arg.text())
                }
                SyntaxKind::EXPR | SyntaxKind::UNQUOTED_STRING => {
                    format!("an expression `{}`", arg.text())
                }
                _ => "an unknown value".to_owned(),
            })
            .unwrap_or_else(|| "an unknown value".to_owned()),
        n => format!("{n} arguments"),
    }
}
