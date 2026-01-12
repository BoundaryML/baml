//! Collects all types referenced by an output type.
//!
//! Given a target type (e.g., the return type of a function), this module
//! traverses the type graph and collects all enums and classes that need
//! to be included in the output format schema.

use std::collections::HashSet;

use baml_base::{MediaKind, Name};
use baml_compiler_tir::{class_field_types, enum_variants, Ty as TirTy};
use baml_db::baml_workspace::Project;
use baml_output_format::{
    Class, ClassField, Enum, EnumVariant, Name as OutputName, OutputFormatBuilder,
    OutputFormatContent,
};
use baml_project::ProjectDatabase;
use indexmap::IndexMap;

/// Collect all enums and classes referenced by the target type.
///
/// # Arguments
/// * `db` - Database for looking up type definitions
/// * `project` - The project containing type definitions
/// * `target` - The output type to analyze
///
/// # Returns
/// An `OutputFormatContent` containing all referenced enums and classes.
pub fn relevant_data_models(
    db: &ProjectDatabase,
    project: Project,
    target: &TirTy,
) -> OutputFormatContent {
    let mut collector = OutputFormatTypeCollector::new(db, project);
    collector.collect(target);
    collector.build(target)
}

struct OutputFormatTypeCollector<'db> {
    // NOTE: These fields are kept for future use (recursive class detection, attributes, etc.)
    #[allow(dead_code)]
    db: &'db ProjectDatabase,
    #[allow(dead_code)]
    project: Project,

    // Lookup caches (populated once at start)
    enum_variants: IndexMap<Name, Vec<Name>>,
    class_fields: IndexMap<Name, IndexMap<Name, TirTy>>,

    // Results
    enums: IndexMap<String, Enum>,
    classes: IndexMap<String, Class>,

    /// Types we've already visited, to avoid infinite loops on recursive types
    /// and prevent duplicate processing when the same type appears multiple times.
    visited_types: HashSet<String>,
}

impl<'db> OutputFormatTypeCollector<'db> {
    fn new(db: &'db ProjectDatabase, project: Project) -> Self {
        // Pre-load enum variants and class fields
        let enum_variants_map = enum_variants(db, project);
        let class_fields_map = class_field_types(db, project);

        // Convert from HashMap to IndexMap to preserve insertion order
        let enum_variants: IndexMap<Name, Vec<Name>> =
            enum_variants_map.enums(db).iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let class_fields: IndexMap<Name, IndexMap<Name, TirTy>> = class_fields_map
            .classes(db)
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().map(|(fk, fv)| (fk.clone(), fv.clone())).collect()))
            .collect();

        Self {
            db,
            project,
            enum_variants,
            class_fields,
            enums: IndexMap::new(),
            classes: IndexMap::new(),
            visited_types: HashSet::new(),
        }
    }

    fn collect(&mut self, target: &TirTy) {
        let mut stack: Vec<TirTy> = vec![target.clone()];

        while let Some(ty) = stack.pop() {
            // Create a string key for deduplication
            let key = format!("{:?}", ty);
            if !self.visited_types.insert(key) {
                continue; // Already processed
            }

            match &ty {
                // User-defined types - need to look up and collect
                TirTy::Enum(name) => {
                    self.collect_enum(name);
                }
                TirTy::Class(name) => {
                    self.collect_class(name, &mut stack);
                }
                TirTy::Named(name) => {
                    // Could be enum or class - check both
                    if self.enum_variants.contains_key(name) {
                        self.collect_enum(name);
                    } else if self.class_fields.contains_key(name) {
                        self.collect_class(name, &mut stack);
                    }
                    // If neither, it's an unresolved type - skip
                }

                // Composite types - recurse into inner types
                TirTy::Optional(inner) | TirTy::List(inner) | TirTy::WatchAccessor(inner) => {
                    stack.push(inner.as_ref().clone());
                }
                TirTy::Map { key, value } => {
                    stack.push(key.as_ref().clone());
                    stack.push(value.as_ref().clone());
                }
                TirTy::Union(variants) => {
                    for variant in variants {
                        stack.push(variant.clone());
                    }
                }
                TirTy::Function { params, ret } => {
                    // Functions in output types are rare but handle them
                    for param in params {
                        stack.push(param.clone());
                    }
                    stack.push(ret.as_ref().clone());
                }

                // Terminal types - no further traversal needed
                TirTy::Int | TirTy::Float | TirTy::String | TirTy::Bool | TirTy::Null => {}
                TirTy::Media(_) => {}
                TirTy::Literal(_) => {}
                TirTy::Unknown | TirTy::Error | TirTy::Void => {}
            }
        }
    }

    fn collect_enum(&mut self, name: &Name) {
        let name_str = name.to_string();
        if self.enums.contains_key(&name_str) {
            return; // Already collected
        }

        let Some(variants) = self.enum_variants.get(name) else {
            return; // Enum not found in database
        };

        let enum_def = Enum {
            name: OutputName::new(name_str.clone()),
            variants: variants
                .iter()
                .map(|v| EnumVariant {
                    name: OutputName::new(v.to_string()),
                    description: None, // Skip attributes for now
                })
                .collect(),
        };

        self.enums.insert(name_str, enum_def);
    }

    fn collect_class(&mut self, name: &Name, stack: &mut Vec<TirTy>) {
        let name_str = name.to_string();
        if self.classes.contains_key(&name_str) {
            return; // Already collected
        }

        let Some(fields) = self.class_fields.get(name) else {
            return; // Class not found in database
        };

        // Collect field types for further traversal
        let class_fields: Vec<ClassField> = fields
            .iter()
            .map(|(field_name, field_type)| {
                // Push field type onto stack for processing
                stack.push(field_type.clone());

                ClassField {
                    name: OutputName::new(field_name.to_string()),
                    field_type: tir_ty_to_base_ty(field_type),
                    description: None, // Skip attributes for now
                    required: !is_optional(field_type),
                }
            })
            .collect();

        let class_def = Class {
            name: OutputName::new(name_str.clone()),
            description: None, // Skip attributes for now
            fields: class_fields,
        };

        self.classes.insert(name_str, class_def);
    }

    fn build(self, target: &TirTy) -> OutputFormatContent {
        let mut builder = OutputFormatBuilder::new();

        for (_, e) in self.enums {
            builder = builder.with_enum(e);
        }

        for (_, c) in self.classes {
            builder = builder.with_class(c);
        }

        // Set the target type
        builder = builder.with_target(tir_ty_to_base_ty(target));

        builder.build()
    }
}

/// Convert a TIR type to a baml_base type.
///
/// This is necessary because baml_compiler_tir::Ty and baml_base::Ty have
/// slightly different representations (e.g., Media(kind) vs Image/Audio/Video/Pdf).
fn tir_ty_to_base_ty(ty: &TirTy) -> baml_base::Ty {
    match ty {
        TirTy::Int => baml_base::Ty::Int,
        TirTy::Float => baml_base::Ty::Float,
        TirTy::String => baml_base::Ty::String,
        TirTy::Bool => baml_base::Ty::Bool,
        TirTy::Null => baml_base::Ty::Null,
        TirTy::Media(kind) => match kind {
            MediaKind::Image => baml_base::Ty::Image,
            MediaKind::Audio => baml_base::Ty::Audio,
            MediaKind::Video => baml_base::Ty::Video,
            MediaKind::Pdf => baml_base::Ty::Pdf,
            MediaKind::Generic => baml_base::Ty::Image, // Default to image for generic
        },
        TirTy::Literal(lit) => baml_base::Ty::Literal(tir_literal_to_base_literal(lit)),
        TirTy::Class(name) => baml_base::Ty::Class(name.clone()),
        TirTy::Enum(name) => baml_base::Ty::Enum(name.clone()),
        TirTy::Named(name) => baml_base::Ty::Named(name.clone()),
        TirTy::Optional(inner) => baml_base::Ty::Optional(Box::new(tir_ty_to_base_ty(inner))),
        TirTy::List(inner) => baml_base::Ty::List(Box::new(tir_ty_to_base_ty(inner))),
        TirTy::Map { key, value } => baml_base::Ty::Map {
            key: Box::new(tir_ty_to_base_ty(key)),
            value: Box::new(tir_ty_to_base_ty(value)),
        },
        TirTy::Union(variants) => {
            baml_base::Ty::Union(variants.iter().map(tir_ty_to_base_ty).collect())
        }
        TirTy::Function { params, ret } => baml_base::Ty::Function {
            params: params.iter().map(tir_ty_to_base_ty).collect(),
            ret: Box::new(tir_ty_to_base_ty(ret)),
        },
        TirTy::Unknown => baml_base::Ty::Unknown,
        TirTy::Error => baml_base::Ty::Error,
        TirTy::Void => baml_base::Ty::Void,
        TirTy::WatchAccessor(inner) => {
            baml_base::Ty::WatchAccessor(Box::new(tir_ty_to_base_ty(inner)))
        }
    }
}

/// Convert a TIR literal value to a baml_base literal value.
fn tir_literal_to_base_literal(lit: &baml_compiler_tir::LiteralValue) -> baml_base::LiteralValue {
    match lit {
        baml_compiler_tir::LiteralValue::Int(v) => baml_base::LiteralValue::Int(*v),
        baml_compiler_tir::LiteralValue::Float(v) => baml_base::LiteralValue::Float(v.clone()),
        baml_compiler_tir::LiteralValue::String(v) => baml_base::LiteralValue::String(v.clone()),
        baml_compiler_tir::LiteralValue::Bool(v) => baml_base::LiteralValue::Bool(*v),
    }
}

/// Helper to check if a type is optional
fn is_optional(ty: &TirTy) -> bool {
    matches!(ty, TirTy::Optional(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tir_ty_to_base_ty_primitives() {
        assert_eq!(tir_ty_to_base_ty(&TirTy::Int), baml_base::Ty::Int);
        assert_eq!(tir_ty_to_base_ty(&TirTy::Float), baml_base::Ty::Float);
        assert_eq!(tir_ty_to_base_ty(&TirTy::String), baml_base::Ty::String);
        assert_eq!(tir_ty_to_base_ty(&TirTy::Bool), baml_base::Ty::Bool);
        assert_eq!(tir_ty_to_base_ty(&TirTy::Null), baml_base::Ty::Null);
    }

    #[test]
    fn test_tir_ty_to_base_ty_media() {
        assert_eq!(
            tir_ty_to_base_ty(&TirTy::Media(MediaKind::Image)),
            baml_base::Ty::Image
        );
        assert_eq!(
            tir_ty_to_base_ty(&TirTy::Media(MediaKind::Audio)),
            baml_base::Ty::Audio
        );
        assert_eq!(
            tir_ty_to_base_ty(&TirTy::Media(MediaKind::Video)),
            baml_base::Ty::Video
        );
        assert_eq!(
            tir_ty_to_base_ty(&TirTy::Media(MediaKind::Pdf)),
            baml_base::Ty::Pdf
        );
    }

    #[test]
    fn test_tir_ty_to_base_ty_composite() {
        // List
        let list_ty = TirTy::List(Box::new(TirTy::String));
        assert_eq!(
            tir_ty_to_base_ty(&list_ty),
            baml_base::Ty::List(Box::new(baml_base::Ty::String))
        );

        // Optional
        let opt_ty = TirTy::Optional(Box::new(TirTy::Int));
        assert_eq!(
            tir_ty_to_base_ty(&opt_ty),
            baml_base::Ty::Optional(Box::new(baml_base::Ty::Int))
        );

        // Map
        let map_ty = TirTy::Map {
            key: Box::new(TirTy::String),
            value: Box::new(TirTy::Int),
        };
        assert_eq!(
            tir_ty_to_base_ty(&map_ty),
            baml_base::Ty::Map {
                key: Box::new(baml_base::Ty::String),
                value: Box::new(baml_base::Ty::Int),
            }
        );
    }

    #[test]
    fn test_is_optional() {
        assert!(is_optional(&TirTy::Optional(Box::new(TirTy::String))));
        assert!(!is_optional(&TirTy::String));
        assert!(!is_optional(&TirTy::List(Box::new(TirTy::String))));
    }
}
