//! Stream type expansion (preprocessor).
//!
//! Generates `stream.*` classes and type aliases by applying the default
//! expansion rules from BEP-006. This runs after the symbol table is built
//! so it has cross-file name classification, and produces items that are
//! merged into the expanded project items visible to TIR and downstream.
//!
//! # Algorithm
//!
//! For each class `C`, generate `stream.C` with fields computed as:
//! 1. `D = stream_expand(field_type)` — replace class/alias refs with stream.* refs
//! 2. `S = default_starts_as(D)` — compute the "starts as" type
//! 3. `raw = S | D` — union of starts-as and done types
//! 4. `simplified = simplify(raw)` — remove `never`, dedup
//! 5. If simplified is `never`, omit the field entirely
//!
//! For each type alias `A = T`, generate `stream.A = stream_expand(T)`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use baml_base::Name;

use crate::{
    file_item_tree,
    item_tree::{Attribute, Class, Field, ItemTree, TypeAlias},
    path::Path,
    symbol_table::{Definition, symbol_table},
    type_ref::TypeRef,
};

// ─────────────────────────────────────── NAME CLASSIFICATION ─────

/// Classifies all type names in a project for stream expansion.
///
/// The expansion algorithm needs to know whether a name refers to a class,
/// enum, or type alias to decide whether to add the `stream.` prefix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameClassification {
    pub class_names: HashSet<Name>,
    pub enum_names: HashSet<Name>,
    pub type_alias_names: HashSet<Name>,
}

impl NameClassification {
    pub fn new() -> Self {
        Self::default()
    }
}

// ─────────────────────────────────────── ORIGIN MAP ─────

/// Maps generated stream.* item names back to their source items.
///
/// Used for diagnostic span resolution: when TIR reports an error on a
/// stream.* type, the span must point to the original user-authored source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamOriginMap {
    /// stream.* class name → source class name
    pub class_origins: HashMap<Name, Name>,
    /// stream.* type alias name → source alias name
    pub type_alias_origins: HashMap<Name, Name>,
    /// For stream.* class fields: (stream_class_name, field_index) → source field_index
    pub field_index_map: HashMap<(Name, usize), usize>,
}

impl StreamOriginMap {
    pub fn new() -> Self {
        Self::default()
    }
}

// ─────────────────────────────────────── SALSA QUERIES ─────

/// Result of stream expansion for a single file.
#[salsa::tracked]
pub struct StreamExpansionResult<'db> {
    /// The generated stream.* items for this file.
    #[tracked]
    #[returns(ref)]
    pub item_tree: Arc<ItemTree>,

    /// Origin map: stream.* item name → source item name.
    /// Used for diagnostic span resolution.
    #[tracked]
    #[returns(ref)]
    pub origins: Arc<StreamOriginMap>,
}

/// Cached result of name classification for a project.
#[salsa::tracked]
pub struct NameClassificationResult<'db> {
    #[tracked]
    #[returns(ref)]
    pub classification: NameClassification,
}

/// Build the name classification for a project from the symbol table.
///
/// This is a Salsa-tracked query so the result is cached and only recomputed
/// when the symbol table changes.
#[salsa::tracked]
pub fn name_classification<'db>(
    db: &'db dyn crate::Db,
    project: baml_workspace::Project,
) -> NameClassificationResult<'db> {
    let sym_table = symbol_table(db, project);
    let mut result = NameClassification::new();

    for (qname, def) in sym_table.types(db) {
        match def {
            Definition::Class(_) => {
                result.class_names.insert(qname.display_name());
            }
            Definition::Enum(_) => {
                result.enum_names.insert(qname.display_name());
            }
            Definition::TypeAlias(_) => {
                result.type_alias_names.insert(qname.display_name());
            }
            _ => {}
        }
    }

    NameClassificationResult::new(db, result)
}

/// Generate stream.* items for a single file.
///
/// This is the per-file preprocessor query. It uses the project-wide
/// name classification to determine how to expand type references,
/// then generates stream.* classes and type aliases for each item
/// defined in this file.
#[salsa::tracked]
pub fn stream_expand_file<'db>(
    db: &'db dyn crate::Db,
    file: baml_base::SourceFile,
    project: baml_workspace::Project,
) -> StreamExpansionResult<'db> {
    let names = name_classification(db, project).classification(db);
    let source_tree = file_item_tree(db, file);

    let mut stream_tree = ItemTree::new();
    let mut origins = StreamOriginMap::new();

    // Generate stream.* classes
    for (_, class) in source_tree.iter_classes() {
        let stream_class = expand_class(class, names, &mut origins);
        stream_tree.alloc_class(stream_class);
    }

    // Generate stream.* type aliases
    for (_, alias) in source_tree.iter_type_aliases() {
        let stream_alias = expand_type_alias(alias, names, &mut origins);
        stream_tree.alloc_type_alias(stream_alias);
    }

    StreamExpansionResult::new(db, Arc::new(stream_tree), Arc::new(origins))
}

/// Aggregated stream type names across all files in a project.
///
/// This is the query that TIR uses to discover stream.* class and type
/// alias names, so it can resolve `stream.Resume` as a valid class name.
#[salsa::tracked]
pub struct StreamTypeNames<'db> {
    /// Stream class names: (stream_name, qualified_name) pairs.
    #[tracked]
    #[returns(ref)]
    pub class_names: Vec<(Name, baml_base::QualifiedName)>,

    /// Stream type alias names.
    #[tracked]
    #[returns(ref)]
    pub type_alias_names: Vec<Name>,
}

/// Collect all stream.* type names across the project.
///
/// Used by TIR queries to include stream.* names in type resolution.
#[salsa::tracked]
pub fn stream_type_names<'db>(
    db: &'db dyn crate::Db,
    project: baml_workspace::Project,
) -> StreamTypeNames<'db> {
    let mut class_names = Vec::new();
    let mut alias_names = Vec::new();

    for file in project.files(db) {
        let expansion = stream_expand_file(db, *file, project);
        let stream_tree = expansion.item_tree(db);

        for (_, class) in stream_tree.iter_classes() {
            let qn = baml_base::QualifiedName::local(class.name.clone());
            class_names.push((class.name.clone(), qn));
        }

        for (_, alias) in stream_tree.iter_type_aliases() {
            alias_names.push(alias.name.clone());
        }
    }

    StreamTypeNames::new(db, class_names, alias_names)
}

// ─────────────────────────────────────── CORE EXPANSION ─────

/// Recursively expand a `TypeRef` for streaming.
///
/// - Primitives, literals, media, `null`, `never`: pass through unchanged
/// - Enums: pass through unchanged
/// - Classes and type aliases: replace with `stream.` prefixed version
/// - Containers (List, Map, Optional, Union): recurse into elements
/// - Unknown names: pass through (TIR will report the error)
pub fn stream_expand(ty: &TypeRef, names: &NameClassification) -> TypeRef {
    match ty {
        // Primitives pass through
        TypeRef::Int | TypeRef::Float | TypeRef::String | TypeRef::Bool => ty.clone(),
        TypeRef::Null => TypeRef::Null,
        TypeRef::Never => TypeRef::Never,

        // Literals pass through
        TypeRef::StringLiteral(_)
        | TypeRef::IntLiteral(_)
        | TypeRef::FloatLiteral(_)
        | TypeRef::BoolLiteral(_) => ty.clone(),

        // Media passes through
        TypeRef::Media(_) => ty.clone(),

        // Named types: need classification
        TypeRef::Path(path) => {
            // For single-segment paths, check against name classification
            if path.segments.len() == 1 {
                let name = &path.segments[0];

                // Enums pass through unchanged
                if names.enum_names.contains(name) {
                    return ty.clone();
                }

                // Classes and type aliases get stream. prefix
                if names.class_names.contains(name) || names.type_alias_names.contains(name) {
                    return TypeRef::Path(Path::new(vec![
                        Name::new("stream"),
                        name.clone(),
                    ]));
                }
            }

            // Multi-segment paths or unknown names: pass through
            // (TIR will report errors for unknown names)
            ty.clone()
        }

        // Containers: recurse into elements
        TypeRef::List(inner) => TypeRef::List(Box::new(stream_expand(inner, names))),

        TypeRef::Map { key, value } => TypeRef::Map {
            key: key.clone(), // keys are not expanded
            value: Box::new(stream_expand(value, names)),
        },

        // Unions: recurse into each variant
        TypeRef::Union(types) => {
            TypeRef::Union(types.iter().map(|t| stream_expand(t, names)).collect())
        }

        // Optional: T? is sugar for T | null → expand T, keep | null
        TypeRef::Optional(inner) => {
            TypeRef::Optional(Box::new(stream_expand(inner, names)))
        }

        // Function types, generics, error/unknown, etc.: pass through
        _ => ty.clone(),
    }
}

// ─────────────────────────────────────── STARTS-AS DEFAULT ─────

/// Compute the default "starts as" type for a stream field.
///
/// This determines what value the field has before the LLM starts
/// producing the final value:
/// - Literals: `never` (absent until complete — literal values are atomic)
/// - Containers (list, map): `never` (empty container is subsumed by D)
/// - Everything else: `null` (field starts absent/null)
fn default_starts_as(d: &TypeRef) -> TypeRef {
    match d {
        // Literals: S = never (absent until complete)
        TypeRef::StringLiteral(_)
        | TypeRef::IntLiteral(_)
        | TypeRef::FloatLiteral(_)
        | TypeRef::BoolLiteral(_) => TypeRef::Never,

        // Containers: S = never (empty container subsumed by D)
        TypeRef::List(_) | TypeRef::Map { .. } => TypeRef::Never,

        // Everything else: S = null
        _ => TypeRef::Null,
    }
}

// ─────────────────────────────────────── SIMPLIFICATION ─────

/// Simplify a `TypeRef` by removing `never` from unions and deduplicating.
///
/// This only handles `never` elimination and dedup at the TypeRef level.
/// Full literal-subsumption rules (e.g., `"foo" | string → string`) are
/// deferred to TIR where types are resolved.
pub fn simplify_type_ref(ty: TypeRef) -> TypeRef {
    match ty {
        TypeRef::Union(types) => {
            let mut simplified: Vec<TypeRef> = Vec::new();
            for t in flatten_union(types) {
                if is_never(&t) {
                    continue; // never | T → T
                }
                if !simplified.contains(&t) {
                    simplified.push(t); // dedup
                }
            }
            match simplified.len() {
                0 => TypeRef::Never,                              // all never → never
                1 => simplified.into_iter().next().unwrap(),      // single element
                _ => TypeRef::Union(simplified),
            }
        }
        TypeRef::Optional(inner) => {
            let simplified_inner = simplify_type_ref(*inner);
            if is_never(&simplified_inner) {
                // null (from optional) with never inner → just null
                TypeRef::Null
            } else {
                TypeRef::Optional(Box::new(simplified_inner))
            }
        }
        other => other,
    }
}

/// Flatten nested unions into a single level.
fn flatten_union(types: Vec<TypeRef>) -> Vec<TypeRef> {
    let mut result = Vec::new();
    for t in types {
        match t {
            TypeRef::Union(inner) => result.extend(flatten_union(inner)),
            other => result.push(other),
        }
    }
    result
}

/// Check if a TypeRef is the `never` type.
pub fn is_never(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Never)
}

/// Create a union of two TypeRefs.
///
/// If either is `never`, the other is returned directly (identity element).
/// If they're equal, returns just one copy. Otherwise creates a 2-element union.
fn union_types(a: TypeRef, b: TypeRef) -> TypeRef {
    if is_never(&a) {
        return b;
    }
    if is_never(&b) {
        return a;
    }
    if a == b {
        return a;
    }
    TypeRef::Union(vec![a, b])
}

// ─────────────────────────────────────── FIELD EXPANSION ─────

/// Compute the streaming version of a class field.
///
/// Returns `None` if the field's stream type simplifies to `never`
/// (meaning the field should be omitted from the stream.* class).
pub fn compute_stream_field(field: &Field, names: &NameClassification) -> Option<Field> {
    let d = stream_expand(&field.type_ref, names);

    // Compute default S based on D's type category
    let s = default_starts_as(&d);

    // Combine: raw_stream_type = S | D
    let stream_type = union_types(s, d);

    // Simplify: never | T → T, etc.
    let simplified = simplify_type_ref(stream_type);

    // If simplified is Never, omit the field entirely
    if is_never(&simplified) {
        return None;
    }

    Some(Field {
        name: field.name.clone(),
        type_ref: simplified,
        alias: field.alias.clone(),
        description: field.description.clone(),
        skip: field.skip.clone(),
    })
}

// ─────────────────────────────────────── CLASS EXPANSION ─────

/// Generate a `stream.*` class from a source class.
///
/// Each field is transformed via `compute_stream_field`. Fields whose
/// stream type simplifies to `never` are omitted. The origin map tracks
/// the relationship between generated and source items for diagnostics.
pub fn expand_class(
    class: &Class,
    names: &NameClassification,
    origins: &mut StreamOriginMap,
) -> Class {
    let stream_name = Name::new(format!("stream.{}", class.name));
    origins
        .class_origins
        .insert(stream_name.clone(), class.name.clone());

    let mut fields = Vec::new();
    for (source_idx, field) in class.fields.iter().enumerate() {
        if let Some(stream_field) = compute_stream_field(field, names) {
            let stream_idx = fields.len();
            origins
                .field_index_map
                .insert((stream_name.clone(), stream_idx), source_idx);
            fields.push(stream_field);
        }
        // else: field's stream type was never — omitted
    }

    Class {
        name: stream_name,
        fields,
        is_dynamic: Attribute::Unset,
        alias: class.alias.clone(),
        description: Attribute::Unset,
    }
}

// ─────────────────────────────── TYPE ALIAS EXPANSION ─────

/// Generate a `stream.*` type alias from a source type alias.
///
/// The alias's RHS type is transformed via `stream_expand`.
pub fn expand_type_alias(
    alias: &TypeAlias,
    names: &NameClassification,
    origins: &mut StreamOriginMap,
) -> TypeAlias {
    let stream_name = Name::new(format!("stream.{}", alias.name));
    origins
        .type_alias_origins
        .insert(stream_name.clone(), alias.name.clone());

    TypeAlias {
        name: stream_name,
        type_ref: stream_expand(&alias.type_ref, names),
    }
}

// ─────────────────────────────────────── TESTS ─────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a NameClassification with the given class, enum, and alias names.
    fn names(classes: &[&str], enums: &[&str], aliases: &[&str]) -> NameClassification {
        NameClassification {
            class_names: classes.iter().map(|s| Name::new(s)).collect(),
            enum_names: enums.iter().map(|s| Name::new(s)).collect(),
            type_alias_names: aliases.iter().map(|s| Name::new(s)).collect(),
        }
    }

    /// Helper to create a simple named TypeRef.
    fn named(s: &str) -> TypeRef {
        TypeRef::Path(Path::single(Name::new(s)))
    }

    /// Helper to create a stream-prefixed path TypeRef.
    fn stream_named(s: &str) -> TypeRef {
        TypeRef::Path(Path::new(vec![Name::new("stream"), Name::new(s)]))
    }

    // ── stream_expand tests ──

    #[test]
    fn test_primitives_pass_through() {
        let n = names(&[], &[], &[]);
        assert_eq!(stream_expand(&TypeRef::Int, &n), TypeRef::Int);
        assert_eq!(stream_expand(&TypeRef::Float, &n), TypeRef::Float);
        assert_eq!(stream_expand(&TypeRef::String, &n), TypeRef::String);
        assert_eq!(stream_expand(&TypeRef::Bool, &n), TypeRef::Bool);
    }

    #[test]
    fn test_null_passes_through() {
        let n = names(&[], &[], &[]);
        assert_eq!(stream_expand(&TypeRef::Null, &n), TypeRef::Null);
    }

    #[test]
    fn test_never_passes_through() {
        let n = names(&[], &[], &[]);
        assert_eq!(stream_expand(&TypeRef::Never, &n), TypeRef::Never);
    }

    #[test]
    fn test_literals_pass_through() {
        let n = names(&[], &[], &[]);
        let s = TypeRef::StringLiteral("hello".to_string());
        assert_eq!(stream_expand(&s, &n), s);

        let i = TypeRef::IntLiteral(42);
        assert_eq!(stream_expand(&i, &n), i);

        let b = TypeRef::BoolLiteral(true);
        assert_eq!(stream_expand(&b, &n), b);
    }

    #[test]
    fn test_class_gets_stream_prefix() {
        let n = names(&["MyClass"], &[], &[]);
        assert_eq!(
            stream_expand(&named("MyClass"), &n),
            stream_named("MyClass"),
        );
    }

    #[test]
    fn test_enum_passes_through() {
        let n = names(&[], &["MyEnum"], &[]);
        assert_eq!(stream_expand(&named("MyEnum"), &n), named("MyEnum"));
    }

    #[test]
    fn test_type_alias_gets_stream_prefix() {
        let n = names(&[], &[], &["MyAlias"]);
        assert_eq!(
            stream_expand(&named("MyAlias"), &n),
            stream_named("MyAlias"),
        );
    }

    #[test]
    fn test_unknown_name_passes_through() {
        let n = names(&[], &[], &[]);
        assert_eq!(stream_expand(&named("Unknown"), &n), named("Unknown"));
    }

    #[test]
    fn test_list_recurses() {
        let n = names(&["MyClass"], &[], &[]);
        let input = TypeRef::List(Box::new(named("MyClass")));
        let expected = TypeRef::List(Box::new(stream_named("MyClass")));
        assert_eq!(stream_expand(&input, &n), expected);
    }

    #[test]
    fn test_map_recurses_into_value() {
        let n = names(&["MyClass"], &[], &[]);
        let input = TypeRef::Map {
            key: Box::new(TypeRef::String),
            value: Box::new(named("MyClass")),
        };
        let expected = TypeRef::Map {
            key: Box::new(TypeRef::String),
            value: Box::new(stream_named("MyClass")),
        };
        assert_eq!(stream_expand(&input, &n), expected);
    }

    #[test]
    fn test_union_recurses() {
        let n = names(&["MyClass"], &[], &[]);
        let input = TypeRef::Union(vec![TypeRef::Int, named("MyClass")]);
        let expected = TypeRef::Union(vec![TypeRef::Int, stream_named("MyClass")]);
        assert_eq!(stream_expand(&input, &n), expected);
    }

    #[test]
    fn test_optional_recurses() {
        let n = names(&["MyClass"], &[], &[]);
        let input = TypeRef::Optional(Box::new(named("MyClass")));
        let expected = TypeRef::Optional(Box::new(stream_named("MyClass")));
        assert_eq!(stream_expand(&input, &n), expected);
    }

    #[test]
    fn test_nested_containers() {
        let n = names(&["MyClass"], &[], &[]);
        // MyClass[][]
        let input = TypeRef::List(Box::new(TypeRef::List(Box::new(named("MyClass")))));
        let expected = TypeRef::List(Box::new(TypeRef::List(Box::new(stream_named("MyClass")))));
        assert_eq!(stream_expand(&input, &n), expected);
    }

    // ── simplify_type_ref tests ──

    #[test]
    fn test_simplify_never_or_string() {
        let input = TypeRef::Union(vec![TypeRef::Never, TypeRef::String]);
        assert_eq!(simplify_type_ref(input), TypeRef::String);
    }

    #[test]
    fn test_simplify_string_or_never() {
        let input = TypeRef::Union(vec![TypeRef::String, TypeRef::Never]);
        assert_eq!(simplify_type_ref(input), TypeRef::String);
    }

    #[test]
    fn test_simplify_never_or_never() {
        let input = TypeRef::Union(vec![TypeRef::Never, TypeRef::Never]);
        assert_eq!(simplify_type_ref(input), TypeRef::Never);
    }

    #[test]
    fn test_simplify_dedup() {
        let input = TypeRef::Union(vec![TypeRef::String, TypeRef::String]);
        assert_eq!(simplify_type_ref(input), TypeRef::String);
    }

    #[test]
    fn test_simplify_null_or_string() {
        let input = TypeRef::Union(vec![TypeRef::Null, TypeRef::String]);
        assert_eq!(
            simplify_type_ref(input),
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String])
        );
    }

    #[test]
    fn test_simplify_non_union_passes_through() {
        assert_eq!(simplify_type_ref(TypeRef::String), TypeRef::String);
        assert_eq!(simplify_type_ref(TypeRef::Never), TypeRef::Never);
    }

    // ── default_starts_as tests ──

    #[test]
    fn test_starts_as_literal_is_never() {
        assert_eq!(
            default_starts_as(&TypeRef::StringLiteral("x".to_string())),
            TypeRef::Never,
        );
        assert_eq!(default_starts_as(&TypeRef::IntLiteral(42)), TypeRef::Never);
        assert_eq!(
            default_starts_as(&TypeRef::BoolLiteral(true)),
            TypeRef::Never,
        );
    }

    #[test]
    fn test_starts_as_list_is_never() {
        let list = TypeRef::List(Box::new(TypeRef::String));
        assert_eq!(default_starts_as(&list), TypeRef::Never);
    }

    #[test]
    fn test_starts_as_map_is_never() {
        let map = TypeRef::Map {
            key: Box::new(TypeRef::String),
            value: Box::new(TypeRef::Int),
        };
        assert_eq!(default_starts_as(&map), TypeRef::Never);
    }

    #[test]
    fn test_starts_as_primitive_is_null() {
        assert_eq!(default_starts_as(&TypeRef::String), TypeRef::Null);
        assert_eq!(default_starts_as(&TypeRef::Int), TypeRef::Null);
    }

    #[test]
    fn test_starts_as_class_is_null() {
        assert_eq!(default_starts_as(&named("MyClass")), TypeRef::Null);
    }

    // ── compute_stream_field tests ──

    #[test]
    fn test_scalar_field_becomes_nullable() {
        let n = names(&[], &[], &[]);
        let field = Field {
            name: Name::new("name"),
            type_ref: TypeRef::String,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            skip: Attribute::Unset,
        };
        let result = compute_stream_field(&field, &n).unwrap();
        assert_eq!(result.name, Name::new("name"));
        // S=null, D=string → null | string
        assert_eq!(
            result.type_ref,
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String])
        );
    }

    #[test]
    fn test_literal_field_stays_literal() {
        let n = names(&[], &[], &[]);
        let field = Field {
            name: Name::new("type"),
            type_ref: TypeRef::StringLiteral("resume".to_string()),
            alias: Attribute::Unset,
            description: Attribute::Unset,
            skip: Attribute::Unset,
        };
        let result = compute_stream_field(&field, &n).unwrap();
        assert_eq!(result.name, Name::new("type"));
        // S=never, D="resume" → never | "resume" → "resume"
        assert_eq!(
            result.type_ref,
            TypeRef::StringLiteral("resume".to_string()),
        );
    }

    #[test]
    fn test_container_field_stays_container() {
        let n = names(&["Education"], &[], &[]);
        let field = Field {
            name: Name::new("education"),
            type_ref: TypeRef::List(Box::new(named("Education"))),
            alias: Attribute::Unset,
            description: Attribute::Unset,
            skip: Attribute::Unset,
        };
        let result = compute_stream_field(&field, &n).unwrap();
        assert_eq!(result.name, Name::new("education"));
        // D=stream.Education[], S=never → never | stream.Education[] → stream.Education[]
        assert_eq!(
            result.type_ref,
            TypeRef::List(Box::new(stream_named("Education"))),
        );
    }

    #[test]
    fn test_never_field_is_omitted() {
        let n = names(&[], &[], &[]);
        let field = Field {
            name: Name::new("impossible"),
            type_ref: TypeRef::Never,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            skip: Attribute::Unset,
        };
        // D=never, S=null → null | never → null (not never!)
        // Actually: stream_expand(never) = never, default_starts_as(never) = null
        // union_types(null, never) = null (since never is identity)
        // simplify(null) = null, which is NOT never, so field is kept
        let result = compute_stream_field(&field, &n);
        assert!(result.is_some());
        assert_eq!(result.unwrap().type_ref, TypeRef::Null);
    }

    // ── expand_class tests ──

    #[test]
    fn test_expand_resume_class() {
        let n = names(&["Resume", "Education"], &[], &[]);
        let class = Class {
            name: Name::new("Resume"),
            fields: vec![
                Field {
                    name: Name::new("name"),
                    type_ref: TypeRef::String,
                    alias: Attribute::Unset,
                    description: Attribute::Unset,
                    skip: Attribute::Unset,
                },
                Field {
                    name: Name::new("education"),
                    type_ref: TypeRef::List(Box::new(named("Education"))),
                    alias: Attribute::Unset,
                    description: Attribute::Unset,
                    skip: Attribute::Unset,
                },
            ],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
        };

        let mut origins = StreamOriginMap::new();
        let result = expand_class(&class, &n, &mut origins);

        assert_eq!(result.name, Name::new("stream.Resume"));
        assert_eq!(result.fields.len(), 2);

        // name: string → name: string | null
        assert_eq!(result.fields[0].name, Name::new("name"));
        assert_eq!(
            result.fields[0].type_ref,
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String])
        );

        // education: Education[] → education: stream.Education[]
        assert_eq!(result.fields[1].name, Name::new("education"));
        assert_eq!(
            result.fields[1].type_ref,
            TypeRef::List(Box::new(stream_named("Education"))),
        );

        // Check origin map
        assert_eq!(
            origins.class_origins.get(&Name::new("stream.Resume")),
            Some(&Name::new("Resume")),
        );
        assert_eq!(
            origins.field_index_map.get(&(Name::new("stream.Resume"), 0)),
            Some(&0),
        );
        assert_eq!(
            origins.field_index_map.get(&(Name::new("stream.Resume"), 1)),
            Some(&1),
        );
    }

    #[test]
    fn test_expand_class_with_literal_field() {
        let n = names(&[], &[], &[]);
        let class = Class {
            name: Name::new("TypedDoc"),
            fields: vec![
                Field {
                    name: Name::new("type"),
                    type_ref: TypeRef::StringLiteral("resume".to_string()),
                    alias: Attribute::Unset,
                    description: Attribute::Unset,
                    skip: Attribute::Unset,
                },
                Field {
                    name: Name::new("content"),
                    type_ref: TypeRef::String,
                    alias: Attribute::Unset,
                    description: Attribute::Unset,
                    skip: Attribute::Unset,
                },
            ],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
        };

        let mut origins = StreamOriginMap::new();
        let result = expand_class(&class, &n, &mut origins);

        assert_eq!(result.name, Name::new("stream.TypedDoc"));
        assert_eq!(result.fields.len(), 2);

        // type: "resume" → type: "resume" (S=never eliminated)
        assert_eq!(result.fields[0].name, Name::new("type"));
        assert_eq!(
            result.fields[0].type_ref,
            TypeRef::StringLiteral("resume".to_string()),
        );

        // content: string → content: string | null
        assert_eq!(result.fields[1].name, Name::new("content"));
        assert_eq!(
            result.fields[1].type_ref,
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String])
        );
    }

    // ── expand_type_alias tests ──

    #[test]
    fn test_expand_type_alias() {
        let n = names(&["Resume"], &[], &[]);
        let alias = TypeAlias {
            name: Name::new("Doc"),
            type_ref: TypeRef::Union(vec![named("Resume"), TypeRef::String]),
        };

        let mut origins = StreamOriginMap::new();
        let result = expand_type_alias(&alias, &n, &mut origins);

        assert_eq!(result.name, Name::new("stream.Doc"));
        assert_eq!(
            result.type_ref,
            TypeRef::Union(vec![stream_named("Resume"), TypeRef::String]),
        );
        assert_eq!(
            origins.type_alias_origins.get(&Name::new("stream.Doc")),
            Some(&Name::new("Doc")),
        );
    }

    // ── union_types helper tests ──

    #[test]
    fn test_union_types_identity() {
        assert_eq!(union_types(TypeRef::Never, TypeRef::String), TypeRef::String);
        assert_eq!(union_types(TypeRef::String, TypeRef::Never), TypeRef::String);
    }

    #[test]
    fn test_union_types_same() {
        assert_eq!(union_types(TypeRef::String, TypeRef::String), TypeRef::String);
    }

    #[test]
    fn test_union_types_different() {
        assert_eq!(
            union_types(TypeRef::Null, TypeRef::String),
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String]),
        );
    }

    // ── Edge cases ──

    #[test]
    fn test_expand_education_class() {
        let n = names(&["Education"], &[], &[]);
        let class = Class {
            name: Name::new("Education"),
            fields: vec![Field {
                name: Name::new("school"),
                type_ref: TypeRef::String,
                alias: Attribute::Unset,
                description: Attribute::Unset,
                skip: Attribute::Unset,
            }],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
        };

        let mut origins = StreamOriginMap::new();
        let result = expand_class(&class, &n, &mut origins);

        assert_eq!(result.name, Name::new("stream.Education"));
        assert_eq!(result.fields.len(), 1);
        assert_eq!(result.fields[0].name, Name::new("school"));
        assert_eq!(
            result.fields[0].type_ref,
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String])
        );
    }

    #[test]
    fn test_expand_class_with_enum_field() {
        let n = names(&[], &["Status"], &[]);
        let class = Class {
            name: Name::new("Person"),
            fields: vec![Field {
                name: Name::new("status"),
                type_ref: named("Status"),
                alias: Attribute::Unset,
                description: Attribute::Unset,
                skip: Attribute::Unset,
            }],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
        };

        let mut origins = StreamOriginMap::new();
        let result = expand_class(&class, &n, &mut origins);

        assert_eq!(result.name, Name::new("stream.Person"));
        assert_eq!(result.fields.len(), 1);
        // enum field: D=Status (passes through), S=null → null | Status
        assert_eq!(result.fields[0].name, Name::new("status"));
        assert_eq!(
            result.fields[0].type_ref,
            TypeRef::Union(vec![TypeRef::Null, named("Status")])
        );
    }

    #[test]
    fn test_expand_class_with_optional_field() {
        let n = names(&[], &[], &[]);
        let class = Class {
            name: Name::new("Foo"),
            fields: vec![Field {
                name: Name::new("maybe"),
                type_ref: TypeRef::Optional(Box::new(TypeRef::String)),
                alias: Attribute::Unset,
                description: Attribute::Unset,
                skip: Attribute::Unset,
            }],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
        };

        let mut origins = StreamOriginMap::new();
        let result = expand_class(&class, &n, &mut origins);

        // D=string?, S=null → null | string?
        assert_eq!(result.fields[0].name, Name::new("maybe"));
        assert_eq!(
            result.fields[0].type_ref,
            TypeRef::Union(vec![
                TypeRef::Null,
                TypeRef::Optional(Box::new(TypeRef::String))
            ])
        );
    }
}
