//! Stream expansion logic: synthesize `stream_*` classes and type aliases.

use baml_base::Name;
use smol_str::SmolStr;

use crate::{
    PpirNames,
    cst_extract::StreamAttrs,
    ty::{ClassifiedField, Ty, PpirTypeRef},
};

//
// ──────────────────────────────────────────────────── OUTPUT TYPES ─────
//

/// A generated `stream_*` class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: Name,
    pub fields: Vec<Field>,
    pub is_dynamic: bool,
}

/// A field in a generated `stream_*` class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: Name,
    pub type_ref: PpirTypeRef,
    /// Raw CST value expression from @stream.starts_as(...), passed through to HIR.
    pub starts_as: Option<String>,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub skip: bool,
}

/// A generated `stream_*` type alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Name,
    pub type_ref: PpirTypeRef,
}

//
// ──────────────────────────────────────────────────── DESUGARING ─────
//

/// Desugar @stream.* attributes into canonical (`stream_type`, `has_completed`) form.
///
/// - @stream.done → @stream.type(T) + `has_completed` flag
///
/// Note: `starts_as` is a raw CST value expression passed through to HIR.
/// `@stream.not_null` affects the type (S=never) and is handled directly
/// in `expand_stream_class`.
pub fn desugar_stream_attrs(field: &ClassifiedField) -> (Option<PpirTypeRef>, bool) {
    let mut stream_type = field.stream_type.clone();
    let mut has_completed = false;

    // @stream.done → the field's original type (not stream-expanded)
    if field.stream_done {
        if stream_type.is_none() {
            stream_type = Some(field.type_ref.clone());
        }
        has_completed = true;
    }

    (stream_type, has_completed)
}

/// Compute the default starts-as value (S) based on D's type structure.
///
/// S is derived from D's category, not the original T's category.
pub fn default_starts_as(d: &PpirTypeRef) -> PpirTypeRef {
    match d {
        // Literals: S = never (absent until complete)
        PpirTypeRef::StringLiteral(_) | PpirTypeRef::IntLiteral(_) | PpirTypeRef::BoolLiteral(_) => {
            PpirTypeRef::Never
        }

        // Never: S = never
        PpirTypeRef::Never => PpirTypeRef::Never,

        // Containers: S = empty container
        PpirTypeRef::List(_) => PpirTypeRef::List(Box::new(PpirTypeRef::Never)),
        PpirTypeRef::Map { key, .. } => PpirTypeRef::Map {
            key: key.clone(),
            value: Box::new(PpirTypeRef::Never),
        },

        // Everything else: S = null
        _ => PpirTypeRef::Null,
    }
}

/// Build a union `PpirTypeRef` from S and D, with minimal simplification.
///
/// Only simplifies `never | T → T` and `T | T → T`.
/// Full simplification is deferred to TIR/VIR.
pub(crate) fn make_union(s: PpirTypeRef, d: PpirTypeRef) -> PpirTypeRef {
    if s == d {
        return s;
    }
    match (&s, &d) {
        (PpirTypeRef::Never, _) => d,
        (_, PpirTypeRef::Never) => s,
        _ => PpirTypeRef::union(vec![s, d]),
    }
}

//
// ──────────────────────────────────────────── BUILDING PPIR FIELDS ─────
//

/// Build `ppir::ClassifiedField`s for a class by reading the CST class definition.
///
/// Extracts field names, types, attributes, and stream annotations directly
/// from the CST. Does NOT depend on HIR.
pub(crate) fn build_ppir_fields(
    class_def: &baml_compiler_syntax::ast::ClassDef,
    cst_stream_attrs: Option<&Vec<(Name, StreamAttrs)>>,
    names: &PpirNames<'_>,
    db: &dyn crate::Db,
) -> Vec<ClassifiedField> {
    class_def
        .fields()
        .filter_map(|field_node| {
            let field_name: Name = SmolStr::new(field_node.name()?.text());

            // Parse field type from CST TypeExpr → PpirTypeRef
            let type_ref = field_node
                .ty()
                .map(|te| PpirTypeRef::from_ast(&te))
                .unwrap_or(PpirTypeRef::Unknown);

            // Classify using cross-file name knowledge
            let ty = Ty::classify(&type_ref, names, db);

            // Look up CST stream annotations for this field
            let stream_attrs = cst_stream_attrs
                .and_then(|fa| fa.iter().find(|(name, _)| *name == field_name))
                .map(|(_, a)| a.clone())
                .unwrap_or_default();

            // Extract carry-through attributes from CST
            let mut alias = None;
            let mut description = None;
            let mut skip = false;

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

            Some(ClassifiedField {
                name: field_name,
                ty,
                type_ref,
                stream_type: stream_attrs.stream_type,
                stream_starts_as: stream_attrs.stream_starts_as,
                stream_with_state: stream_attrs.stream_with_state,
                stream_done: stream_attrs.stream_done,
                stream_not_null: stream_attrs.stream_not_null,
                alias,
                description,
                skip,
            })
        })
        .collect()
}

//
// ──────────────────────────────────────────── STREAM EXPANSION ─────
//

/// Expand a class into its `stream_*` variant.
///
/// Takes the class name, `is_dynamic` flag, and PPIR fields (with classified
/// types and stream annotations), and returns a `Class`.
pub(crate) fn expand_stream_class(
    class_name: &Name,
    is_dynamic: bool,
    ppir_fields: &[ClassifiedField],
) -> Class {
    let mut stream_fields = Vec::new();

    for pf in ppir_fields {
        // 1. Desugar legacy attributes
        let (effective_stream_type, _has_completed) = desugar_stream_attrs(pf);

        // 2. Compute D (stream-expanded type)
        let d = effective_stream_type.unwrap_or_else(|| pf.ty.stream_expand());

        // 3. Compute S (starting value type)
        // @stream.not_null → S=never (field absent until first data)
        // Otherwise use default_starts_as based on D's category
        let s = if pf.stream_not_null {
            PpirTypeRef::Never
        } else {
            default_starts_as(&d)
        };

        // 4. Check for field omission (S=never AND D=never → omit field)
        if matches!((&s, &d), (PpirTypeRef::Never, PpirTypeRef::Never)) {
            continue;
        }

        // 5. Build stream type = S | D
        let stream_type_ref = make_union(s, d);

        // 6. Create Field for the generated stream_* class
        stream_fields.push(Field {
            name: pf.name.clone(),
            type_ref: stream_type_ref,
            starts_as: pf.stream_starts_as.clone(),
            alias: pf.alias.clone(),
            description: pf.description.clone(),
            skip: pf.skip,
        });
    }

    Class {
        name: SmolStr::new(format!("stream_{class_name}")),
        fields: stream_fields,
        is_dynamic,
    }
}

/// Expand a type alias into its `stream_*` variant.
///
/// Classifies the alias's type expression and stream-expands it.
pub(crate) fn expand_stream_type_alias(
    alias_name: &Name,
    type_ref: &PpirTypeRef,
    names: &PpirNames<'_>,
    db: &dyn crate::Db,
) -> TypeAlias {
    let classified = Ty::classify(type_ref, names, db);
    let expanded_type_ref = classified.stream_expand();

    TypeAlias {
        name: SmolStr::new(format!("stream_{alias_name}")),
        type_ref: expanded_type_ref,
    }
}
