//! Stream expansion logic and output types.
//!
//! PPIR expansion computes per-field expansion data (`stream_type`, `sap_starts_as`,
//! `sap_in_progress_never`) and per-alias expanded bodies. HIR lowering consumes
//! these to synthesize `stream_*` class and type alias definitions.

use baml_base::Name;
use baml_compiler_syntax::{GreenNode, SyntaxNode};
use smol_str::SmolStr;

use crate::{
    PpirNames,
    ty::{PpirField, PpirTy, PpirTypeAttrs},
};

//
// ──────────────────────────────────────────────── NEW OUTPUT TYPES ─────
//

/// SAP starts-as value, synthesized from `@stream.starts_as` / `@stream.not_null` / defaults.
/// This becomes the `@sap.class_completed_field_missing` and `@sap.class_in_progress_field_missing`
/// attribute values, computed as part of `@stream.*` desugaring.
///
/// Note on the `Explicit` variant: when the user writes `@stream.starts_as(<arg>)`,
/// PPIR expansion does NOT parse `<arg>`. Instead it clones the `SyntaxNode` from the CST
/// and stores it here. HIR lowering is responsible for parsing the
/// `SyntaxNode` into a value expression with full name resolution context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PpirStreamStartsAs {
    /// Field is absent until it begins streaming.
    /// From `@stream.not_null`, `@@stream.done`, or default for literal/never `stream_types`.
    Never,
    /// Default value computed during PPIR expansion from `stream_type`'s syntactic category.
    /// null for scalars, `[]` for lists, `{}` for maps, never for literals.
    DefaultFor(PpirTy),
    /// Explicit `@stream.starts_as(<arg>)`.
    /// `green`: raw CST for text serialization.
    /// `typeof_s`: best-effort inferred type (Never if unrecognizable).
    /// Uses `GreenNode` (not `SyntaxNode`) because Salsa tracked structs require Send+Sync.
    Explicit { green: GreenNode, typeof_s: PpirTy },
}

impl PpirStreamStartsAs {
    /// Extract the type representation for union computation.
    /// Used during HIR lowering as one side of `sap_starts_as_type | stream_type`.
    pub fn as_ty(&self) -> Option<PpirTy> {
        match self {
            PpirStreamStartsAs::Never => Some(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            }),
            PpirStreamStartsAs::DefaultFor(ty) => Some(ty.clone()),
            PpirStreamStartsAs::Explicit { typeof_s, .. } => Some(typeof_s.clone()),
        }
    }
}

/// Per-class desugared results. Carries the original class name (NOT `stream_*`).
/// Used by `expand_cst` to clone-and-transform the original `CLASS_DEF`.
/// `is_dynamic` and `class_stream_done` are detected from the original CST
/// during the transform, so they don't need to be carried here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirDesugaredClass {
    pub name: Name,
    pub fields: Vec<PpirDesugaredField>,
}

/// Per-field desugared results with synthesized `@sap.*` attributes.
/// Carry-through attributes (alias, description, skip) are preserved by
/// cloning the original CST FIELD node during `expand_cst`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirDesugaredField {
    pub name: Name,
    /// The during-streaming type — result of `stream_expand` on the field's type.
    pub stream_type: PpirTy,
    /// `@sap.in_progress(never)` — synthesized from `@stream.done`.
    pub sap_in_progress_never: bool,
    /// Synthesized from `@stream.starts_as` / `@stream.not_null` / defaults.
    /// Becomes `@sap.class_completed_field_missing` and `@sap.class_in_progress_field_missing`.
    pub sap_starts_as: PpirStreamStartsAs,
}

/// Per-alias desugared results. Carries the original alias name (NOT `stream_*`).
/// Used by `expand_cst` to clone-and-transform the original `TYPE_ALIAS_DEF`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirDesugaredTypeAlias {
    pub name: Name,
    /// The result of `stream_expand` on the alias body.
    pub expanded_body: PpirTy,
}

//
// ──────────────────────────────────────────── STREAM EXPAND ─────
//

/// Compute the stream-expanded type from a `PpirTy`.
///
/// Checks `PpirTypeAttrs` before recursing:
/// - `@stream.type(D)`: use D, don't recurse
/// - `@stream.done` (without `stream_type`): use T as-is (atomic)
/// - Otherwise: normal recursive expansion using name classification
pub fn stream_expand(ty: &PpirTy, names: PpirNames<'_>, db: &dyn crate::Db) -> PpirTy {
    let attrs = ty.attrs();

    // Explicit @stream.type(D) — use D directly
    if let Some(d) = &attrs.stream_type {
        return (**d).clone();
    }

    // @stream.done without explicit type — type is atomic, keep as-is
    if attrs.stream_done {
        return ty.clone_without_attrs();
    }

    // Normal recursive expansion (inline name classification via PpirNames)
    match ty {
        PpirTy::Int { .. } | PpirTy::Float { .. } | PpirTy::String { .. } | PpirTy::Bool { .. } => {
            ty.clone_without_attrs()
        }

        PpirTy::Null { .. } => PpirTy::Null {
            attrs: PpirTypeAttrs::default(),
        },
        PpirTy::Never { .. } => PpirTy::Never {
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::StringLiteral { .. } | PpirTy::IntLiteral { .. } | PpirTy::BoolLiteral { .. } => {
            ty.clone_without_attrs()
        }

        // Inline name classification: class/type_alias → stream_*, enum → unchanged
        PpirTy::Named { name, .. } => {
            if names.class_names(db).contains_key(name) || names.type_alias_names(db).contains(name)
            {
                PpirTy::Named {
                    name: SmolStr::new(format!("stream_{name}")),
                    attrs: PpirTypeAttrs::default(),
                }
            } else {
                // Enum or unknown — unchanged
                ty.clone_without_attrs()
            }
        }

        PpirTy::List { inner, .. } => PpirTy::List {
            inner: Box::new(stream_expand(inner, names, db)),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Map { key, value, .. } => PpirTy::Map {
            key: key.clone(),
            value: Box::new(stream_expand(value, names, db)),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Union { variants, .. } => PpirTy::Union {
            variants: variants
                .iter()
                .map(|v| stream_expand(v, names, db))
                .collect(),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Optional { inner, .. } => PpirTy::Union {
            variants: vec![
                stream_expand(inner, names, db),
                PpirTy::Null {
                    attrs: PpirTypeAttrs::default(),
                },
            ],
            attrs: PpirTypeAttrs::default(),
        },

        _ => ty.clone_without_attrs(),
    }
}

//
// ──────────────────────────────────────── DEFAULT SAP STARTS-AS ─────
//

/// Compute the default starts-as value from a field's `stream_type`.
///
/// Per the stream-types spec:
/// - Literal types → never (absent until complete)
/// - Never → never
/// - List → empty list (`list<never>`)
/// - Map → empty map (`map<key, never>`)
/// - Everything else → null
pub fn default_sap_starts_as(stream_type: &PpirTy) -> PpirStreamStartsAs {
    match stream_type {
        PpirTy::StringLiteral { .. }
        | PpirTy::IntLiteral { .. }
        | PpirTy::BoolLiteral { .. }
        | PpirTy::Never { .. } => PpirStreamStartsAs::Never,

        PpirTy::List { .. } => PpirStreamStartsAs::DefaultFor(PpirTy::List {
            inner: Box::new(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            }),
            attrs: PpirTypeAttrs::default(),
        }),

        PpirTy::Map { key, .. } => PpirStreamStartsAs::DefaultFor(PpirTy::Map {
            key: key.clone(),
            value: Box::new(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            }),
            attrs: PpirTypeAttrs::default(),
        }),

        _ => PpirStreamStartsAs::DefaultFor(PpirTy::Null {
            attrs: PpirTypeAttrs::default(),
        }),
    }
}

//
// ──────────────────────────────────────── BUILDING PPIR FIELDS ─────
//

/// Build `PpirField`s for a class by reading the CST class definition.
///
/// Type-level annotations (`@stream.done`, `@stream.type`, `@stream.with_state`)
/// are captured by `PpirTy::from_ast()` on the field's type.
/// Field-level annotations (`@stream.starts_as`, `@stream.not_null`) and
/// carry-through attributes (`@alias`, `@description`, `@skip`) are read
/// from field attributes directly.
///
/// `@@stream.not_null` is handled via cross-type lookup in `desugar_field()`.
/// `@@stream.done` is handled at the class level (not distributed to fields).
pub(crate) fn build_ppir_fields(class_def: &baml_compiler_syntax::ast::ClassDef) -> Vec<PpirField> {
    class_def
        .fields()
        .filter_map(|field_node| {
            let field_name: Name = SmolStr::new(field_node.name()?.text());

            // Parse field type from CST TypeExpr → PpirTy
            // This captures type-level @stream.* annotations via TypeExpr::attributes()
            let ty = field_node
                .ty()
                .map(|te| PpirTy::from_ast(&te))
                .unwrap_or(PpirTy::Unknown {
                    attrs: PpirTypeAttrs::default(),
                });

            // Extract field-level attributes
            let mut starts_as: Option<SyntaxNode> = None;
            let mut not_null = false;

            // Read field-level stream annotations from the TYPE_EXPR.
            // The parser puts ALL @stream.* annotations inside the TYPE_EXPR
            // node (not as direct field children). Type-level annotations
            // (@stream.done, @stream.type, @stream.with_state) are already
            // captured by PpirTy::from_ast(); here we extract the field-level
            // ones (@stream.starts_as, @stream.not_null).
            if let Some(type_expr) = field_node.ty() {
                for attr in type_expr.attributes() {
                    if let Some(attr_name) = attr.full_name() {
                        match attr_name.as_str() {
                            "stream.starts_as" => starts_as = attr.arg_syntax_node(),
                            "stream.not_null" => not_null = true,
                            _ => {}
                        }
                    }
                }
            }

            Some(PpirField {
                name: field_name,
                ty,
                starts_as,
                not_null,
            })
        })
        .collect()
}

//
// ──────────────────────────────────────── EXPANSION ─────
//

/// Desugar a single field's stream annotations into `PpirDesugaredField`.
///
/// Computes `stream_type` via `stream_expand`, synthesizes @sap.* attributes.
pub(crate) fn desugar_field(
    pf: &PpirField,
    names: PpirNames<'_>,
    db: &dyn crate::Db,
) -> PpirDesugaredField {
    // 1. Compute stream_type via stream_expand (respects type-level attrs)
    let stream_type = stream_expand(&pf.ty, names, db);

    // 2. Synthesize @sap.in_progress from @stream.done
    let sap_in_progress_never = pf.ty.attrs().stream_done;

    // 3. Synthesize sap_starts_as from @stream.starts_as / @stream.not_null / defaults
    //    Priority order:
    //    1. Field-level @stream.not_null → Never
    //    2. Explicit @stream.starts_as(value) → Explicit(green)
    //    3. Type-level @@stream.not_null on referenced type → Never
    //    4. Default from stream_type
    let sap_starts_as = if pf.not_null {
        PpirStreamStartsAs::Never
    } else if let Some(starts_as_node) = &pf.starts_as {
        let green = starts_as_node.green().into_owned();
        let text = extract_starts_as_text(&green);
        let starts_as = crate::normalize::parse_starts_as_value(&text);
        let typeof_s = crate::normalize::infer_typeof_s(&starts_as, names.enum_names(db))
            .unwrap_or(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            });
        PpirStreamStartsAs::Explicit { green, typeof_s }
    } else if type_has_block_attr(&pf.ty, "stream.not_null", names, db) {
        PpirStreamStartsAs::Never
    } else {
        default_sap_starts_as(&stream_type)
    };

    PpirDesugaredField {
        name: pf.name.clone(),
        stream_type,
        sap_in_progress_never,
        sap_starts_as,
    }
}

/// Check if the field's top-level type references a class/enum that has
/// a specific @@stream.* block attribute.
///
/// Only matches bare named types (e.g., `Foo`). Does NOT match `Foo[]`, `Foo?`,
/// `Foo | Bar`, etc. — those use their own default `starts_as` behavior.
fn type_has_block_attr(ty: &PpirTy, attr: &str, names: PpirNames<'_>, db: &dyn crate::Db) -> bool {
    let PpirTy::Named { name, .. } = ty else {
        return false;
    };
    let has_attr = |attrs: &Vec<Name>| attrs.iter().any(|a| a == attr);
    names
        .class_names(db)
        .get(name.as_str())
        .is_some_and(has_attr)
        || names
            .enum_names(db)
            .get(name.as_str())
            .is_some_and(has_attr)
}

/// Extract the text content from a `starts_as` `ATTRIBUTE_ARGS` `GreenNode`.
///
/// This mirrors the old `string_arg()` logic: strips quotes, parens, etc.
/// Accepts a `GreenNode` (stored in `PpirSapMissing::Explicit` for Salsa
/// Send+Sync compatibility) and reconstructs a `SyntaxNode` for walking.
pub fn extract_starts_as_text(green: &GreenNode) -> String {
    use baml_compiler_syntax::SyntaxKind;

    let node = SyntaxNode::new_root(green.clone());

    // Try to find a STRING_LITERAL child and extract content
    for child in node.children() {
        match child.kind() {
            SyntaxKind::STRING_LITERAL => {
                let text = child.text().to_string();
                let trimmed = text.trim();
                if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
                    return trimmed[1..trimmed.len() - 1].to_string();
                }
            }
            SyntaxKind::RAW_STRING_LITERAL => {
                let text = child.text().to_string();
                let trimmed = text.trim();
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                if hash_count > 0 {
                    let inner = &trimmed[hash_count..];
                    if inner.starts_with('"') {
                        if let Some(end_pos) = inner.rfind('"') {
                            if end_pos > 0 {
                                return inner[1..end_pos].to_string();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Fallback: collect non-structural tokens
    node.descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|token| {
            !matches!(
                token.kind(),
                SyntaxKind::WHITESPACE
                    | SyntaxKind::NEWLINE
                    | SyntaxKind::LINE_COMMENT
                    | SyntaxKind::BLOCK_COMMENT
                    | SyntaxKind::QUOTE
                    | SyntaxKind::L_PAREN
                    | SyntaxKind::R_PAREN
                    | SyntaxKind::COMMA
            )
        })
        .map(|token| token.text().to_string())
        .collect()
}
