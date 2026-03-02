//! Stream expansion logic and output types.
//!
//! Phase 1 computes per-field expansion data (stream_type, sap_missing,
//! sap_in_progress_never) and per-alias expanded bodies. Phase 3 consumes
//! these to synthesize `stream_*` class and type alias definitions.

use baml_base::{Name, sap::{FieldAttr, SapAttrValue, SapConstValue, TyAttr}};
use baml_compiler_syntax::{GreenNode, SyntaxNode};
use smol_str::SmolStr;

use crate::{
    PpirNames,
    normalize::{StartsAs, StartsAsLiteral},
    ty::{PpirField, PpirTy, PpirTypeAttrs},
};

//
// ──────────────────────────────────────────────── NEW OUTPUT TYPES ─────
//

/// SAP missing value, synthesized from `@stream.starts_as` / `@stream.not_null` / defaults.
/// This is the `@sap.missing` attribute value, computed as part of `@stream.*` desugaring.
/// Phase 3 converts to `FieldAttr { sap_missing: SapAttrValue }`.
///
/// Note on the `Explicit` variant: when the user writes `@stream.starts_as(<arg>)`,
/// Phase 1 does NOT parse `<arg>`. Instead it clones the SyntaxNode from the CST
/// and stores it here. Phase 3 (PPIR → HIR) is responsible for parsing the
/// SyntaxNode into a value expression with full name resolution context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PpirSapMissing {
    /// Field is absent until it begins streaming.
    /// From `@stream.not_null`, `@@stream.done`, or default for literal/never stream_types.
    Never,
    /// Default value computed by Phase 1 from stream_type's syntactic category.
    /// null for scalars, `[]` for lists, `{}` for maps, never for literals.
    Default(PpirTy),
    /// Explicit `@stream.starts_as(<arg>)` — GreenNode cloned from the CST.
    /// Phase 3 reconstructs a SyntaxNode via `SyntaxNode::new_root(green)` and
    /// parses it into a value expression during PPIR → HIR lowering.
    /// Uses GreenNode (not SyntaxNode) because Salsa tracked structs require Send+Sync.
    Explicit(GreenNode),
}

impl PpirSapMissing {
    /// Extract the type representation for union computation.
    /// Phase 3 uses this as one side of `sap_missing_type | stream_type`.
    ///
    /// Returns `None` for `Explicit` — Phase 3 must parse the SyntaxNode first.
    pub fn as_ty(&self) -> Option<PpirTy> {
        match self {
            PpirSapMissing::Never => Some(PpirTy::Never { attrs: PpirTypeAttrs::default() }),
            PpirSapMissing::Default(t) => Some(t.clone()),
            PpirSapMissing::Explicit(_) => None,
        }
    }
}

/// Per-class expansion results. Carries the original class name (NOT `stream_*`).
/// Phase 3 synthesizes the corresponding `stream_*` class definition from this data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirExpandedClass {
    pub name: Name,
    pub fields: Vec<PpirExpandedField>,
    pub is_dynamic: bool,
}

/// Per-field expansion results with synthesized `@sap.*` attributes.
/// Phase 3 uses these to build `stream_*` class fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirExpandedField {
    pub name: Name,
    /// The during-streaming type — result of `stream_expand` on the field's type.
    pub stream_type: PpirTy,
    /// `@sap.in_progress(never)` — synthesized from `@stream.done`.
    pub sap_in_progress_never: bool,
    /// `@sap.missing` — synthesized from `@stream.starts_as` / `@stream.not_null` / defaults.
    pub sap_missing: PpirSapMissing,
    /// From `@stream.with_state` — Phase 4 wraps the final type in `StreamState<T>`.
    pub with_state: bool,
    /// Carry-through attributes.
    pub alias: Option<String>,
    pub description: Option<String>,
    pub skip: bool,
}

/// Per-alias expansion results. Carries the original alias name (NOT `stream_*`).
/// Phase 3 synthesizes `type stream_{name} = expanded_body`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirExpandedTypeAlias {
    pub name: Name,
    /// The result of `stream_expand` on the alias body.
    pub expanded_body: PpirTy,
}

//
// ──────────────────────────────────────────── BRIDGE OUTPUT TYPES ─────
//
// These types are used by ppir_stream_items (the bridge query) to produce
// stream_* class and type alias definitions for HIR consumption.
// They will be removed when Phase 3 takes over synthesis.

/// A generated `stream_*` class (bridge output for HIR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: Name,
    pub fields: Vec<Field>,
    pub is_dynamic: bool,
}

/// A field in a generated `stream_*` class (bridge output for HIR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: Name,
    pub type_ref: PpirTy,
    /// Raw starts_as text, passed through for HIR normalized annotations.
    pub starts_as: Option<String>,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub skip: bool,
    /// SAP field attribute (sap_missing) computed from normalized starts_as.
    pub field_attr: FieldAttr,
    /// SAP type attribute (sap_in_progress) computed from in_progress_never.
    pub ty_attr: TyAttr,
}

/// A generated `stream_*` type alias (bridge output for HIR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Name,
    pub type_ref: PpirTy,
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
pub fn stream_expand(
    ty: &PpirTy,
    names: &PpirNames<'_>,
    db: &dyn crate::Db,
) -> PpirTy {
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
        PpirTy::Int { .. }
        | PpirTy::Float { .. }
        | PpirTy::String { .. }
        | PpirTy::Bool { .. } => ty.clone_without_attrs(),

        PpirTy::Null { .. } => PpirTy::Null { attrs: PpirTypeAttrs::default() },
        PpirTy::Never { .. } => PpirTy::Never { attrs: PpirTypeAttrs::default() },

        PpirTy::StringLiteral { .. }
        | PpirTy::IntLiteral { .. }
        | PpirTy::BoolLiteral { .. } => ty.clone_without_attrs(),

        // Inline name classification: class/type_alias → stream_*, enum → unchanged
        PpirTy::Named { name, .. } => {
            if names.class_names(db).contains(name)
                || names.type_alias_names(db).contains(name)
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
            variants: variants.iter().map(|v| stream_expand(v, names, db)).collect(),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Optional { inner, .. } => PpirTy::Union {
            variants: vec![
                stream_expand(inner, names, db),
                PpirTy::Null { attrs: PpirTypeAttrs::default() },
            ],
            attrs: PpirTypeAttrs::default(),
        },

        _ => ty.clone_without_attrs(),
    }
}

//
// ──────────────────────────────────────── DEFAULT SAP MISSING ─────
//

/// Compute the default `@sap.missing` value from a field's stream_type.
///
/// Per the stream-types spec:
/// - Literal types → never (absent until complete)
/// - Never → never
/// - List → empty list (list<never>)
/// - Map → empty map (map<key, never>)
/// - Everything else → null
pub fn default_sap_missing(stream_type: &PpirTy) -> PpirSapMissing {
    match stream_type {
        PpirTy::StringLiteral { .. }
        | PpirTy::IntLiteral { .. }
        | PpirTy::BoolLiteral { .. }
        | PpirTy::Never { .. } => PpirSapMissing::Never,

        PpirTy::List { .. } => PpirSapMissing::Default(PpirTy::List {
            inner: Box::new(PpirTy::Never { attrs: PpirTypeAttrs::default() }),
            attrs: PpirTypeAttrs::default(),
        }),

        PpirTy::Map { key, .. } => PpirSapMissing::Default(PpirTy::Map {
            key: key.clone(),
            value: Box::new(PpirTy::Never { attrs: PpirTypeAttrs::default() }),
            attrs: PpirTypeAttrs::default(),
        }),

        _ => PpirSapMissing::Default(PpirTy::Null { attrs: PpirTypeAttrs::default() }),
    }
}

/// Compute the default starts-as type (S) from D's type structure.
/// Bridge function: returns PpirTy instead of PpirSapMissing.
pub fn default_starts_as(d: &PpirTy) -> PpirTy {
    match d {
        PpirTy::StringLiteral { .. }
        | PpirTy::IntLiteral { .. }
        | PpirTy::BoolLiteral { .. } => PpirTy::Never { attrs: PpirTypeAttrs::default() },

        PpirTy::Never { .. } => PpirTy::Never { attrs: PpirTypeAttrs::default() },

        PpirTy::List { .. } => PpirTy::List {
            inner: Box::new(PpirTy::Never { attrs: PpirTypeAttrs::default() }),
            attrs: PpirTypeAttrs::default(),
        },
        PpirTy::Map { key, .. } => PpirTy::Map {
            key: key.clone(),
            value: Box::new(PpirTy::Never { attrs: PpirTypeAttrs::default() }),
            attrs: PpirTypeAttrs::default(),
        },

        _ => PpirTy::Null { attrs: PpirTypeAttrs::default() },
    }
}

//
// ──────────────────────────────────────────── MAKE UNION ─────
//

/// Build a union `PpirTy` from S and D, with structural simplification.
///
/// Simplifies:
/// - `never | T → T` and `T | T → T`
/// - `list<never> | list<T> → list<T>` (empty list subsumed by any list)
/// - `map<K, never> | map<K, V> → map<K, V>` (empty map subsumed by any map)
pub(crate) fn make_union(s: PpirTy, d: PpirTy) -> PpirTy {
    if s == d {
        return s;
    }
    match (&s, &d) {
        (PpirTy::Never { .. }, _) => d,
        (_, PpirTy::Never { .. }) => s,
        // list<never> | list<T> → list<T> (empty list subsumed)
        (PpirTy::List { inner: s_inner, .. }, PpirTy::List { .. })
            if matches!(**s_inner, PpirTy::Never { .. }) =>
        {
            d
        }
        (PpirTy::List { .. }, PpirTy::List { inner: d_inner, .. })
            if matches!(**d_inner, PpirTy::Never { .. }) =>
        {
            s
        }
        // map<K, never> | map<K, V> → map<K, V> (empty map subsumed)
        (PpirTy::Map { value: s_val, .. }, PpirTy::Map { .. })
            if matches!(**s_val, PpirTy::Never { .. }) =>
        {
            d
        }
        (PpirTy::Map { .. }, PpirTy::Map { value: d_val, .. })
            if matches!(**d_val, PpirTy::Never { .. }) =>
        {
            s
        }
        _ => PpirTy::union(vec![s, d]),
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
/// Class-level `@@stream.done` and `@@stream.not_null` distribute to all fields.
pub(crate) fn build_ppir_fields(
    class_def: &baml_compiler_syntax::ast::ClassDef,
) -> Vec<PpirField> {
    // Check for class-level @@stream.* block attributes
    let class_stream_done = class_def
        .block_attributes()
        .any(|a| a.full_name().as_deref() == Some("stream.done"));
    let class_stream_not_null = class_def
        .block_attributes()
        .any(|a| a.full_name().as_deref() == Some("stream.not_null"));

    class_def
        .fields()
        .filter_map(|field_node| {
            let field_name: Name = SmolStr::new(field_node.name()?.text());

            // Parse field type from CST TypeExpr → PpirTy
            // This captures type-level @stream.* annotations via TypeExpr::attributes()
            let mut ty = field_node
                .ty()
                .map(|te| PpirTy::from_ast(&te))
                .unwrap_or(PpirTy::Unknown { attrs: PpirTypeAttrs::default() });

            // Distribute @@stream.done to type attrs (lower priority than field-level)
            if class_stream_done && !ty.attrs().stream_done {
                ty.attrs_mut().stream_done = true;
            }

            // Extract field-level attributes
            let mut starts_as: Option<SyntaxNode> = None;
            let mut not_null = class_stream_not_null;
            let mut alias = None;
            let mut description = None;
            let mut skip = false;

            // Read carry-through attributes from field-level (alias, description, skip)
            for attr in field_node.attributes() {
                if let Some(attr_name) = attr.full_name() {
                    match attr_name.as_str() {
                        "alias" => alias = attr.string_arg(),
                        "description" | "desc" => description = attr.string_arg(),
                        "skip" => skip = true,
                        _ => {}
                    }
                }
            }

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
                class_stream_done,
                alias,
                description,
                skip,
            })
        })
        .collect()
}

//
// ──────────────────────────────────────── EXPANSION ─────
//

/// Expand a single field's annotations into PpirExpandedField.
///
/// Computes stream_type via stream_expand, synthesizes @sap.* attributes.
pub(crate) fn expand_field(
    pf: &PpirField,
    names: &PpirNames<'_>,
    db: &dyn crate::Db,
) -> PpirExpandedField {
    // 1. Compute stream_type via stream_expand (respects type-level attrs)
    let stream_type = stream_expand(&pf.ty, names, db);

    // 2. Synthesize @sap.in_progress from @stream.done
    let sap_in_progress_never = pf.ty.attrs().stream_done;

    // 3. Synthesize @sap.missing from @stream.starts_as / @stream.not_null / defaults
    //    @@stream.done distributes starts_as=never at lower priority than explicit
    //    @stream.starts_as. This makes all fields absent until the class appears.
    let sap_missing = if pf.not_null {
        PpirSapMissing::Never
    } else if let Some(starts_as_node) = &pf.starts_as {
        PpirSapMissing::Explicit(starts_as_node.green().into_owned())
    } else if pf.class_stream_done {
        // @@stream.done distributes starts_as=never at low priority
        PpirSapMissing::Never
    } else {
        default_sap_missing(&stream_type)
    };

    PpirExpandedField {
        name: pf.name.clone(),
        stream_type,
        sap_in_progress_never,
        sap_missing,
        with_state: pf.ty.attrs().stream_with_state,
        alias: pf.alias.clone(),
        description: pf.description.clone(),
        skip: pf.skip,
    }
}

//
// ──────────────────────────────────────── BRIDGE SYNTHESIS ─────
//
// These functions synthesize the old output format from PpirExpandedItems
// for the ppir_stream_items bridge query. Will be removed when Phase 3
// takes over synthesis.

/// Synthesize a stream_* class definition from expansion data.
pub(crate) fn synthesize_bridge_class(expanded: &PpirExpandedClass) -> Class {
    let mut stream_fields = Vec::new();

    for ef in &expanded.fields {
        // Compute S type for the union
        let s = match &ef.sap_missing {
            PpirSapMissing::Never => PpirTy::Never { attrs: PpirTypeAttrs::default() },
            PpirSapMissing::Default(ty) => ty.clone(),
            PpirSapMissing::Explicit(green) => {
                // Parse the deferred @stream.starts_as(<arg>) value and compute typeof(S).
                let text = extract_starts_as_text(green);
                let starts_as = crate::normalize::parse_starts_as_value(&text);
                match crate::normalize::infer_typeof_s(&starts_as) {
                    Some(ty) => ty,
                    // EmptyList/EmptyMap: typeof deferred, use Never so simplify gives D.
                    None => PpirTy::Never { attrs: PpirTypeAttrs::default() },
                }
            }
        };

        let d = ef.stream_type.clone();

        // Omit if both S and D are never
        if matches!((&s, &d), (PpirTy::Never { .. }, PpirTy::Never { .. })) {
            continue;
        }

        // Build stream type = simplify(S | D)
        let stream_type_ref = crate::simplify::simplify_union(vec![s, d]);

        // Extract starts_as text for metadata
        let starts_as_text = match &ef.sap_missing {
            PpirSapMissing::Explicit(green) => Some(extract_starts_as_text(green)),
            _ => None,
        };

        stream_fields.push(Field {
            name: ef.name.clone(),
            type_ref: stream_type_ref,
            starts_as: starts_as_text,
            alias: ef.alias.clone(),
            description: ef.description.clone(),
            skip: ef.skip,
            field_attr: compute_field_attr(ef),
            ty_attr: compute_ty_attr(ef),
        });
    }

    Class {
        name: SmolStr::new(format!("stream_{}", expanded.name)),
        fields: stream_fields,
        is_dynamic: expanded.is_dynamic,
    }
}

/// Convert a `StartsAs` value to the corresponding `SapAttrValue` for `sap_missing`.
fn starts_as_to_sap_missing(starts_as: &StartsAs) -> SapAttrValue {
    match starts_as {
        StartsAs::Never => SapAttrValue::Never,
        StartsAs::Null => SapAttrValue::ConstValueExpr(SapConstValue::Null),
        StartsAs::Literal(lit) => SapAttrValue::ConstValueExpr(match lit {
            StartsAsLiteral::String(s) => SapConstValue::String(s.clone()),
            StartsAsLiteral::Int(i) => SapConstValue::Int(*i),
            StartsAsLiteral::Float(f) => SapConstValue::Float(f.clone()),
            StartsAsLiteral::Bool(b) => SapConstValue::Bool(*b),
        }),
        StartsAs::EmptyList => SapAttrValue::ConstValueExpr(SapConstValue::EmptyList),
        StartsAs::EmptyMap => SapAttrValue::ConstValueExpr(SapConstValue::EmptyMap),
    }
}

/// Compute `FieldAttr` (sap_missing) from a `PpirExpandedField`.
///
/// Converts the Phase 1 `PpirSapMissing` into a runtime `FieldAttr`.
/// For `Explicit` nodes, parses the GreenNode text and converts to SAP value.
fn compute_field_attr(ef: &PpirExpandedField) -> FieldAttr {
    let sap_value = match &ef.sap_missing {
        PpirSapMissing::Never => SapAttrValue::Never,
        PpirSapMissing::Default(ty) => {
            let starts_as = crate::normalize::default_starts_as_semantic(ty);
            starts_as_to_sap_missing(&starts_as)
        }
        PpirSapMissing::Explicit(green) => {
            let text = extract_starts_as_text(green);
            let starts_as = crate::normalize::parse_starts_as_value(&text);
            starts_as_to_sap_missing(&starts_as)
        }
    };
    FieldAttr::new(sap_value)
}

/// Compute `TyAttr` (sap_in_progress) from a `PpirExpandedField`.
///
/// When `sap_in_progress_never` is true (from `@stream.done`), sets
/// `sap_in_progress = Never`. Otherwise returns default.
fn compute_ty_attr(ef: &PpirExpandedField) -> TyAttr {
    if ef.sap_in_progress_never {
        TyAttr::new(SapAttrValue::Never, SapAttrValue::DefaultForType)
    } else {
        TyAttr::default()
    }
}

/// Synthesize a stream_* type alias from expansion data.
pub(crate) fn synthesize_bridge_type_alias(expanded: &PpirExpandedTypeAlias) -> TypeAlias {
    TypeAlias {
        name: SmolStr::new(format!("stream_{}", expanded.name)),
        type_ref: expanded.expanded_body.clone(),
    }
}

/// Extract the text content from a starts_as ATTRIBUTE_ARGS GreenNode.
///
/// This mirrors the old `string_arg()` logic: strips quotes, parens, etc.
/// Accepts a `GreenNode` (stored in `PpirSapMissing::Explicit` for Salsa
/// Send+Sync compatibility) and reconstructs a `SyntaxNode` for walking.
pub(crate) fn extract_starts_as_text(green: &GreenNode) -> String {
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
