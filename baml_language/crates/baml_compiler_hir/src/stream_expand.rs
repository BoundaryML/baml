//! Stream expansion support for BEP-006.
//!
//! This module provides:
//! - Name collection from the CST (for classifying type references)
//! - The `stream_expand` function that rewrites type references with `stream.` prefixes
//! - `stream_expand_class` / `stream_expand_type_alias` for generating synthetic items

use std::collections::HashSet;

use baml_base::Name;
use baml_compiler_syntax::SyntaxNode;

use crate::item_tree::{
    Attribute, Class, ClassCompilerGenerated, Field, SapAnnotation, SapValue, TypeAlias,
    TypeAliasCompilerGenerated,
};
use crate::path::Path;
use crate::type_ref::TypeRef;

// ═══════════════════════════════════════════════════════════════════════════
// NAME COLLECTION
// ═══════════════════════════════════════════════════════════════════════════

/// Sets of type names collected from the CST, used by stream expansion
/// to determine whether a type reference should get a `stream.` prefix.
///
/// - Classes and type aliases get `stream.` prefixes
/// - Enums do NOT get `stream.` prefixes
#[derive(Debug, Clone, Default)]
pub struct TypeNameSets {
    pub class_names: HashSet<Name>,
    pub enum_names: HashSet<Name>,
    pub alias_names: HashSet<Name>,
}

/// Collect type names (classes, enums, type aliases) from a single file's CST.
///
/// This is a lightweight pre-pass that walks top-level CST children and extracts
/// names without performing full HIR lowering. It's used by stream expansion
/// to classify type references (class vs enum vs alias) before generating
/// `stream.*` variants.
pub fn collect_file_type_names(root: &SyntaxNode) -> TypeNameSets {
    use baml_compiler_syntax::ast::{ClassDef, EnumDef, TypeAliasDef};
    use baml_compiler_syntax::SyntaxKind;
    use rowan::ast::AstNode;

    let mut names = TypeNameSets::default();

    for child in root.children() {
        match child.kind() {
            SyntaxKind::CLASS_DEF => {
                if let Some(class) = ClassDef::cast(child) {
                    if let Some(name_token) = class.name() {
                        names.class_names.insert(Name::new(name_token.text()));
                    }
                }
            }
            SyntaxKind::ENUM_DEF => {
                if let Some(enum_def) = EnumDef::cast(child) {
                    if let Some(name_token) = enum_def.name() {
                        names.enum_names.insert(Name::new(name_token.text()));
                    }
                }
            }
            SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(alias) = TypeAliasDef::cast(child) {
                    if let Some(name_token) = alias.name() {
                        names.alias_names.insert(Name::new(name_token.text()));
                    }
                }
            }
            _ => {}
        }
    }

    names
}

/// Collect type names from all files in a project.
///
/// This aggregates names across all source files so that stream expansion
/// in file A can correctly classify types defined in file B.
pub fn collect_project_type_names(
    db: &dyn crate::Db,
    project: baml_workspace::Project,
) -> TypeNameSets {
    use baml_compiler_parser::syntax_tree;

    let mut all = TypeNameSets::default();

    for file in project.files(db) {
        let tree = syntax_tree(db, *file);
        let file_names = collect_file_type_names(&tree);
        all.class_names.extend(file_names.class_names);
        all.enum_names.extend(file_names.enum_names);
        all.alias_names.extend(file_names.alias_names);
    }

    all
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE EXPANSION
// ═══════════════════════════════════════════════════════════════════════════

/// Get the display name of a path (joining segments with `.`).
fn path_display_name(path: &Path) -> String {
    path.segments
        .iter()
        .map(Name::as_str)
        .collect::<Vec<_>>()
        .join(".")
}

/// Recursively compute the default streaming type for a TypeRef.
///
/// `stream_expand(T)` =
///   - primitive           → T (unchanged)
///   - literal             → T (unchanged)
///   - enum                → T (unchanged)
///   - class C             → stream.C
///   - type_alias A        → stream.A
///   - T[]                 → stream_expand(T)[]
///   - map<K,V>            → map<K, stream_expand(V)>
///   - A | B               → stream_expand(A) | stream_expand(B)
///   - T?                  → stream_expand(T) | null
///   - null                → null
///   - never               → never
///   - already stream.*    → T (don't double-prefix)
pub fn stream_expand(type_ref: &TypeRef, names: &TypeNameSets) -> TypeRef {
    match type_ref {
        // Primitives, literals, null, never -- pass through
        TypeRef::Int
        | TypeRef::Float
        | TypeRef::String
        | TypeRef::Bool
        | TypeRef::Null
        | TypeRef::Never
        | TypeRef::StringLiteral(_)
        | TypeRef::IntLiteral(_)
        | TypeRef::FloatLiteral(_)
        | TypeRef::BoolLiteral(_)
        | TypeRef::Media(_)
        | TypeRef::BuiltinUnknown
        | TypeRef::Type
        | TypeRef::Error
        | TypeRef::Unknown => type_ref.clone(),

        // Named type -- check classification
        TypeRef::Path(path) => {
            let name = path_display_name(path);
            // Don't double-prefix
            if name.starts_with("stream.") {
                return type_ref.clone();
            }
            // Enums pass through unchanged
            if names.enum_names.contains(&Name::new(&name)) {
                return type_ref.clone();
            }
            // Classes and type aliases get stream. prefix
            if names.class_names.contains(&Name::new(&name))
                || names.alias_names.contains(&Name::new(&name))
            {
                let segments: Vec<Name> = vec![Name::new("stream"), Name::new(&name)];
                return TypeRef::Path(Path::new(segments));
            }
            // Unknown name -- pass through (will error in TIR)
            type_ref.clone()
        }

        // Containers -- recurse
        TypeRef::List(inner) => TypeRef::List(Box::new(stream_expand(inner, names))),

        TypeRef::Map { key, value } => TypeRef::Map {
            key: key.clone(), // keys don't stream
            value: Box::new(stream_expand(value, names)),
        },

        TypeRef::Optional(inner) => {
            // T? = T | null, expand T and keep | null
            TypeRef::Union(vec![stream_expand(inner, names), TypeRef::Null])
        }

        TypeRef::Union(members) => {
            TypeRef::Union(members.iter().map(|m| stream_expand(m, names)).collect())
        }

        // Function types, generics, type params -- pass through
        TypeRef::Function { .. } | TypeRef::Generic { .. } | TypeRef::TypeParam(_) => {
            type_ref.clone()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STARTS_AS DEFAULT LOGIC
// ═══════════════════════════════════════════════════════════════════════════

/// The default starts_as category for a given D (stream type).
enum StartsAsDefault {
    Null,  // scalars, classes, enums, unions: S = null
    Never, // literals: S = never (absent until complete)
    List,  // list containers: S subsumed by list type
    Map,   // map containers: S subsumed by map type
}

/// Determine the default starts_as category based on D's type.
fn default_starts_as(d: &TypeRef) -> StartsAsDefault {
    match d {
        TypeRef::StringLiteral(_)
        | TypeRef::IntLiteral(_)
        | TypeRef::FloatLiteral(_)
        | TypeRef::BoolLiteral(_) => StartsAsDefault::Never,
        TypeRef::List(_) => StartsAsDefault::List,
        TypeRef::Map { .. } => StartsAsDefault::Map,
        _ => StartsAsDefault::Null,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLASS EXPANSION
// ═══════════════════════════════════════════════════════════════════════════

/// Convert a TypeRef (from @stream.starts_as) to a SapValue for @sap.missing.
///
/// Only singleton types are valid. Returns None for non-singleton types.
fn sap_value_from_type_ref(type_ref: &TypeRef) -> Option<SapValue> {
    match type_ref {
        TypeRef::Null => Some(SapValue::Null),
        TypeRef::Never => Some(SapValue::Never),
        TypeRef::StringLiteral(s) => Some(SapValue::String(s.clone())),
        TypeRef::IntLiteral(i) => Some(SapValue::Int(*i)),
        TypeRef::FloatLiteral(f) => Some(SapValue::Float(f.clone())),
        TypeRef::BoolLiteral(b) => Some(SapValue::Bool(*b)),
        // For empty list/map, we'd need special TypeRef variants.
        // For now, these aren't representable as TypeRef values.
        _ => None,
    }
}

/// Generate the `stream.*` variant of a class.
///
/// For each field, computes the full S|D formula:
/// 1. D = user's `@stream.type` if present, else `stream_expand(T)` (default)
/// 2. S = user's `@stream.starts_as` if present, else `@sap.must_start` means S=never, else default
/// 3. stream_type = simplify(S | D)
/// 4. Build SAP annotations from the computed values
///
/// Fields whose stream type is `never` are omitted from the generated class.
pub fn stream_expand_class(class: &Class, names: &TypeNameSets) -> Class {
    let stream_name = Name::new(format!("stream.{}", class.name));

    let fields: Vec<Field> = class
        .fields
        .iter()
        .filter_map(|field| {
            // Step 1: Compute D (stream type) - user override or default
            let d = if let Some(user_d) = &field.stream_type_attr {
                user_d.clone()
            } else {
                stream_expand(&field.type_ref, names)
            };

            // Early exit: if D is Never, field is entirely absent from stream type.
            // This handles @stream.type(never) → field omitted.
            if matches!(&d, TypeRef::Never) {
                return None;
            }

            // Step 2: Compute S (starts_as) based on D and user attributes
            let s = if let Some(user_s) = &field.stream_starts_as {
                user_s.clone()
            } else if field.sap_must_start {
                // @sap.must_start: S = never (field absent until parsing begins)
                TypeRef::Never
            } else {
                // Default based on D's category
                match default_starts_as(&d) {
                    StartsAsDefault::Null => TypeRef::Null,
                    StartsAsDefault::Never => TypeRef::Never,
                    StartsAsDefault::List => TypeRef::Never, // [] subsumed by list type
                    StartsAsDefault::Map => TypeRef::Never,  // {} subsumed by map type
                }
            };

            // Step 3: Combine S | D
            let raw = match (&s, &d) {
                (TypeRef::Never, _) => d.clone(),          // never | D = D
                (_, TypeRef::Never) => s.clone(),          // S | never = S
                _ => TypeRef::Union(vec![s.clone(), d.clone()]),
            };

            // Step 4: If result is Never, omit the field
            if matches!(&raw, TypeRef::Never) {
                return None;
            }

            // Step 5: Build SAP annotations
            let mut sap_annotations = Vec::new();

            // @sap.missing from starts_as value
            if let Some(sap_val) = sap_value_from_type_ref(&s) {
                // Always record the starts_as value as @sap.missing
                // (Never means field absent, Null means null default)
                sap_annotations.push(SapAnnotation::Missing(sap_val));
            }

            // @sap.completed from field attribute
            if field.sap_completed {
                sap_annotations.push(SapAnnotation::Completed);
            }

            Some(Field {
                name: field.name.clone(),
                type_ref: raw,
                alias: field.alias.clone(),
                description: field.description.clone(),
                skip: field.skip.clone(),
                // User-facing attrs are not carried to generated fields
                stream_starts_as: None,
                stream_type_attr: None,
                sap_completed: false,
                sap_must_start: false,
                // stream_with_state IS carried through
                stream_with_state: field.stream_with_state,
                sap_annotations,
            })
        })
        .collect();

    Class {
        name: stream_name.clone(),
        fields,
        is_dynamic: class.is_dynamic.clone(),
        alias: Attribute::Unset, // stream.* classes don't inherit alias
        description: Attribute::Unset,
        compiler_generated: Some(ClassCompilerGenerated::StreamExpand {
            source_name: class.name.clone(),
        }),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE ALIAS EXPANSION
// ═══════════════════════════════════════════════════════════════════════════

/// Generate the `stream.*` variant of a type alias.
pub fn stream_expand_type_alias(alias: &TypeAlias, names: &TypeNameSets) -> TypeAlias {
    let stream_name = Name::new(format!("stream.{}", alias.name));
    let expanded = stream_expand(&alias.type_ref, names);

    TypeAlias {
        name: stream_name,
        type_ref: expanded,
        compiler_generated: Some(TypeAliasCompilerGenerated::StreamExpand {
            source_name: alias.name.clone(),
        }),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_tree::Attribute;

    /// Helper to parse BAML source and collect type names.
    fn collect_names_from_source(source: &str) -> TypeNameSets {
        use baml_base::FileId;
        use baml_compiler_syntax::SyntaxNode;
        let tokens = baml_compiler_lexer::lex_lossless(source, FileId::new(0));
        let (green, _errors) = baml_compiler_parser::parse_file(&tokens);
        let root = SyntaxNode::new_root(green);
        collect_file_type_names(&root)
    }

    fn make_names(classes: &[&str], enums: &[&str], aliases: &[&str]) -> TypeNameSets {
        TypeNameSets {
            class_names: classes.iter().map(|s| Name::new(*s)).collect(),
            enum_names: enums.iter().map(|s| Name::new(*s)).collect(),
            alias_names: aliases.iter().map(|s| Name::new(*s)).collect(),
        }
    }

    /// Helper to create a simple field with default streaming attributes.
    fn make_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: Name::new(name),
            type_ref,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            skip: Attribute::Unset,
            stream_starts_as: None,
            stream_type_attr: None,
            sap_completed: false,
            sap_must_start: false,
            stream_with_state: false,
            sap_annotations: Vec::new(),
        }
    }

    // ─── Name collection tests ──────────────────────────────────────

    #[test]
    fn test_collects_class_names() {
        let names = collect_names_from_source(
            r#"
            class Resume {
                name string
            }
            class Education {
                school string
            }
            "#,
        );
        assert!(names.class_names.contains(&Name::new("Resume")));
        assert!(names.class_names.contains(&Name::new("Education")));
        assert_eq!(names.class_names.len(), 2);
        assert!(names.enum_names.is_empty());
        assert!(names.alias_names.is_empty());
    }

    #[test]
    fn test_collects_enum_names() {
        let names = collect_names_from_source(
            r#"
            enum Status {
                Active
                Inactive
            }
            "#,
        );
        assert!(names.enum_names.contains(&Name::new("Status")));
        assert_eq!(names.enum_names.len(), 1);
        assert!(names.class_names.is_empty());
    }

    #[test]
    fn test_collects_type_alias_names() {
        let names = collect_names_from_source(
            r#"
            type Foo = string | int
            "#,
        );
        assert!(names.alias_names.contains(&Name::new("Foo")));
        assert_eq!(names.alias_names.len(), 1);
    }

    #[test]
    fn test_collects_mixed_types() {
        let names = collect_names_from_source(
            r#"
            class Resume {
                name string
                status Status
            }
            enum Status {
                Active
                Inactive
            }
            type Foo = Resume | string
            "#,
        );
        assert!(names.class_names.contains(&Name::new("Resume")));
        assert!(names.enum_names.contains(&Name::new("Status")));
        assert!(names.alias_names.contains(&Name::new("Foo")));
    }

    // ─── stream_expand type rewriting tests ─────────────────────────

    #[test]
    fn test_expand_primitive_unchanged() {
        let names = make_names(&[], &[], &[]);
        assert_eq!(stream_expand(&TypeRef::String, &names), TypeRef::String);
        assert_eq!(stream_expand(&TypeRef::Int, &names), TypeRef::Int);
        assert_eq!(stream_expand(&TypeRef::Bool, &names), TypeRef::Bool);
        assert_eq!(stream_expand(&TypeRef::Float, &names), TypeRef::Float);
        assert_eq!(stream_expand(&TypeRef::Null, &names), TypeRef::Null);
        assert_eq!(stream_expand(&TypeRef::Never, &names), TypeRef::Never);
    }

    #[test]
    fn test_expand_literal_unchanged() {
        let names = make_names(&[], &[], &[]);
        let lit = TypeRef::StringLiteral("resume".to_string());
        assert_eq!(stream_expand(&lit, &names), lit);
    }

    #[test]
    fn test_expand_class_gets_prefix() {
        let names = make_names(&["Person"], &[], &[]);
        let input = TypeRef::Path(Path::single(Name::new("Person")));
        let expected = TypeRef::Path(Path::new(vec![
            Name::new("stream"),
            Name::new("Person"),
        ]));
        assert_eq!(stream_expand(&input, &names), expected);
    }

    #[test]
    fn test_expand_enum_unchanged() {
        let names = make_names(&[], &["Status"], &[]);
        let input = TypeRef::Path(Path::single(Name::new("Status")));
        assert_eq!(stream_expand(&input, &names), input);
    }

    #[test]
    fn test_expand_alias_gets_prefix() {
        let names = make_names(&[], &[], &["MyAlias"]);
        let input = TypeRef::Path(Path::single(Name::new("MyAlias")));
        let expected = TypeRef::Path(Path::new(vec![
            Name::new("stream"),
            Name::new("MyAlias"),
        ]));
        assert_eq!(stream_expand(&input, &names), expected);
    }

    #[test]
    fn test_expand_no_double_prefix() {
        let names = make_names(&["Bar"], &[], &[]);
        let input = TypeRef::Path(Path::new(vec![Name::new("stream"), Name::new("Bar")]));
        assert_eq!(stream_expand(&input, &names), input);
    }

    #[test]
    fn test_expand_list_recurses() {
        let names = make_names(&["Person"], &[], &[]);
        let input = TypeRef::List(Box::new(TypeRef::Path(Path::single(Name::new("Person")))));
        let expected = TypeRef::List(Box::new(TypeRef::Path(Path::new(vec![
            Name::new("stream"),
            Name::new("Person"),
        ]))));
        assert_eq!(stream_expand(&input, &names), expected);
    }

    #[test]
    fn test_expand_map_recurses_value_only() {
        let names = make_names(&["Person"], &[], &[]);
        let input = TypeRef::Map {
            key: Box::new(TypeRef::String),
            value: Box::new(TypeRef::Path(Path::single(Name::new("Person")))),
        };
        let expected = TypeRef::Map {
            key: Box::new(TypeRef::String),
            value: Box::new(TypeRef::Path(Path::new(vec![
                Name::new("stream"),
                Name::new("Person"),
            ]))),
        };
        assert_eq!(stream_expand(&input, &names), expected);
    }

    #[test]
    fn test_expand_union_recurses() {
        let names = make_names(&["Person"], &[], &[]);
        let input = TypeRef::Union(vec![
            TypeRef::Path(Path::single(Name::new("Person"))),
            TypeRef::String,
        ]);
        let expected = TypeRef::Union(vec![
            TypeRef::Path(Path::new(vec![Name::new("stream"), Name::new("Person")])),
            TypeRef::String,
        ]);
        assert_eq!(stream_expand(&input, &names), expected);
    }

    #[test]
    fn test_expand_optional_becomes_union() {
        let names = make_names(&["Person"], &[], &[]);
        let input = TypeRef::Optional(Box::new(TypeRef::Path(Path::single(Name::new("Person")))));
        let expected = TypeRef::Union(vec![
            TypeRef::Path(Path::new(vec![Name::new("stream"), Name::new("Person")])),
            TypeRef::Null,
        ]);
        assert_eq!(stream_expand(&input, &names), expected);
    }

    // ─── stream_expand_class tests ──────────────────────────────────

    #[test]
    fn test_expand_class_basic() {
        let names = make_names(&["Education"], &["Status"], &[]);
        let class = Class {
            name: Name::new("Resume"),
            fields: vec![
                make_field("name", TypeRef::String),
                make_field(
                    "education",
                    TypeRef::List(Box::new(TypeRef::Path(Path::single(Name::new("Education"))))),
                ),
                make_field("status", TypeRef::Path(Path::single(Name::new("Status")))),
            ],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);

        assert_eq!(stream.name, Name::new("stream.Resume"));
        assert!(stream.compiler_generated.is_some());
        assert_eq!(stream.fields.len(), 3);

        // name: string → string | null
        assert_eq!(stream.fields[0].name, Name::new("name"));
        assert_eq!(
            stream.fields[0].type_ref,
            TypeRef::Union(vec![TypeRef::Null, TypeRef::String])
        );

        // education: Education[] → stream.Education[] (lists start empty, not null)
        assert_eq!(stream.fields[1].name, Name::new("education"));
        assert_eq!(
            stream.fields[1].type_ref,
            TypeRef::List(Box::new(TypeRef::Path(Path::new(vec![
                Name::new("stream"),
                Name::new("Education"),
            ]))))
        );

        // status: Status → Status | null (enums don't get stream. prefix)
        assert_eq!(stream.fields[2].name, Name::new("status"));
        assert_eq!(
            stream.fields[2].type_ref,
            TypeRef::Union(vec![
                TypeRef::Null,
                TypeRef::Path(Path::single(Name::new("Status")))
            ])
        );
    }

    #[test]
    fn test_expand_class_literal_field_absent() {
        let names = make_names(&[], &[], &[]);
        let class = Class {
            name: Name::new("Foo"),
            fields: vec![make_field("type", TypeRef::StringLiteral("resume".to_string()))],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        // Literal field: S=never, D="resume" → "resume" (absent until complete)
        assert_eq!(
            stream.fields[0].type_ref,
            TypeRef::StringLiteral("resume".to_string())
        );
    }

    // ─── stream_expand_class with user attributes (Phase 3) ────────

    #[test]
    fn test_expand_class_with_stream_starts_as_string_literal() {
        // @stream.starts_as("") → S = "", D = string → stream_type = "" | string = string
        // sap_annotations: [Missing(String(""))]
        let names = make_names(&[], &[], &[]);
        let mut field = make_field("name", TypeRef::String);
        field.stream_starts_as = Some(TypeRef::StringLiteral("".to_string()));

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        // S="" | D=string → Union(["", string])
        assert_eq!(
            stream.fields[0].type_ref,
            TypeRef::Union(vec![
                TypeRef::StringLiteral("".to_string()),
                TypeRef::String,
            ])
        );
        // Should have Missing("") annotation
        assert_eq!(
            stream.fields[0].sap_annotations,
            vec![SapAnnotation::Missing(SapValue::String("".to_string()))]
        );
    }

    #[test]
    fn test_expand_class_with_sap_completed() {
        // @sap.completed on a literal field
        let names = make_names(&[], &[], &[]);
        let mut field = make_field("type", TypeRef::StringLiteral("resume".to_string()));
        field.sap_completed = true;

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        // Should have both Missing(Never) and Completed annotations
        assert!(stream.fields[0]
            .sap_annotations
            .contains(&SapAnnotation::Completed));
    }

    #[test]
    fn test_expand_class_with_sap_must_start() {
        // @sap.must_start → S = never → field absent until parsing begins
        let names = make_names(&[], &[], &[]);
        let mut field = make_field("name", TypeRef::String);
        field.sap_must_start = true;

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        // S=never, D=string → string (no null!)
        assert_eq!(stream.fields[0].type_ref, TypeRef::String);
        // Should have Missing(Never) annotation
        assert_eq!(
            stream.fields[0].sap_annotations,
            vec![SapAnnotation::Missing(SapValue::Never)]
        );
    }

    #[test]
    fn test_expand_class_with_stream_type_override() {
        // @stream.type(Education[]) overrides the D computation
        let names = make_names(&["Education"], &[], &[]);
        let mut field = make_field(
            "education",
            TypeRef::List(Box::new(TypeRef::Path(Path::single(Name::new("Education"))))),
        );
        field.stream_type_attr = Some(TypeRef::List(Box::new(TypeRef::Path(Path::single(
            Name::new("Education"),
        )))));

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        // D = Education[] (user override, NOT stream.Education[])
        // S = never (list default) → type = Education[]
        assert_eq!(
            stream.fields[0].type_ref,
            TypeRef::List(Box::new(TypeRef::Path(Path::single(Name::new("Education")))))
        );
    }

    #[test]
    fn test_expand_class_with_stream_type_never_omits_field() {
        // @stream.type(never) → D = never → field omitted
        let names = make_names(&[], &[], &[]);
        let mut field = make_field("tag", TypeRef::StringLiteral("resume".to_string()));
        field.stream_type_attr = Some(TypeRef::Never);

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        // Field with @stream.type(never) should be omitted
        assert_eq!(stream.fields.len(), 0);
    }

    #[test]
    fn test_expand_class_with_stream_with_state() {
        // @stream.with_state flag should be carried to generated field
        let names = make_names(&[], &[], &[]);
        let mut field = make_field("name", TypeRef::String);
        field.stream_with_state = true;

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        assert!(stream.fields[0].stream_with_state);
    }

    #[test]
    fn test_expand_class_starts_as_never_makes_non_nullable() {
        // @stream.starts_as(never) → S = never → type = D only (not nullable)
        let names = make_names(&[], &[], &[]);
        let mut field = make_field("name", TypeRef::String);
        field.stream_starts_as = Some(TypeRef::Never);

        let class = Class {
            name: Name::new("Foo"),
            fields: vec![field],
            is_dynamic: Attribute::Unset,
            alias: Attribute::Unset,
            description: Attribute::Unset,
            compiler_generated: None,
        };

        let stream = stream_expand_class(&class, &names);
        assert_eq!(stream.fields.len(), 1);
        // S=never | D=string → string (no null!)
        assert_eq!(stream.fields[0].type_ref, TypeRef::String);
    }

    // ─── TypeRef::from_text tests ────────────────────────────────────

    #[test]
    fn test_from_text_primitives() {
        assert_eq!(TypeRef::from_text("string"), TypeRef::String);
        assert_eq!(TypeRef::from_text("int"), TypeRef::Int);
        assert_eq!(TypeRef::from_text("float"), TypeRef::Float);
        assert_eq!(TypeRef::from_text("bool"), TypeRef::Bool);
        assert_eq!(TypeRef::from_text("null"), TypeRef::Null);
        assert_eq!(TypeRef::from_text("never"), TypeRef::Never);
    }

    #[test]
    fn test_from_text_named_type() {
        assert_eq!(
            TypeRef::from_text("Person"),
            TypeRef::Path(Path::single(Name::new("Person")))
        );
    }

    #[test]
    fn test_from_text_array() {
        assert_eq!(
            TypeRef::from_text("Person[]"),
            TypeRef::List(Box::new(TypeRef::Path(Path::single(Name::new("Person")))))
        );
    }

    #[test]
    fn test_from_text_dotted_name() {
        assert_eq!(
            TypeRef::from_text("stream.Person"),
            TypeRef::Path(Path::new(vec![Name::new("stream"), Name::new("Person")]))
        );
    }

    // ─── stream_expand_type_alias tests ─────────────────────────────

    #[test]
    fn test_expand_type_alias() {
        let names = make_names(&["Bar"], &[], &[]);
        let alias = TypeAlias {
            name: Name::new("Foo"),
            type_ref: TypeRef::Union(vec![
                TypeRef::Path(Path::single(Name::new("Bar"))),
                TypeRef::String,
            ]),
            compiler_generated: None,
        };

        let stream = stream_expand_type_alias(&alias, &names);
        assert_eq!(stream.name, Name::new("stream.Foo"));
        assert!(stream.compiler_generated.is_some());
        assert_eq!(
            stream.type_ref,
            TypeRef::Union(vec![
                TypeRef::Path(Path::new(vec![Name::new("stream"), Name::new("Bar")])),
                TypeRef::String,
            ])
        );
    }
}
