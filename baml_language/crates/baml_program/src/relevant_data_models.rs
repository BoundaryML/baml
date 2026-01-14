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
use baml_compiler_hir::{
    file_item_tree, project_class_fields, project_items, ClassId, EnumId, ItemId,
};
use baml_compiler_tir::{class_field_types, enum_variants, type_aliases, Ty as TirTy};
use baml_db::baml_workspace::Project;
use baml_output_format::{
    Class, ClassField, Enum, EnumVariant, Name as OutputName, OutputFormatBuilder,
    OutputFormatContent,
};
use baml_project::ProjectDatabase;
use indexmap::{IndexMap, IndexSet};

use crate::tarjan::{Graph, Tarjan};

/// Context for resolving type aliases during type conversion.
struct TypeAliasContext {
    /// Type alias definitions: alias_name -> resolved_type
    defs: HashMap<Name, TirTy>,
    /// Names of aliases that are recursive (reference themselves)
    recursive: HashSet<String>,
}

impl TypeAliasContext {
    /// Create a new type alias context, computing which aliases are recursive.
    fn new(type_alias_defs: HashMap<Name, TirTy>) -> Self {
        let mut recursive = HashSet::new();

        // Find all recursive aliases
        for name in type_alias_defs.keys() {
            if Self::type_references_name_static(&type_alias_defs, &type_alias_defs[name], &name.to_string(), &mut HashSet::new()) {
                recursive.insert(name.to_string());
            }
        }

        Self {
            defs: type_alias_defs,
            recursive,
        }
    }

    /// Check if a type references a given name (directly or indirectly through aliases).
    fn type_references_name_static(
        type_alias_defs: &HashMap<Name, TirTy>,
        ty: &TirTy,
        target_name: &str,
        visited: &mut HashSet<String>,
    ) -> bool {
        match ty {
            TirTy::Named(name) => {
                let name_str = name.to_string();
                if name_str == target_name {
                    return true;
                }
                // Prevent infinite loops
                if !visited.insert(name_str.clone()) {
                    return false;
                }
                // Check resolved type
                if let Some(resolved) = type_alias_defs.get(name) {
                    let result = Self::type_references_name_static(type_alias_defs, resolved, target_name, visited);
                    visited.remove(&name_str);
                    return result;
                }
                visited.remove(&name_str);
                false
            }
            TirTy::Optional(inner) | TirTy::List(inner) | TirTy::WatchAccessor(inner) => {
                Self::type_references_name_static(type_alias_defs, inner, target_name, visited)
            }
            TirTy::Map { key, value } => {
                Self::type_references_name_static(type_alias_defs, key, target_name, visited)
                    || Self::type_references_name_static(type_alias_defs, value, target_name, visited)
            }
            TirTy::Union(variants) => {
                variants.iter().any(|v| Self::type_references_name_static(type_alias_defs, v, target_name, visited))
            }
            TirTy::Function { params, ret } => {
                params.iter().any(|p| Self::type_references_name_static(type_alias_defs, p, target_name, visited))
                    || Self::type_references_name_static(type_alias_defs, ret, target_name, visited)
            }
            _ => false,
        }
    }

    /// Check if a type alias name is recursive.
    fn is_recursive(&self, name: &str) -> bool {
        self.recursive.contains(name)
    }

    /// Get the resolved type for a type alias name.
    fn get(&self, name: &Name) -> Option<&TirTy> {
        self.defs.get(name)
    }
}

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
    db: &'db ProjectDatabase,
    #[allow(dead_code)]
    project: Project,

    // Lookup caches (populated once at start)
    enum_variants: IndexMap<Name, Vec<Name>>,
    /// Class fields with types from TIR (types resolved)
    class_fields: IndexMap<Name, IndexMap<Name, TirTy>>,
    /// Class field order from HIR (preserves source order)
    class_field_order: HashMap<String, Vec<String>>,
    /// Type alias context with recursive alias tracking
    type_alias_ctx: TypeAliasContext,
    /// Class HIR locations for attribute lookup
    class_locs: HashMap<String, ClassId<'db>>,
    /// Enum HIR locations for attribute lookup
    enum_locs: HashMap<String, EnumId<'db>>,

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

        // Create type alias context with recursive alias tracking
        let type_alias_ctx = TypeAliasContext::new(type_alias_defs);

        // Build class and enum location maps for attribute lookup
        let mut class_locs: HashMap<String, ClassId<'db>> = HashMap::new();
        let mut enum_locs: HashMap<String, EnumId<'db>> = HashMap::new();

        for item in project_items(db, project).items(db) {
            match item {
                ItemId::Class(class_id) => {
                    let file = class_id.file(db);
                    let item_tree = file_item_tree(db, file);
                    let class_data = &item_tree[class_id.id(db)];
                    class_locs.insert(class_data.name.to_string(), *class_id);
                }
                ItemId::Enum(enum_id) => {
                    let file = enum_id.file(db);
                    let item_tree = file_item_tree(db, file);
                    let enum_data = &item_tree[enum_id.id(db)];
                    enum_locs.insert(enum_data.name.to_string(), *enum_id);
                }
                _ => {}
            }
        }

        Self {
            db,
            project,
            enum_variants,
            class_fields,
            class_field_order,
            type_alias_ctx,
            class_locs,
            enum_locs,
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
                    } else if let Some(resolved_type) = self.type_alias_ctx.get(name).cloned() {
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

        // Get HIR enum data for attributes
        let hir_enum = self.enum_locs.get(&name_str).map(|enum_id| {
            let file = enum_id.file(self.db);
            let item_tree = file_item_tree(self.db, file);
            let enum_data = &item_tree[enum_id.id(self.db)];
            (
                enum_data.alias.value().cloned(),
                enum_data.variants.clone(),
            )
        });

        let (enum_alias, hir_variants) = hir_enum.unwrap_or((None, vec![]));

        // Build a map of variant name -> (alias, description, skip) from HIR
        let variant_attrs: HashMap<String, (Option<String>, Option<String>, bool)> = hir_variants
            .iter()
            .map(|v| {
                (
                    v.name.to_string(),
                    (
                        v.alias.value().cloned(),
                        v.description.value().cloned(),
                        v.skip.is_explicit(),
                    ),
                )
            })
            .collect();

        let enum_output_name = if let Some(alias_val) = enum_alias {
            OutputName::with_alias(name_str.clone(), alias_val)
        } else {
            OutputName::new(name_str.clone())
        };

        let enum_def = Enum {
            name: enum_output_name,
            variants: variants
                .iter()
                .filter_map(|v| {
                    let variant_name_str = v.to_string();
                    let (alias, description, skip) = variant_attrs
                        .get(&variant_name_str)
                        .cloned()
                        .unwrap_or((None, None, false));

                    // Skip variants with @skip attribute
                    if skip {
                        return None;
                    }

                    let output_name = if let Some(alias_val) = alias {
                        OutputName::with_alias(variant_name_str, alias_val)
                    } else {
                        OutputName::new(variant_name_str)
                    };

                    Some(EnumVariant {
                        name: output_name,
                        description,
                    })
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

        // Get HIR class data for attributes
        let hir_class = self.class_locs.get(&name_str).map(|class_id| {
            let file = class_id.file(self.db);
            let item_tree = file_item_tree(self.db, file);
            // Clone the class data to avoid borrowing issues
            let class_data = &item_tree[class_id.id(self.db)];
            (
                class_data.alias.value().cloned(),
                class_data.description.value().cloned(),
                class_data.fields.clone(),
            )
        });

        let (class_alias, class_description, hir_fields) = hir_class.unwrap_or((None, None, vec![]));

        // Build a map of field name -> (alias, description, skip) from HIR
        let field_attrs: HashMap<String, (Option<String>, Option<String>, bool)> = hir_fields
            .iter()
            .map(|f| {
                (
                    f.name.to_string(),
                    (
                        f.alias.value().cloned(),
                        f.description.value().cloned(),
                        f.skip.is_explicit(),
                    ),
                )
            })
            .collect();

        // Get field order from HIR (preserves source order)
        let field_order = self.class_field_order.get(&name_str);

        // Collect field types for further traversal, preserving source order
        let class_fields: Vec<ClassField> = if let Some(order) = field_order {
            // Use HIR field order
            order
                .iter()
                .filter_map(|field_name| {
                    let name_key = Name::new(field_name.clone());
                    fields.get(&name_key).and_then(|field_type| {
                        // Get field attributes
                        let (alias, description, skip) = field_attrs
                            .get(field_name)
                            .cloned()
                            .unwrap_or((None, None, false));

                        // Skip fields with @skip attribute
                        if skip {
                            return None;
                        }

                        // Push field type onto stack for processing
                        stack.push(field_type.clone());

                        let output_name = if let Some(alias_val) = alias {
                            OutputName::with_alias(field_name.clone(), alias_val)
                        } else {
                            OutputName::new(field_name.clone())
                        };

                        Some(ClassField {
                            name: output_name,
                            field_type: tir_ty_to_base_ty_with_alias_ctx(field_type, &self.type_alias_ctx, &mut HashSet::new()),
                            description,
                            required: !is_optional(field_type),
                        })
                    })
                })
                .collect()
        } else {
            // Fallback: use TIR order (may not be source order)
            fields
                .iter()
                .filter_map(|(field_name, field_type)| {
                    let field_name_str = field_name.to_string();
                    // Get field attributes
                    let (alias, description, skip) = field_attrs
                        .get(&field_name_str)
                        .cloned()
                        .unwrap_or((None, None, false));

                    // Skip fields with @skip attribute
                    if skip {
                        return None;
                    }

                    // Push field type onto stack for processing
                    stack.push(field_type.clone());

                    let output_name = if let Some(alias_val) = alias {
                        OutputName::with_alias(field_name_str, alias_val)
                    } else {
                        OutputName::new(field_name_str)
                    };

                    Some(ClassField {
                        name: output_name,
                        field_type: tir_ty_to_base_ty_with_alias_ctx(field_type, &self.type_alias_ctx, &mut HashSet::new()),
                        description,
                        required: !is_optional(field_type),
                    })
                })
                .collect()
        };

        let class_output_name = if let Some(alias_val) = class_alias {
            OutputName::with_alias(name_str.clone(), alias_val)
        } else {
            OutputName::new(name_str.clone())
        };

        let class_def = Class {
            name: class_output_name,
            description: class_description,
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

        // If this alias is recursive, add it to structural_recursive_aliases
        if self.type_alias_ctx.is_recursive(&name_str) {
            if !self.structural_recursive_aliases.contains_key(&name_str) {
                // For recursive aliases, convert to base type (will keep Named references)
                let base_ty = tir_ty_to_base_ty_with_alias_ctx(resolved_type, &self.type_alias_ctx, &mut HashSet::new());
                self.structural_recursive_aliases.insert(name_str, base_ty);
            }
        }

        // Push the resolved type onto the stack for further traversal
        stack.push(resolved_type.clone());
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
    /// Resolves type aliases to find underlying class references.
    fn extract_class_refs(&self, ty: &TirTy, refs: &mut HashSet<String>) {
        self.extract_class_refs_impl(ty, refs, &mut HashSet::new());
    }

    fn extract_class_refs_impl(&self, ty: &TirTy, refs: &mut HashSet<String>, visited_aliases: &mut HashSet<String>) {
        match ty {
            TirTy::Class(name) => {
                let name_str = name.to_string();
                // Only include if it's in our collected classes
                if self.collected_class_names.contains(&name_str) {
                    refs.insert(name_str);
                }
            }
            TirTy::Named(name) => {
                let name_str = name.to_string();
                // First check if it's a collected class
                if self.collected_class_names.contains(&name_str) {
                    refs.insert(name_str);
                } else if let Some(resolved) = self.type_alias_ctx.get(name) {
                    // It's a type alias - resolve it and extract refs from the resolved type
                    // Prevent infinite loops by tracking visited aliases
                    if visited_aliases.insert(name_str) {
                        self.extract_class_refs_impl(resolved, refs, visited_aliases);
                    }
                }
            }
            TirTy::Optional(inner) | TirTy::List(inner) | TirTy::WatchAccessor(inner) => {
                self.extract_class_refs_impl(inner, refs, visited_aliases);
            }
            TirTy::Map { key, value } => {
                self.extract_class_refs_impl(key, refs, visited_aliases);
                self.extract_class_refs_impl(value, refs, visited_aliases);
            }
            TirTy::Union(variants) => {
                for v in variants {
                    self.extract_class_refs_impl(v, refs, visited_aliases);
                }
            }
            TirTy::Function { params, ret } => {
                for p in params {
                    self.extract_class_refs_impl(p, refs, visited_aliases);
                }
                self.extract_class_refs_impl(ret, refs, visited_aliases);
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

        // Set the target type, resolving type aliases
        let target_ty = tir_ty_to_base_ty_with_alias_ctx(target, &self.type_alias_ctx, &mut HashSet::new());
        builder = builder.with_target(target_ty);

        builder.build()
    }
}

/// Convert a TIR type to a baml_base type, resolving type aliases.
///
/// This function resolves non-recursive type aliases to their underlying types.
/// Recursive aliases are kept as `Ty::Named` since they're handled separately
/// via `structural_recursive_aliases`.
fn tir_ty_to_base_ty_with_alias_ctx(
    ty: &TirTy,
    alias_ctx: &TypeAliasContext,
    visited: &mut HashSet<String>,
) -> baml_base::Ty {
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
            MediaKind::Generic => baml_base::Ty::Image,
        },
        TirTy::Literal(lit) => baml_base::Ty::Literal(tir_literal_to_base_literal(lit)),
        TirTy::Class(name) => baml_base::Ty::Class(name.clone()),
        TirTy::Enum(name) => baml_base::Ty::Enum(name.clone()),
        TirTy::Named(name) => {
            let name_str = name.to_string();

            // Recursive aliases: keep as Named (handled via structural_recursive_aliases)
            if alias_ctx.is_recursive(&name_str) {
                return baml_base::Ty::Named(name.clone());
            }

            // Prevent infinite loops during resolution
            if !visited.insert(name_str.clone()) {
                return baml_base::Ty::Named(name.clone());
            }

            // Non-recursive alias: resolve to underlying type
            if let Some(resolved) = alias_ctx.get(name) {
                let result = tir_ty_to_base_ty_with_alias_ctx(resolved, alias_ctx, visited);
                visited.remove(&name_str);
                return result;
            }

            // Not a type alias (class/enum handled elsewhere, or unknown)
            visited.remove(&name_str);
            baml_base::Ty::Named(name.clone())
        }
        TirTy::Optional(inner) => {
            baml_base::Ty::Optional(Box::new(tir_ty_to_base_ty_with_alias_ctx(inner, alias_ctx, visited)))
        }
        TirTy::List(inner) => {
            baml_base::Ty::List(Box::new(tir_ty_to_base_ty_with_alias_ctx(inner, alias_ctx, visited)))
        }
        TirTy::Map { key, value } => baml_base::Ty::Map {
            key: Box::new(tir_ty_to_base_ty_with_alias_ctx(key, alias_ctx, visited)),
            value: Box::new(tir_ty_to_base_ty_with_alias_ctx(value, alias_ctx, visited)),
        },
        TirTy::Union(variants) => {
            baml_base::Ty::Union(
                variants
                    .iter()
                    .map(|v| tir_ty_to_base_ty_with_alias_ctx(v, alias_ctx, visited))
                    .collect(),
            )
        }
        TirTy::Function { params, ret } => baml_base::Ty::Function {
            params: params
                .iter()
                .map(|p| tir_ty_to_base_ty_with_alias_ctx(p, alias_ctx, visited))
                .collect(),
            ret: Box::new(tir_ty_to_base_ty_with_alias_ctx(ret, alias_ctx, visited)),
        },
        TirTy::Unknown => baml_base::Ty::Unknown,
        TirTy::Error => baml_base::Ty::Error,
        TirTy::Void => baml_base::Ty::Void,
        TirTy::WatchAccessor(inner) => {
            baml_base::Ty::WatchAccessor(Box::new(tir_ty_to_base_ty_with_alias_ctx(inner, alias_ctx, visited)))
        }
    }
}

/// Convert a TIR type to a baml_base type (without alias resolution).
///
/// This is the legacy function kept for tests. Use `tir_ty_to_base_ty_with_alias_ctx`
/// in the collector for proper type alias handling.
#[cfg(test)]
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
            MediaKind::Generic => baml_base::Ty::Image,
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
