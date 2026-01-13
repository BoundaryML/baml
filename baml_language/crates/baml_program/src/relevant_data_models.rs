//! Collects all types referenced by an output type.
//!
//! Given a target type (e.g., the return type of a function), this module
//! traverses the type graph and collects all enums and classes that need
//! to be included in the output format schema.
//!
//! This module also detects recursive cycles using Tarjan's strongly connected
//! components algorithm, marking classes that are part of cycles as recursive.

use std::collections::{HashMap, HashSet};

use baml_base::{MediaKind, Name};
use baml_compiler_hir::project_class_fields;
use baml_compiler_tir::{class_field_types, enum_variants, type_aliases, Ty as TirTy};
use baml_db::baml_workspace::Project;
use baml_output_format::{
    Class, ClassField, Enum, EnumVariant, Name as OutputName, OutputFormatBuilder,
    OutputFormatContent,
};
use baml_project::ProjectDatabase;
use indexmap::{IndexMap, IndexSet};

use crate::tarjan::{Graph, Tarjan};

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
    // NOTE: These fields are kept for future use (attributes, etc.)
    #[allow(dead_code)]
    db: &'db ProjectDatabase,
    #[allow(dead_code)]
    project: Project,

    // Lookup caches (populated once at start)
    enum_variants: IndexMap<Name, Vec<Name>>,
    /// Class fields with types from TIR (types resolved)
    class_fields: IndexMap<Name, IndexMap<Name, TirTy>>,
    /// Class field order from HIR (preserves source order)
    class_field_order: HashMap<String, Vec<String>>,
    /// Type alias definitions: alias_name -> resolved_type
    type_alias_defs: HashMap<Name, TirTy>,

    // Results
    enums: IndexMap<String, Enum>,
    classes: IndexMap<String, Class>,
    /// Structural recursive type aliases that need to be rendered
    structural_recursive_aliases: IndexMap<String, baml_base::Ty>,

    /// Types we've already visited, to avoid infinite loops on recursive types
    /// and prevent duplicate processing when the same type appears multiple times.
    visited_types: HashSet<String>,

    /// Classes we've collected, in order of discovery.
    /// Used to build the dependency graph for cycle detection.
    collected_class_names: Vec<String>,

    /// Type alias names we've collected, for cycle detection.
    collected_alias_names: Vec<String>,
}

impl<'db> OutputFormatTypeCollector<'db> {
    fn new(db: &'db ProjectDatabase, project: Project) -> Self {
        // Pre-load enum variants and class fields
        let enum_variants_map = enum_variants(db, project);
        let class_fields_map = class_field_types(db, project);
        let type_aliases_map = type_aliases(db, project);
        let hir_class_fields = project_class_fields(db, project);

        // Convert from HashMap to IndexMap to preserve insertion order
        let enum_variants: IndexMap<Name, Vec<Name>> =
            enum_variants_map.enums(db).iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let class_fields: IndexMap<Name, IndexMap<Name, TirTy>> = class_fields_map
            .classes(db)
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().map(|(fk, fv)| (fk.clone(), fv.clone())).collect()))
            .collect();
        let type_alias_defs: HashMap<Name, TirTy> = type_aliases_map
            .aliases(db)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Build field order from HIR (preserves source order)
        let class_field_order: HashMap<String, Vec<String>> = hir_class_fields
            .classes(db)
            .iter()
            .map(|(class_name, fields)| {
                let field_names: Vec<String> = fields.iter().map(|(name, _)| name.to_string()).collect();
                (class_name.to_string(), field_names)
            })
            .collect();

        Self {
            db,
            project,
            enum_variants,
            class_fields,
            class_field_order,
            type_alias_defs,
            enums: IndexMap::new(),
            classes: IndexMap::new(),
            structural_recursive_aliases: IndexMap::new(),
            visited_types: HashSet::new(),
            collected_class_names: Vec::new(),
            collected_alias_names: Vec::new(),
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
                    // Could be enum, class, or type alias - check all
                    if self.enum_variants.contains_key(name) {
                        self.collect_enum(name);
                    } else if self.class_fields.contains_key(name) {
                        self.collect_class(name, &mut stack);
                    } else if let Some(resolved_type) = self.type_alias_defs.get(name).cloned() {
                        // It's a type alias - collect it
                        self.collect_type_alias(name, &resolved_type, &mut stack);
                    }
                    // If none, it's an unresolved type - skip
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

        // Track this class for cycle detection
        self.collected_class_names.push(name_str.clone());

        // Get field order from HIR (preserves source order)
        let field_order = self.class_field_order.get(&name_str);

        // Collect field types for further traversal, preserving source order
        let class_fields: Vec<ClassField> = if let Some(order) = field_order {
            // Use HIR field order
            order
                .iter()
                .filter_map(|field_name| {
                    let name_key = Name::new(field_name.clone());
                    fields.get(&name_key).map(|field_type| {
                        // Push field type onto stack for processing
                        stack.push(field_type.clone());

                        ClassField {
                            name: OutputName::new(field_name.clone()),
                            field_type: tir_ty_to_base_ty(field_type),
                            description: None, // Skip attributes for now
                            required: !is_optional(field_type),
                        }
                    })
                })
                .collect()
        } else {
            // Fallback: use TIR order (may not be source order)
            fields
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
                .collect()
        };

        let class_def = Class {
            name: OutputName::new(name_str.clone()),
            description: None, // Skip attributes for now
            fields: class_fields,
        };

        self.classes.insert(name_str, class_def);
    }

    fn collect_type_alias(&mut self, name: &Name, resolved_type: &TirTy, stack: &mut Vec<TirTy>) {
        let name_str = name.to_string();

        // Track this alias for cycle detection
        if !self.collected_alias_names.contains(&name_str) {
            self.collected_alias_names.push(name_str.clone());
        }

        // Check if this alias creates a recursive cycle by seeing if the resolved
        // type references this alias name. If so, add it to structural_recursive_aliases.
        if self.type_references_alias(resolved_type, &name_str) {
            if !self.structural_recursive_aliases.contains_key(&name_str) {
                let base_ty = tir_ty_to_base_ty(resolved_type);
                self.structural_recursive_aliases.insert(name_str, base_ty);
            }
        }

        // Push the resolved type onto the stack for further traversal
        stack.push(resolved_type.clone());
    }

    /// Check if a type references a given alias name (directly or indirectly).
    fn type_references_alias(&self, ty: &TirTy, alias_name: &str) -> bool {
        let mut visited = HashSet::new();
        self.type_references_alias_impl(ty, alias_name, &mut visited)
    }

    fn type_references_alias_impl(&self, ty: &TirTy, alias_name: &str, visited: &mut HashSet<String>) -> bool {
        match ty {
            TirTy::Named(name) => {
                let name_str = name.to_string();
                if name_str == alias_name {
                    return true;
                }
                // Prevent infinite loops by tracking visited aliases
                if !visited.insert(name_str.clone()) {
                    return false; // Already visited this alias
                }
                // Check if this named type resolves to something that references the alias
                if let Some(resolved) = self.type_alias_defs.get(name) {
                    return self.type_references_alias_impl(resolved, alias_name, visited);
                }
                false
            }
            TirTy::Class(name) | TirTy::Enum(name) => name.to_string() == alias_name,
            TirTy::Optional(inner) | TirTy::List(inner) | TirTy::WatchAccessor(inner) => {
                self.type_references_alias_impl(inner, alias_name, visited)
            }
            TirTy::Map { key, value } => {
                self.type_references_alias_impl(key, alias_name, visited)
                    || self.type_references_alias_impl(value, alias_name, visited)
            }
            TirTy::Union(variants) => {
                variants.iter().any(|v| self.type_references_alias_impl(v, alias_name, visited))
            }
            TirTy::Function { params, ret } => {
                params.iter().any(|p| self.type_references_alias_impl(p, alias_name, visited))
                    || self.type_references_alias_impl(ret, alias_name, visited)
            }
            _ => false,
        }
    }

    /// Build a dependency graph from collected classes.
    ///
    /// The graph maps each class name to the set of class names it references.
    fn build_dependency_graph(&self) -> Graph<String> {
        let mut graph: Graph<String> = std::collections::HashMap::new();

        for class_name in &self.collected_class_names {
            let mut deps: HashSet<String> = HashSet::new();

            // Find the Name key that matches this class
            if let Some((_, fields)) = self
                .class_fields
                .iter()
                .find(|(n, _)| n.to_string() == *class_name)
            {
                for (_, field_type) in fields {
                    self.extract_class_refs(field_type, &mut deps);
                }
            }

            graph.insert(class_name.clone(), deps);
        }

        graph
    }

    /// Extract class references from a type.
    ///
    /// Recursively walks the type structure and collects all class names
    /// that are referenced, but only if they're in our collected set.
    fn extract_class_refs(&self, ty: &TirTy, refs: &mut HashSet<String>) {
        match ty {
            TirTy::Class(name) | TirTy::Named(name) => {
                let name_str = name.to_string();
                // Only include if it's in our collected classes
                if self.collected_class_names.contains(&name_str) {
                    refs.insert(name_str);
                }
            }
            TirTy::Optional(inner) | TirTy::List(inner) | TirTy::WatchAccessor(inner) => {
                self.extract_class_refs(inner, refs);
            }
            TirTy::Map { key, value } => {
                self.extract_class_refs(key, refs);
                self.extract_class_refs(value, refs);
            }
            TirTy::Union(variants) => {
                for v in variants {
                    self.extract_class_refs(v, refs);
                }
            }
            TirTy::Function { params, ret } => {
                for p in params {
                    self.extract_class_refs(p, refs);
                }
                self.extract_class_refs(ret, refs);
            }
            // Terminal types - no class references
            TirTy::Int
            | TirTy::Float
            | TirTy::String
            | TirTy::Bool
            | TirTy::Null
            | TirTy::Media(_)
            | TirTy::Literal(_)
            | TirTy::Enum(_)
            | TirTy::Unknown
            | TirTy::Error
            | TirTy::Void => {}
        }
    }

    /// Compute recursive classes using Tarjan's algorithm.
    ///
    /// Returns a set of all class names that are part of a cycle.
    fn compute_recursive_classes(&self) -> IndexSet<String> {
        let graph = self.build_dependency_graph();
        let cycles = Tarjan::components(&graph);

        let mut recursive: IndexSet<String> = IndexSet::new();
        for cycle in cycles {
            for class_name in cycle {
                recursive.insert(class_name);
            }
        }
        recursive
    }

    fn build(self, target: &TirTy) -> OutputFormatContent {
        // Compute recursive classes before consuming self
        let recursive_classes = self.compute_recursive_classes();

        let mut builder = OutputFormatBuilder::new();

        for (_, e) in self.enums {
            builder = builder.with_enum(e);
        }

        for (_, c) in self.classes {
            builder = builder.with_class(c);
        }

        // Add recursive classes
        for name in recursive_classes {
            builder = builder.with_recursive_class(name);
        }

        // Add structural recursive aliases
        for (name, ty) in self.structural_recursive_aliases {
            builder = builder.with_structural_recursive_alias(name, ty);
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
