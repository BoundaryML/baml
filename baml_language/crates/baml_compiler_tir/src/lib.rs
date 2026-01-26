//! Typed Intermediate Representation (TIR).
//!
//! Provides type checking and inference for BAML.
//!
//! # Architecture
//!
//! The TIR layer performs bidirectional type checking:
//! - **Inference (synthesize)**: Compute the type of an expression from its structure
//! - **Checking**: Verify an expression has an expected type
//!
//! This follows patterns from rust-analyzer and ruff for incremental type checking.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use baml_base::{FileId, Name, Span};
use baml_compiler_diagnostics::TypeError;
use baml_compiler_hir::{
    ErrorLocation, ExprBody, ExprId, FunctionBody, FunctionLoc, FunctionSignature, MatchArmId,
    Pattern, SignatureSourceMap, StmtId, TirContext,
};
use baml_workspace::Project;

/// Type alias for TIR type errors.
///
/// Uses `TirContext<Ty>` which has:
/// - `Ty` as the type representation
/// - `ErrorLocation` as the location (position-independent IDs)
///
/// This enables Salsa caching to work correctly - whitespace changes don't
/// invalidate type inference results because locations use IDs instead of spans.
pub type TirTypeError = TypeError<TirContext<Ty>>;

pub mod builtins;
mod exhaustiveness;
mod lower;
mod normalize;
pub mod pretty;
mod resolve;
mod types;

pub use builtins::{
    Bindings, lookup_function, lookup_method, match_pattern, method_param_types,
    method_return_type, substitute,
};
pub use exhaustiveness::{ExhaustivenessChecker, ExhaustivenessResult, ValueSet};
pub use lower::{TypeLoweringContext, lower_type_ref_validated_resolved};
pub use pretty::{expr_to_string, render_body_tree, render_function_tree};
pub use resolve::{ResolutionMap, ResolvedMethod, ResolvedValue, resolve_method};
use text_size::TextRange;
pub use types::*;

/// Substitute type variable bindings into a `TypePattern`, falling back to `Ty::Unknown`
/// for unbound type variables.
///
/// This is used for builtin function type inference where some type variables may be
/// bound from arguments but others might not be.
fn substitute_with_fallback(pattern: &baml_builtins::TypePattern, bindings: &Bindings) -> Ty {
    use baml_builtins::TypePattern;
    match pattern {
        TypePattern::Var(name) => bindings.get(name).cloned().unwrap_or(Ty::Unknown),
        TypePattern::Int => Ty::Int,
        TypePattern::Float => Ty::Float,
        TypePattern::String => Ty::String,
        TypePattern::Bool => Ty::Bool,
        TypePattern::Null => Ty::Null,
        TypePattern::Array(elem) => Ty::List(Box::new(substitute_with_fallback(elem, bindings))),
        TypePattern::Map { key, value } => Ty::Map {
            key: Box::new(substitute_with_fallback(key, bindings)),
            value: Box::new(substitute_with_fallback(value, bindings)),
        },
        TypePattern::Media => Ty::Media(baml_base::MediaKind::Generic),
        TypePattern::Optional(inner) => {
            Ty::Optional(Box::new(substitute_with_fallback(inner, bindings)))
        }
        TypePattern::Builtin(path) => Ty::Builtin((*path).to_string()),
    }
}

// ============================================================================
// Path Resolution
// ============================================================================

/// Resolved path categories after name resolution.
///
/// When we encounter a multi-segment path like `user.name.length` or `Status.Active`,
/// we need to determine what it actually refers to. This enum captures the different
/// possibilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPath {
    /// Local variable: `user` or `user.name.length`
    /// The name is the local variable. Field access types are in `path_segment_types`.
    Local { name: Name },

    /// Enum variant: `Status.Active`
    /// First segment is the enum type, second is the variant name.
    EnumVariant { enum_name: Name, variant_name: Name },

    /// Module item: `baml.HttpMethod.Get`
    /// The path navigates through modules to reach an item.
    ModuleItem {
        module_path: Vec<Name>,
        item_name: Name,
    },

    /// Function reference: `MyFunction`
    /// A single-segment path that resolves to a function.
    Function { name: Name },

    /// Method call on a type: `image.from_url`
    /// Used when the receiver is a type with associated methods.
    Method {
        receiver_type: Name,
        method_name: Name,
    },

    /// Unknown/unresolved path
    Unknown,
}

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for TIR queries.
///
/// Extends `baml_compiler_hir::Db`. Use the free functions in this crate
/// (e.g., `typing_context`, `class_field_types`) for TIR queries.
#[salsa::db]
pub trait Db: baml_compiler_hir::Db {}

// ============================================================================
// Tracked Struct for Enum Variants
// ============================================================================

/// Tracked struct holding enum variants (enum name -> variant names).
#[salsa::tracked]
pub struct EnumVariantsMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub enums: HashMap<Name, Vec<Name>>,
}

/// Tracked struct holding function types (function name -> function type).
#[salsa::tracked]
pub struct TypingContextMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub functions: HashMap<Name, Ty>,
}

/// Tracked struct holding class field types (class name -> field name -> field type).
#[salsa::tracked]
pub struct ClassFieldTypesMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: HashMap<Name, HashMap<Name, Ty>>,
}

/// Tracked struct holding type aliases (alias name -> resolved type).
#[salsa::tracked]
pub struct TypeAliasesMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub aliases: HashMap<Name, Ty>,
}

/// Tracked struct holding class names.
#[salsa::tracked]
pub struct ClassNamesSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: HashSet<Name>,
}

/// Tracked struct holding enum names.
#[salsa::tracked]
pub struct EnumNamesSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: HashSet<Name>,
}

/// Tracked struct holding all known type names (classes, enums, type aliases).
#[salsa::tracked]
pub struct KnownTypesSet<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: HashSet<Name>,
}

// ============================================================================
// TIR Queries
// ============================================================================

/// Query: Get enum variants for a project.
///
/// Maps enum names to their variant names, e.g.:
/// `Status` -> `[Active, Inactive, Pending]`
#[salsa::tracked]
pub fn enum_variants(db: &dyn Db, project: Project) -> EnumVariantsMap<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut enums = HashMap::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Enum(enum_loc) = item {
            let file = enum_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];

            let variants: Vec<Name> = enum_data.variants.iter().map(|v| v.name.clone()).collect();
            enums.insert(enum_data.name.clone(), variants);
        }
    }

    EnumVariantsMap::new(db, enums)
}

// ============================================================================
// Salsa Tracked Queries
// ============================================================================
//
// These queries compute type-related data and are cached by Salsa.

/// Query: Get the typing context for a project.
///
/// Maps function names to their arrow types.
#[salsa::tracked]
pub fn typing_context(db: &dyn Db, project: Project) -> TypingContextMap<'_> {
    let resolution_ctx = TypeResolutionContext::new(db, project);
    let mut context = HashMap::new();

    for file in project.files(db) {
        let items_struct = baml_compiler_hir::file_items(db, *file);
        let items = items_struct.items(db);

        for item in items {
            if let baml_compiler_hir::ItemId::Function(func_loc) = item {
                let signature = baml_compiler_hir::function_signature(db, *func_loc);
                let span = Span::default(); // TODO: get proper span from signature

                let param_types: Vec<Ty> = signature
                    .params
                    .iter()
                    .map(|p| resolution_ctx.lower_type_ref(&p.type_ref, span).0)
                    .collect();

                let return_type = resolution_ctx
                    .lower_type_ref(&signature.return_type, span)
                    .0;

                let func_type = Ty::Function {
                    params: param_types,
                    ret: Box::new(return_type),
                };

                context.insert(signature.name.clone(), func_type);
            }
        }
    }

    TypingContextMap::new(db, context /* functions */)
}

/// Query: Get class field types for a project.
///
/// Maps class names to their field types.
#[salsa::tracked]
pub fn class_field_types(db: &dyn Db, project: Project) -> ClassFieldTypesMap<'_> {
    let hir_fields = baml_compiler_hir::project_class_fields(db, project);
    let resolution_ctx = TypeResolutionContext::new(db, project);
    let span = Span::default(); // TODO: get proper span from fields

    let classes = hir_fields
        .classes(db)
        .iter()
        .map(|(class_name, fields)| {
            let lowered_fields = fields
                .iter()
                .map(|(field_name, type_ref)| {
                    (
                        field_name.clone(),
                        resolution_ctx.lower_type_ref(type_ref, span).0,
                    )
                })
                .collect();
            (class_name.clone(), lowered_fields)
        })
        .collect();

    ClassFieldTypesMap::new(db, classes)
}

/// Query: Get type alias definitions for a project.
///
/// Maps type alias names to their resolved types.
#[salsa::tracked]
pub fn type_aliases(db: &dyn Db, project: Project) -> TypeAliasesMap<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let resolution_ctx = TypeResolutionContext::new(db, project);
    let span = Span::default(); // TODO: get proper span from alias
    let mut aliases = HashMap::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::TypeAlias(alias_loc) = item {
            let file = alias_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let alias_data = &item_tree[alias_loc.id(db)];

            let lowered_ty = resolution_ctx.lower_type_ref(&alias_data.type_ref, span).0;
            aliases.insert(alias_data.name.clone(), lowered_ty);
        }
    }

    TypeAliasesMap::new(db, aliases)
}

/// Query: Get class names for a project.
#[salsa::tracked]
pub fn class_names(db: &dyn Db, project: Project) -> ClassNamesSet<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut names = HashSet::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Class(class_loc) = item {
            let file = class_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let class_data = &item_tree[class_loc.id(db)];
            names.insert(class_data.name.clone());
        }
    }

    ClassNamesSet::new(db, names)
}

/// Query: Get enum names for a project.
#[salsa::tracked]
pub fn enum_names(db: &dyn Db, project: Project) -> EnumNamesSet<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut names = HashSet::new();

    for item in items.items(db) {
        if let baml_compiler_hir::ItemId::Enum(enum_loc) = item {
            let file = enum_loc.file(db);
            let item_tree = baml_compiler_hir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];
            names.insert(enum_data.name.clone());
        }
    }

    EnumNamesSet::new(db, names)
}

/// Query: Get all known type names for a project (classes, enums, type aliases).
#[salsa::tracked]
pub fn known_types(db: &dyn Db, project: Project) -> KnownTypesSet<'_> {
    let items = baml_compiler_hir::project_items(db, project);
    let mut names = HashSet::new();

    for item in items.items(db) {
        match item {
            baml_compiler_hir::ItemId::Class(class_loc) => {
                let file = class_loc.file(db);
                let item_tree = baml_compiler_hir::file_item_tree(db, file);
                let class_data = &item_tree[class_loc.id(db)];
                names.insert(class_data.name.clone());
            }
            baml_compiler_hir::ItemId::Enum(enum_loc) => {
                let file = enum_loc.file(db);
                let item_tree = baml_compiler_hir::file_item_tree(db, file);
                let enum_data = &item_tree[enum_loc.id(db)];
                names.insert(enum_data.name.clone());
            }
            baml_compiler_hir::ItemId::TypeAlias(alias_loc) => {
                let file = alias_loc.file(db);
                let item_tree = baml_compiler_hir::file_item_tree(db, file);
                let alias_data = &item_tree[alias_loc.id(db)];
                names.insert(alias_data.name.clone());
            }
            _ => {}
        }
    }

    KnownTypesSet::new(db, names)
}

/// Context for type resolution across a project.
///
/// This bundles together all the sets needed for resolved type lowering.
/// Create this once per project and reuse it for all type lowering operations.
pub struct TypeResolutionContext {
    pub class_names: HashSet<Name>,
    pub enum_names: HashSet<Name>,
    pub known_types: HashSet<Name>,
}

impl TypeResolutionContext {
    /// Create a new type resolution context for a project.
    pub fn new(db: &dyn Db, project: Project) -> Self {
        Self {
            class_names: class_names(db, project).names(db).clone(),
            enum_names: enum_names(db, project).names(db).clone(),
            known_types: known_types(db, project).names(db).clone(),
        }
    }

    /// Lower a type reference with full resolution.
    pub fn lower_type_ref(
        &self,
        type_ref: &baml_compiler_hir::TypeRef,
        span: Span,
    ) -> (Ty, Vec<TirTypeError>) {
        lower_type_ref_validated_resolved(
            type_ref,
            &self.known_types,
            &self.class_names,
            &self.enum_names,
            span,
        )
    }
}

// ============================================================================
// Type Inference Results
// ============================================================================

/// Result of type inference for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResult {
    /// Inferred return type of the function.
    pub return_type: Ty,
    /// Types of parameters.
    pub param_types: HashMap<Name, Ty>,
    /// Types inferred for each expression.
    pub expr_types: HashMap<ExprId, Ty>,
    /// For multi-segment path expressions, the type of each segment.
    /// For `o.inner.value` where `o: Outer`, stores `[Outer, Inner, int]`.
    /// Used by codegen to look up field indices at each step.
    pub path_segment_types: HashMap<ExprId, Vec<Ty>>,
    /// Expressions that are enum variant values (e.g., `Status.Active`).
    /// Maps expression ID to (`enum_name`, `variant_name`).
    /// Used by codegen to emit enum variant construction.
    pub enum_variant_exprs: HashMap<ExprId, (Name, Name)>,
    /// Match expressions that are exhaustive (all cases covered).
    /// Used by codegen to emit `unreachable` for fallthrough paths,
    /// enabling phi-like optimization for match results.
    pub exhaustive_matches: HashSet<ExprId>,
    /// Type checking errors.
    pub errors: Vec<TirTypeError>,
    /// Resolution information for IDE features (go-to-definition, find-references).
    /// Maps expression IDs to what they resolve to.
    pub expr_resolutions: ResolutionMap,
}

// ============================================================================
// Type Context
// ============================================================================

/// Where a local variable was defined (for go-to-definition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionSite {
    /// Defined in a let statement.
    Statement(StmtId),
    /// Defined as a function parameter (with its index).
    Parameter(usize),
}

/// Context for type inference, tracking scopes and accumulated results.
pub struct TypeContext<'db> {
    db: &'db dyn Db,
    /// Stack of variable scopes (innermost last).
    scopes: Vec<HashMap<Name, Ty>>,
    /// Class field types: `class_name` -> (`field_name` -> `field_type`)
    class_fields: HashMap<Name, HashMap<Name, Ty>>,
    /// Type alias definitions: `alias_name` -> `resolved_type`
    type_aliases: HashMap<Name, Ty>,
    /// Enum variant definitions: `enum_name` -> `Vec<variant_name>`
    enum_variants: HashMap<Name, Vec<Name>>,
    /// Class names for type resolution
    class_names: HashSet<Name>,
    /// Enum names for type resolution
    enum_names: HashSet<Name>,
    /// Known type names for validation
    known_types: HashSet<Name>,
    /// Inferred types for expressions.
    expr_types: HashMap<ExprId, Ty>,
    /// For multi-segment paths, the type of each segment.
    path_segment_types: HashMap<ExprId, Vec<Ty>>,
    /// Expressions that are enum variant values.
    enum_variant_exprs: HashMap<ExprId, (Name, Name)>,
    /// Match expressions that are exhaustive (all cases covered).
    exhaustive_matches: HashSet<ExprId>,
    /// Types of all return statements encountered during inference.
    /// Used to validate that all return paths match the declared return type.
    return_types: Vec<(Ty, Span)>,
    /// Accumulated type errors.
    errors: Vec<TirTypeError>,
    /// The current file being typechecked
    file_id: FileId,
    /// Variables declared with `watch let` (tracked for $watch validation).
    watched_vars: HashSet<Name>,
    /// Resolution map for expressions (for IDE features).
    expr_resolutions: ResolutionMap,
    /// Track where local variables were defined (for go-to-definition).
    local_definitions: HashMap<Name, DefinitionSite>,
}

impl<'db> TypeContext<'db> {
    /// Create a new type context with full type resolution info.
    #[allow(clippy::too_many_arguments)]
    pub fn with_type_info(
        db: &'db dyn Db,
        globals: HashMap<Name, Ty>,
        class_fields: HashMap<Name, HashMap<Name, Ty>>,
        type_aliases: HashMap<Name, Ty>,
        enum_variants: HashMap<Name, Vec<Name>>,
        class_names: HashSet<Name>,
        enum_names: HashSet<Name>,
        known_types: HashSet<Name>,
        file_id: FileId,
    ) -> Self {
        TypeContext {
            db,
            scopes: vec![globals],
            class_fields,
            type_aliases,
            enum_variants,
            class_names,
            enum_names,
            known_types,
            expr_types: HashMap::new(),
            path_segment_types: HashMap::new(),
            enum_variant_exprs: HashMap::new(),
            exhaustive_matches: HashSet::new(),
            return_types: Vec::new(),
            errors: Vec::new(),
            file_id,
            watched_vars: HashSet::new(),
            expr_resolutions: HashMap::new(),
            local_definitions: HashMap::new(),
        }
    }

    /// Record a return type encountered during inference.
    pub fn record_return(&mut self, ty: Ty, span: Span) {
        self.return_types.push((ty, span));
    }

    /// Look up a type alias definition.
    pub fn lookup_type_alias(&self, name: &Name) -> Option<&Ty> {
        self.type_aliases.get(name)
    }

    /// Look up enum variants.
    pub fn lookup_enum_variants(&self, name: &Name) -> Option<&Vec<Name>> {
        self.enum_variants.get(name)
    }

    /// Look up a field in a class.
    pub fn lookup_class_field(&self, class_name: &Name, field_name: &Name) -> Option<&Ty> {
        self.class_fields
            .get(class_name)
            .and_then(|fields| fields.get(field_name))
    }

    /// Push a new scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a variable in the current scope.
    pub fn define(&mut self, name: Name, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable in the scope chain.
    pub fn lookup(&self, name: &Name) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Record the type of an expression.
    pub fn set_expr_type(&mut self, expr: ExprId, ty: Ty) {
        self.expr_types.insert(expr, ty);
    }

    /// Add a type error.
    pub fn push_error(&mut self, error: TirTypeError) {
        self.errors.push(error);
    }

    /// Mark a variable as watched (declared with `watch let`).
    pub fn mark_watched(&mut self, name: Name) {
        self.watched_vars.insert(name);
    }

    /// Check if a variable is watched (declared with `watch let`).
    pub fn is_watched(&self, name: &Name) -> bool {
        self.watched_vars.contains(name)
    }

    /// Set the resolution for an expression.
    pub fn set_expr_resolution(&mut self, expr_id: ExprId, resolution: ResolvedValue) {
        self.expr_resolutions.insert(expr_id, resolution);
    }

    /// Define a local variable and track its definition site.
    pub fn define_with_site(&mut self, name: Name, ty: Ty, definition_site: DefinitionSite) {
        // Get the current scope (last in the stack)
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.clone(), ty);
        }
        self.local_definitions.insert(name, definition_site);
    }

    /// Get the definition site for a local variable.
    pub fn get_definition_site(&self, name: &Name) -> Option<DefinitionSite> {
        self.local_definitions.get(name).copied()
    }

    /// Get the database reference.
    pub fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub fn build_span(&self, range: TextRange) -> Span {
        Span::new(self.file_id, range)
    }

    pub fn build_span_default(&self, range: &Option<TextRange>) -> Span {
        // todo: probably this should be an error? it should be an invariant that
        // all exprs have valid spans
        range.map(|s| self.build_span(s)).unwrap_or_default()
    }

    /// Check if `sub` is a subtype of `sup`, resolving type aliases.
    pub fn is_subtype_of(&self, sub: &Ty, sup: &Ty) -> bool {
        normalize::is_subtype_of(sub, sup, &self.type_aliases)
    }

    /// Lower a `TypeRef` to a Ty with full resolution (classes/enums resolved to names).
    pub fn lower_type_resolved(&self, type_ref: &baml_compiler_hir::TypeRef, span: Span) -> Ty {
        let (ty, _errors) = lower_type_ref_validated_resolved(
            type_ref,
            &self.known_types,
            &self.class_names,
            &self.enum_names,
            span,
        );
        // Note: errors are not accumulated here since they should have been
        // caught during earlier validation passes
        ty
    }

    /// Resolve a named type to its proper Ty representation.
    ///
    /// This resolves class and enum names to `Ty::Class` and `Ty::Enum` with FQNs,
    /// while type aliases and unknown types stay as `Ty::TypeAlias`.
    pub fn resolve_named_type(&self, name: &Name) -> Ty {
        use baml_compiler_hir::FullyQualifiedName;
        if self.class_names.contains(name) {
            Ty::Class(FullyQualifiedName::local(name.clone()))
        } else if self.enum_names.contains(name) {
            Ty::Enum(FullyQualifiedName::local(name.clone()))
        } else {
            // Type alias or unknown type - stays as TypeAlias, will be resolved during normalization
            Ty::TypeAlias(FullyQualifiedName::local(name.clone()))
        }
    }

    /// Resolve a path to determine what it refers to.
    ///
    /// This is the core path resolution logic that determines whether a path like
    /// `user.name` is a local variable with field access, an enum variant, a module
    /// item, etc.
    ///
    /// # Resolution Order
    /// 1. Check if the first segment is a local variable -> Local with field accesses
    /// 2. Check if it's a function name -> Function
    /// 3. Check if first segment is a class name (for enum variants)
    /// 4. Check if it's a module path (TODO: not yet implemented)
    /// 5. Unknown
    pub fn resolve_path(&self, segments: &[Name]) -> ResolvedPath {
        if segments.is_empty() {
            return ResolvedPath::Unknown;
        }

        let first = &segments[0];

        // Check if first segment is a local variable
        if self.lookup(first).is_some() {
            return ResolvedPath::Local {
                name: first.clone(),
            };
        }

        // For single-segment paths, check if it's a function
        if segments.len() == 1 {
            // Check globals (which include functions)
            if self.scopes.first().and_then(|s| s.get(first)).is_some() {
                return ResolvedPath::Function {
                    name: first.clone(),
                };
            }
        }

        // For two-segment paths, check if it could be an enum variant
        // TODO: This needs access to the type registry to check if `first` is an enum
        if segments.len() == 2 {
            // For now, we can't distinguish enum variants without more context
            // This will be populated when we have the Module infrastructure
        }

        // TODO: Check module paths when Module type is fully integrated

        // Unknown path
        ResolvedPath::Unknown
    }
}

// ============================================================================
// Type Inference
// ============================================================================

/// Infer types for a function body.
///
/// This is the main entry point for type inference. It takes a pre-lowered
/// function body and infers types for all expressions.
///
/// The `globals` parameter provides types for top-level functions, allowing
/// function calls to be properly typed. Pass `None` if no global context is needed.
///
/// Note: In a full implementation, this would be a Salsa tracked function.
/// For now, it's a regular function that takes the body directly.
#[allow(clippy::too_many_arguments)]
pub fn infer_function_body<'db>(
    db: &'db dyn Db,
    body: &FunctionBody,
    param_types: HashMap<Name, Ty>,
    expected_return: &Ty,
    return_type_span: Option<Span>,
    globals: Option<HashMap<Name, Ty>>,
    class_fields: Option<HashMap<Name, HashMap<Name, Ty>>>,
    type_aliases: Option<HashMap<Name, Ty>>,
    enum_variants: Option<HashMap<Name, Vec<Name>>>,
    class_names_opt: Option<HashSet<Name>>,
    enum_names_opt: Option<HashSet<Name>>,
    known_types: Option<HashSet<Name>>,
    function_loc: FunctionLoc<'db>,
) -> InferenceResult {
    let file_id = function_loc.file(db).file_id(db);
    let mut ctx = TypeContext::with_type_info(
        db,
        globals.unwrap_or_default(),
        class_fields.unwrap_or_default(),
        type_aliases.unwrap_or_default(),
        enum_variants.unwrap_or_default(),
        class_names_opt.unwrap_or_default(),
        enum_names_opt.unwrap_or_default(),
        known_types.unwrap_or_default(),
        file_id,
    );

    // Add parameters to the current scope (on top of globals)
    // Track their index in the parameter list for go-to-definition
    for (index, (name, ty)) in param_types.iter().enumerate() {
        ctx.define_with_site(name.clone(), ty.clone(), DefinitionSite::Parameter(index));
    }

    // Type check the body against the expected return type (checking mode for bidirectional typing)
    let (trailing_expr_type, body_location) = match body {
        FunctionBody::Expr(expr_body, _source_map) => {
            if let Some(root_expr) = expr_body.root_expr {
                // Use check_expr for bidirectional typing - check body against expected return type
                let ty = check_expr(&mut ctx, root_expr, expr_body, expected_return);
                (ty, ErrorLocation::Expr(root_expr))
            } else {
                (Ty::Void, ErrorLocation::Span(Span::default()))
            }
        }
        FunctionBody::Llm(_) => {
            // LLM functions return their declared return type
            (
                expected_return.clone(),
                ErrorLocation::Span(Span::default()),
            )
        }
        FunctionBody::Missing => (Ty::Unknown, ErrorLocation::Span(Span::default())),
    };

    // With bidirectional type checking, return statements are already checked
    // via check_stmt_with_return, so we don't need to check them again here.

    // With bidirectional type checking, check_expr already reported any mismatches
    // between the body and expected return type. We only need to check one case:
    // If there are no returns and no tail expression, and we expected a non-void type
    if ctx.return_types.is_empty()
        && trailing_expr_type.is_void()
        && !expected_return.is_void()
        && !expected_return.is_unknown()
        && !expected_return.is_error()
    {
        // If the trailing type is void (no tail expression) but we need a non-void return,
        // emit a clearer "missing return expression" error
        let error = if trailing_expr_type.is_void() && !expected_return.is_void() {
            TypeError::MissingReturnExpression {
                expected: expected_return.clone(),
                location: body_location,
            }
        } else {
            TypeError::TypeMismatch {
                expected: expected_return.clone(),
                found: trailing_expr_type.clone(),
                location: body_location,
                info_location: return_type_span.map(ErrorLocation::Span),
            }
        };
        ctx.push_error(error);
    }

    // Determine the inferred return type:
    // - If there are explicit return statements, use the expected type (we already validated them)
    // - If there's a trailing expression (not Void), use its type
    // - Otherwise, use Void
    let return_type = if !ctx.return_types.is_empty() {
        // If there are return statements, the function returns what they return
        // (we've already checked they match expected_return)
        expected_return.clone()
    } else if !trailing_expr_type.is_void() {
        trailing_expr_type
    } else {
        Ty::Void
    };

    InferenceResult {
        return_type,
        param_types,
        expr_types: ctx.expr_types,
        path_segment_types: ctx.path_segment_types,
        enum_variant_exprs: ctx.enum_variant_exprs,
        exhaustive_matches: ctx.exhaustive_matches,
        errors: ctx.errors,
        expr_resolutions: ctx.expr_resolutions,
    }
}

/// Salsa tracked query for function type inference.
///
/// This caches the type inference results for a function, enabling
/// incremental recomputation when dependencies change.
#[salsa::tracked]
pub fn function_type_inference<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> Arc<InferenceResult> {
    // Get the function signature and body
    // NOTE: We intentionally don't call function_signature_source_map here.
    // This allows Salsa early cutoff: when only whitespace/comments change,
    // function_signature returns an equal value, so this query is cached.
    // The trade-off is that type mismatch errors won't point to the return
    // type annotation, but they'll still point to the offending expression.
    let signature = baml_compiler_hir::function_signature(db, function);
    let body = baml_compiler_hir::function_body(db, function);

    // Get the project context
    let project = db.project();

    // Build global context from the project
    // Get function signatures as global types (for function calls)
    let typing_ctx = typing_context(db, project);
    let globals = Some(typing_ctx.functions(db).clone());

    // Get class field types
    let class_field_types = class_field_types(db, project);
    let class_fields = Some(class_field_types.classes(db).clone());

    // Get type aliases
    let type_aliases_map = type_aliases(db, project);
    let type_aliases = Some(type_aliases_map.aliases(db).clone());

    // Get enum variants
    let enum_variants_map = enum_variants(db, project);
    let enum_variants = Some(enum_variants_map.enums(db).clone());

    let result = infer_function(
        db,
        &signature,
        None, // No source map - enables Salsa early cutoff on whitespace changes
        &body,
        globals,
        class_fields,
        type_aliases,
        enum_variants,
        function,
    );

    Arc::new(result)
}

/// Infer types for a function given its signature and body.
///
/// This queries the database for known type names and validates that all type
/// references in the signature refer to types that exist. Unknown types will
/// produce errors and be replaced with `Ty::Error` to suppress downstream
/// type mismatches.
///
/// The `globals` parameter provides types for top-level functions, allowing
/// function calls to be properly typed. Pass `None` if no global context is needed.
///
/// The `sig_source_map` parameter is optional. When provided, type mismatch errors
/// will include a secondary location pointing to the return type annotation.
/// When `None`, errors still point to the offending expression but without the
/// return type annotation location. Pass `None` for cached queries to enable
/// Salsa early cutoff on whitespace/comment changes.
#[allow(clippy::too_many_arguments)]
pub fn infer_function<'db>(
    db: &'db dyn Db,
    signature: &FunctionSignature,
    sig_source_map: Option<&SignatureSourceMap>,
    body: &FunctionBody,
    globals: Option<HashMap<Name, Ty>>,
    class_fields: Option<HashMap<Name, HashMap<Name, Ty>>>,
    type_aliases: Option<HashMap<Name, Ty>>,
    enum_variants: Option<HashMap<Name, Vec<Name>>>,
    function_loc: FunctionLoc<'db>,
) -> InferenceResult {
    // Query known type names from the project (Salsa-cached)
    let project = db.project();
    let known_type_names = baml_compiler_hir::project_type_names(db, project);
    let known_types: std::collections::HashSet<_> =
        known_type_names.names(db).iter().cloned().collect();

    // Get class and enum name sets for type resolution (Salsa-cached)
    let class_name_set = class_names(db, project).names(db).clone();
    let enum_name_set = enum_names(db, project).names(db).clone();

    let file_id = function_loc.file(db).file_id(db);
    // Use a placeholder span for now - ideally we'd have spans on TypeRef
    let placeholder_span = Span::new(file_id, TextRange::empty(0.into()));

    let mut type_errors: Vec<TirTypeError> = Vec::new();

    // Convert parameter TypeRefs to Tys with validation and resolution
    let param_types: HashMap<Name, Ty> = signature
        .params
        .iter()
        .map(|param| {
            let (ty, errors) = lower_type_ref_validated_resolved(
                &param.type_ref,
                &known_types,
                &class_name_set,
                &enum_name_set,
                placeholder_span,
            );
            type_errors.extend(errors);
            (param.name.clone(), ty)
        })
        .collect();

    // Convert return type with validation and resolution
    let (expected_return, errors) = lower_type_ref_validated_resolved(
        &signature.return_type,
        &known_types,
        &class_name_set,
        &enum_name_set,
        placeholder_span,
    );
    type_errors.extend(errors);

    // Convert return type TextRange to Span for diagnostics (if source map provided)
    let return_type_span = sig_source_map
        .and_then(SignatureSourceMap::return_type_span)
        .map(|range| Span::new(file_id, range));

    // Delegate to the body inference function
    let mut result = infer_function_body(
        db,
        body,
        param_types,
        &expected_return,
        return_type_span,
        globals,
        class_fields,
        type_aliases,
        enum_variants,
        Some(class_name_set),
        Some(enum_name_set),
        Some(known_types),
        function_loc,
    );

    // Prepend type lowering errors to the result
    // (they should appear before type checking errors)
    type_errors.extend(result.errors);
    result.errors = type_errors;

    result
}

/// Infer the type of an expression (synthesize mode).
fn infer_expr(ctx: &mut TypeContext<'_>, expr_id: ExprId, body: &ExprBody) -> Ty {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[expr_id];

    // Use position-independent location for errors - resolved to spans at render time
    let location = ErrorLocation::Expr(expr_id);

    let ty = match expr {
        Expr::Literal(lit) => infer_literal(lit),

        Expr::Path(segments) => {
            if segments.is_empty() {
                Ty::Unknown
            } else if segments.len() == 1 {
                // Single segment: variable, function, class, or enum lookup
                let name = &segments[0];
                if let Some(ty) = ctx.lookup(name) {
                    let ty = ty.clone();

                    // Determine the resolution based on what kind of entity this is
                    let resolution = if let Some(definition_site) = ctx.get_definition_site(name) {
                        // Has a definition site -> it's a local variable or parameter
                        ResolvedValue::Local {
                            name: name.clone(),
                            definition_site: Some(definition_site),
                        }
                    } else if ctx.class_names.contains(name) {
                        // Class name
                        use baml_compiler_hir::FullyQualifiedName;
                        ResolvedValue::Class(FullyQualifiedName::local(name.clone()))
                    } else if ctx.enum_names.contains(name) {
                        // Enum name
                        use baml_compiler_hir::FullyQualifiedName;
                        ResolvedValue::Enum(FullyQualifiedName::local(name.clone()))
                    } else if ctx.type_aliases.contains_key(name) {
                        // Type alias
                        use baml_compiler_hir::FullyQualifiedName;
                        ResolvedValue::TypeAlias(FullyQualifiedName::local(name.clone()))
                    } else {
                        // Must be a function in globals
                        use baml_compiler_hir::FullyQualifiedName;
                        ResolvedValue::Function(FullyQualifiedName::local(name.clone()))
                    };

                    // Store resolution for IDE features
                    ctx.set_expr_resolution(expr_id, resolution);
                    ty
                } else {
                    ctx.push_error(TypeError::UnknownVariable {
                        name: name.to_string(),
                        location,
                    });
                    Ty::Unknown
                }
            } else {
                // Multi-segment path: could be:
                // 1. A builtin function (e.g., baml.Array.length)
                // 2. A variable followed by field accesses (e.g., obj.field)

                // First, check if this is a builtin function path
                let full_path = segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                if let Some(def) = builtins::lookup_builtin_by_path(&full_path) {
                    // Store resolution for builtin function
                    ctx.set_expr_resolution(
                        expr_id,
                        ResolvedValue::BuiltinFunction {
                            // Use normalized path (baaml -> baml)
                            path: def.path.to_string(),
                        },
                    );

                    // It's a builtin function - return its function type
                    let mut param_types: Vec<Ty> = Vec::new();
                    if let Some(ref receiver_pattern) = def.receiver {
                        param_types.push(builtins::substitute_unknown(receiver_pattern));
                    }
                    for (_, pattern) in &def.params {
                        param_types.push(builtins::substitute_unknown(pattern));
                    }
                    let return_type = builtins::substitute_unknown(&def.returns);
                    return Ty::Function {
                        params: param_types,
                        ret: Box::new(return_type),
                    };
                }

                // Check if this is an enum variant (e.g., Status.Active)
                if segments.len() == 2 {
                    let enum_name = &segments[0];
                    let variant_name = &segments[1];

                    if let Some(variants) = ctx.lookup_enum_variants(enum_name) {
                        if variants.contains(variant_name) {
                            use baml_compiler_hir::FullyQualifiedName;
                            let enum_fqn = FullyQualifiedName::local(enum_name.clone());

                            // Store resolution for enum variant
                            ctx.set_expr_resolution(
                                expr_id,
                                ResolvedValue::EnumVariant {
                                    enum_fqn: enum_fqn.clone(),
                                    variant: variant_name.clone(),
                                },
                            );

                            // This is a valid enum variant - record it and return the enum type
                            ctx.enum_variant_exprs
                                .insert(expr_id, (enum_name.clone(), variant_name.clone()));
                            return Ty::Enum(enum_fqn);
                        }
                        // Enum exists but variant doesn't
                        ctx.push_error(TypeError::UnknownEnumVariant {
                            enum_name: enum_name.to_string(),
                            variant_name: variant_name.to_string(),
                            location,
                        });
                        return Ty::Unknown;
                    }
                }

                // Otherwise, treat as variable + field accesses
                let first = &segments[0];
                let mut ty = if let Some(t) = ctx.lookup(first) {
                    t.clone()
                } else {
                    ctx.push_error(TypeError::UnknownVariable {
                        name: first.to_string(),
                        location,
                    });
                    return Ty::Unknown;
                };

                // Record segment types for codegen (first segment type, then each field access result)
                let mut segment_types = vec![ty.clone()];

                // Apply field accesses for remaining segments
                for field in &segments[1..] {
                    // Before updating ty, check if we're accessing a field on a class
                    // This is for IDE resolution of field access paths
                    if let Ty::Class(class_fqn) = &ty {
                        ctx.set_expr_resolution(
                            expr_id,
                            ResolvedValue::Field {
                                class_fqn: class_fqn.clone(),
                                field: field.clone(),
                            },
                        );
                    }
                    ty = infer_field_access(ctx, &ty, field, location);
                    segment_types.push(ty.clone());
                }

                // Store segment types for this path expression
                ctx.path_segment_types.insert(expr_id, segment_types);

                ty
            }
        }

        Expr::Binary { lhs, op, rhs } => {
            // Special case: instanceof operator - RHS is a type reference, not an expression
            if *op == baml_compiler_hir::BinaryOp::Instanceof {
                let _lhs_ty = infer_expr(ctx, *lhs, body);
                // For instanceof, don't try to resolve RHS as a variable.
                // The RHS is a type name and will be resolved at runtime.
                // Just return bool since instanceof always returns a boolean.
                Ty::Bool
            } else {
                let lhs_ty = infer_expr(ctx, *lhs, body);
                let rhs_ty = infer_expr(ctx, *rhs, body);
                infer_binary_op(ctx, *op, &lhs_ty, &rhs_ty, location)
            }
        }

        Expr::Unary { op, expr: inner } => {
            let inner_ty = infer_expr(ctx, *inner, body);
            infer_unary_op(ctx, *op, &inner_ty, location)
        }

        Expr::Call { callee, args } => {
            // Check if this is a method call (callee is a FieldAccess or multi-segment Path)
            // If so, we need to pass the receiver as the first argument.
            // We track (type, Option<span>) for each argument so we can report errors
            // at the correct location. Implicit receiver args have None for span.
            let (callee_ty, effective_args): (Ty, Vec<(Ty, Option<ErrorLocation>)>) = match &body
                .exprs[*callee]
            {
                Expr::FieldAccess { base, field: _ } => {
                    // Method call: receiver.method(args) -> Type.method(receiver, args)
                    // This handles complex expressions like `f().method()` or `arr[0].method()`
                    let receiver_ty = infer_expr(ctx, *base, body);
                    let callee_ty = infer_expr(ctx, *callee, body);

                    // Build effective args: [(receiver_type, None), ...explicit_args with spans]
                    let mut effective_args = vec![(receiver_ty, None)];
                    for arg in args {
                        let arg_ty = infer_expr(ctx, *arg, body);
                        let arg_location = Some(ErrorLocation::Expr(*arg));
                        effective_args.push((arg_ty, arg_location));
                    }
                    (callee_ty, effective_args)
                }
                Expr::Path(segments) if segments.len() >= 2 => {
                    // First, check if this is a direct builtin function call
                    // (e.g., baml.Array.length(arr), baml.deep_copy(x))
                    let full_path = segments
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    if let Some(def) = builtins::lookup_builtin_by_path(&full_path) {
                        // It's a builtin function - infer argument types first so we can
                        // bind type variables (e.g., T in deep_copy(x: T) -> T)
                        let arg_types_with_spans: Vec<(Ty, Option<ErrorLocation>)> = args
                            .iter()
                            .map(|arg| {
                                let ty = infer_expr(ctx, *arg, body);
                                let arg_location = Some(ErrorLocation::Expr(*arg));
                                (ty, arg_location)
                            })
                            .collect();
                        let arg_types: Vec<Ty> = arg_types_with_spans
                            .iter()
                            .map(|(ty, _)| ty.clone())
                            .collect();

                        // Build parameter patterns and match against argument types to
                        // extract type variable bindings
                        let mut param_patterns: Vec<&baml_builtins::TypePattern> = Vec::new();
                        if let Some(ref receiver_pattern) = def.receiver {
                            param_patterns.push(receiver_pattern);
                        }
                        for (_, pattern) in &def.params {
                            param_patterns.push(pattern);
                        }

                        // Try to match each argument against its parameter pattern
                        let mut bindings = builtins::Bindings::new();
                        for (arg_ty, param_pattern) in arg_types.iter().zip(param_patterns.iter()) {
                            if let Some(new_bindings) =
                                builtins::match_pattern(param_pattern, arg_ty)
                            {
                                // Merge bindings (first binding wins for consistency)
                                for (name, ty) in new_bindings {
                                    bindings.entry(name).or_insert(ty);
                                }
                            }
                        }

                        // Build function type using bindings for type variables
                        let param_types: Vec<Ty> = param_patterns
                            .iter()
                            .map(|p| {
                                if bindings.is_empty() {
                                    builtins::substitute_unknown(p)
                                } else {
                                    substitute_with_fallback(p, &bindings)
                                }
                            })
                            .collect();

                        let return_type = if bindings.is_empty() {
                            builtins::substitute_unknown(&def.returns)
                        } else {
                            substitute_with_fallback(&def.returns, &bindings)
                        };

                        let callee_ty = Ty::Function {
                            params: param_types,
                            ret: Box::new(return_type),
                        };
                        (callee_ty, arg_types_with_spans)
                    } else {
                        // Method call via Path: `receiver.method(args)`
                        // For multi-segment paths like `baz.Greeting()`, the first segment(s)
                        // form the receiver and the last segment is the method name.
                        //
                        // We infer the receiver type from all segments except the last,
                        // then look up the method on that type.
                        let receiver_segments = &segments[..segments.len() - 1];

                        // Infer receiver type (could be single var or nested field access)
                        let receiver_ty = if receiver_segments.len() == 1 {
                            // Simple receiver: `baz.method()`
                            ctx.lookup(&receiver_segments[0])
                                .cloned()
                                .unwrap_or(Ty::Unknown)
                        } else {
                            // Nested receiver: `obj.field.method()`
                            let first = &receiver_segments[0];
                            let mut ty = ctx.lookup(first).cloned().unwrap_or(Ty::Unknown);
                            for field in &receiver_segments[1..] {
                                ty = infer_field_access(ctx, &ty, field, location);
                            }
                            ty
                        };

                        let callee_ty = infer_expr(ctx, *callee, body);

                        // Build effective args: [(receiver_type, None), ...explicit_args with spans]
                        let mut effective_args = vec![(receiver_ty, None)];
                        for arg in args {
                            let arg_ty = infer_expr(ctx, *arg, body);
                            let arg_location = Some(ErrorLocation::Expr(*arg));
                            effective_args.push((arg_ty, arg_location));
                        }
                        (callee_ty, effective_args)
                    }
                }
                _ => {
                    // Regular function call (single-segment Path or other expression)
                    let callee_ty = infer_expr(ctx, *callee, body);
                    let arg_types_with_spans: Vec<(Ty, Option<ErrorLocation>)> = args
                        .iter()
                        .map(|arg| {
                            let ty = infer_expr(ctx, *arg, body);
                            let arg_location = Some(ErrorLocation::Expr(*arg));
                            (ty, arg_location)
                        })
                        .collect();
                    (callee_ty, arg_types_with_spans)
                }
            };

            // If the callee is a function type, check arguments and return the return type
            match &callee_ty {
                Ty::Function { params, ret } => {
                    // Check argument count
                    if effective_args.len() != params.len() {
                        ctx.push_error(TypeError::ArgumentCountMismatch {
                            expected: params.len(),
                            found: effective_args.len(),
                            location,
                        });
                    }

                    // Check argument types - use each argument's location for precise error location
                    for ((arg_ty, arg_location), param_ty) in
                        effective_args.iter().zip(params.iter())
                    {
                        if !ctx.is_subtype_of(arg_ty, param_ty) {
                            // Use the argument's location if available, otherwise fall back to call location
                            let error_location = arg_location.unwrap_or(location);
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: param_ty.clone(),
                                found: generalize_for_error(param_ty, arg_ty),
                                location: error_location,
                                info_location: None,
                            });
                        }
                    }

                    // Return the function's return type
                    (**ret).clone()
                }
                Ty::Unknown => Ty::Unknown,
                _ => {
                    ctx.push_error(TypeError::NotCallable {
                        ty: callee_ty,
                        location,
                    });
                    Ty::Unknown
                }
            }
        }

        Expr::FieldAccess { base, field } => {
            // Special validation for $watch accessor
            if field.as_str() == "$watch" {
                // $watch can only be used on simple variable expressions
                let base_expr = &body.exprs[*base];
                match base_expr {
                    Expr::Path(segments) if segments.len() == 1 => {
                        // Simple variable - check if it's declared as watched
                        let var_name = &segments[0];
                        if !ctx.is_watched(var_name) {
                            ctx.push_error(TypeError::WatchOnUnwatchedVariable {
                                name: var_name.to_string(),
                                location,
                            });
                        }
                    }
                    _ => {
                        // Not a simple variable (e.g., arr[0].$watch, obj.field.$watch)
                        ctx.push_error(TypeError::WatchOnNonVariable { location });
                    }
                }
            }

            let base_ty = infer_expr(ctx, *base, body);
            infer_field_access(ctx, &base_ty, field, location)
        }

        Expr::Index { base, index } => {
            let base_ty = infer_expr(ctx, *base, body);
            let index_ty = infer_expr(ctx, *index, body);
            infer_index_access(ctx, &base_ty, &index_ty, location)
        }

        Expr::Array { elements } => {
            if elements.is_empty() {
                Ty::List(Box::new(Ty::Unknown))
            } else {
                // Infer element type from first element, but generalize literals to base types
                // This ensures [1, 2, 3] is int[] not "1"[]
                let first_ty = infer_expr(ctx, elements[0], body);
                let elem_ty = generalize(&first_ty);

                // Check all elements have compatible types
                for &elem in &elements[1..] {
                    let other_ty = infer_expr(ctx, elem, body);
                    if !ctx.is_subtype_of(&other_ty, &elem_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: elem_ty.clone(),
                            found: other_ty,
                            location,
                            info_location: None,
                        });
                    }
                }
                Ty::List(Box::new(elem_ty))
            }
        }

        Expr::Object {
            type_name,
            fields,
            spreads,
        } => {
            // Infer field types
            for (_, value_expr) in fields {
                infer_expr(ctx, *value_expr, body);
            }

            // Determine the expected object type
            let obj_ty = if let Some(name) = type_name {
                ctx.resolve_named_type(name)
            } else {
                Ty::Unknown
            };

            // Store resolution for IDE features if this is a class instantiation
            if let Some(name) = type_name {
                if ctx.class_names.contains(name) {
                    use baml_compiler_hir::FullyQualifiedName;
                    ctx.set_expr_resolution(
                        expr_id,
                        ResolvedValue::Class(FullyQualifiedName::local(name.clone())),
                    );
                }
            }

            // Type check spread expressions - they must be the same type as the object
            for spread in spreads {
                let spread_ty = infer_expr(ctx, spread.expr, body);
                // If we have a named type, verify the spread is compatible
                if !matches!(obj_ty, Ty::Unknown) && !ctx.is_subtype_of(&spread_ty, &obj_ty) {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: obj_ty.clone(),
                        found: spread_ty,
                        location,
                        info_location: None,
                    });
                }
            }

            obj_ty
        }

        Expr::Map { entries } => {
            if entries.is_empty() {
                Ty::Map {
                    key: Box::new(Ty::Unknown),
                    value: Box::new(Ty::Unknown),
                }
            } else {
                // Infer key and value types from first entry, but generalize literals to base types
                // This ensures {"x": 1} is map<string, int> not map<"x", 1>
                let first_key_ty = infer_expr(ctx, entries[0].0, body);
                let first_value_ty = infer_expr(ctx, entries[0].1, body);
                let key_ty = generalize(&first_key_ty);
                let value_ty = generalize(&first_value_ty);

                // Check all entries have compatible types
                for &(key, value) in &entries[1..] {
                    let other_key_ty = infer_expr(ctx, key, body);
                    let other_value_ty = infer_expr(ctx, value, body);
                    if !ctx.is_subtype_of(&other_key_ty, &key_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: key_ty.clone(),
                            found: generalize_for_error(&key_ty, &other_key_ty),
                            location,
                            info_location: None,
                        });
                    }
                    if !ctx.is_subtype_of(&other_value_ty, &value_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: value_ty.clone(),
                            found: generalize_for_error(&value_ty, &other_value_ty),
                            location,
                            info_location: None,
                        });
                    }
                }
                Ty::Map {
                    key: Box::new(key_ty),
                    value: Box::new(value_ty),
                }
            }
        }

        Expr::Block { stmts, tail_expr } => {
            ctx.push_scope();

            // Type check statements
            for &stmt_id in stmts {
                check_stmt(ctx, stmt_id, body);
            }

            // Type of block is type of tail expression
            let result = if let Some(tail) = tail_expr {
                infer_expr(ctx, *tail, body)
            } else {
                Ty::Void
            };

            ctx.pop_scope();
            result
        }

        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            // Condition must be bool
            let cond_ty = infer_expr(ctx, *condition, body);
            if !ctx.is_subtype_of(&cond_ty, &Ty::Bool) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Bool,
                    found: cond_ty,
                    location,
                    info_location: None,
                });
            }

            // Check for instanceof narrowing
            let instanceof_narrowing = extract_instanceof_narrowing(ctx, *condition, body);

            // Infer then-branch with narrowed type if applicable
            let then_ty = if let Some((var_name, narrowed_ty)) = &instanceof_narrowing {
                ctx.push_scope();
                ctx.define(var_name.clone(), narrowed_ty.clone());
                let ty = infer_expr(ctx, *then_branch, body);
                ctx.pop_scope();
                ty
            } else {
                infer_expr(ctx, *then_branch, body)
            };

            let else_ty = if let Some(else_expr) = else_branch {
                infer_expr(ctx, *else_expr, body)
            } else {
                Ty::Void
            };

            // Generalize literal types for the result, similar to arrays.
            // This ensures `if (c) { 1 } else { 2 }` is `int` not `1 | 2`.
            let then_ty = generalize(&then_ty);
            let else_ty = generalize(&else_ty);

            // Result is union of branches (simplified)
            if then_ty == else_ty {
                then_ty
            } else if else_branch.is_none() {
                // if without else returns optional
                Ty::Union(vec![then_ty, Ty::Null])
            } else {
                Ty::Union(vec![then_ty, else_ty])
            }
        }

        // Match expressions synthesize a type.
        // TODO: we should support bidirectional type checking
        Expr::Match { scrutinee, arms } => {
            let scrutinee_ty = infer_expr(ctx, *scrutinee, body);

            if arms.is_empty() {
                // Empty match is non-exhaustive (unless scrutinee is uninhabited).
                // An uninhabited type has no possible values, so an empty match is
                // actually exhaustive—there are no cases to handle.
                // See `Ty::is_uninhabited()` for the full definition and rationale.
                if !scrutinee_ty.is_uninhabited() {
                    ctx.push_error(TypeError::NonExhaustiveMatch {
                        scrutinee_type: scrutinee_ty.clone(),
                        missing_cases: vec!["all cases".to_string()],
                        location: ErrorLocation::Expr(expr_id),
                    });
                }
                Ty::Unknown
            } else {
                // Perform exhaustiveness checking and unreachable arm detection
                check_match_exhaustiveness(ctx, &scrutinee_ty, arms, body, expr_id);

                // Collect result types from all arms
                let arm_types: Vec<Ty> = arms
                    .iter()
                    .map(|arm_id| {
                        let arm = &body.match_arms[*arm_id];

                        // Push a scope for the arm's pattern bindings
                        ctx.push_scope();

                        // Extract pattern and determine the narrowed type
                        let pattern = &body.patterns[arm.pattern];
                        let (binding_name, narrowed_ty) =
                            extract_pattern_binding(ctx, pattern, &scrutinee_ty, body);

                        // Bind the pattern variable with the narrowed type
                        if let Some(name) = binding_name {
                            ctx.define(name, narrowed_ty);
                        }

                        // Type-check the guard (if present)
                        if let Some(guard) = arm.guard {
                            let guard_ty = infer_expr(ctx, guard, body);
                            if !ctx.is_subtype_of(&guard_ty, &Ty::Bool) && !guard_ty.is_unknown() {
                                ctx.push_error(TypeError::TypeMismatch {
                                    expected: Ty::Bool,
                                    found: guard_ty,
                                    location,
                                    info_location: None,
                                });
                            }
                        }

                        // Type-check the arm body
                        let body_ty = infer_expr(ctx, arm.body, body);

                        ctx.pop_scope();
                        body_ty
                    })
                    .collect();

                // If all arms have the same type, use that; otherwise union
                if arm_types.iter().all(|t| t == &arm_types[0]) {
                    arm_types.into_iter().next().unwrap_or(Ty::Unknown)
                } else {
                    Ty::Union(arm_types)
                }
            }
        }

        Expr::Missing => Ty::Unknown,
    };

    ctx.set_expr_type(expr_id, ty.clone());
    ty
}

/// Check that an expression has the expected type (checking mode).
///
/// In bidirectional type checking, checking mode is used when we know what type
/// we expect an expression to have. This allows for better type inference in many
/// cases compared to synthesis followed by subtype checking.
///
/// Returns the actual type of the expression (which should be a subtype of expected).
fn check_expr(ctx: &mut TypeContext<'_>, expr_id: ExprId, body: &ExprBody, expected: &Ty) -> Ty {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[expr_id];
    let location = ErrorLocation::Expr(expr_id);

    let ty = match expr {
        // For most cases, we synthesize then check subtyping
        // But some cases can use the expected type for better inference
        Expr::Block { stmts, tail_expr } => {
            ctx.push_scope();

            // Type check statements with expected return type for better checking
            for &stmt_id in stmts {
                check_stmt_with_return(ctx, stmt_id, body, Some(expected));
            }

            // Check tail expression against expected type
            let result = if let Some(tail) = tail_expr {
                check_expr(ctx, *tail, body, expected)
            } else {
                // No tail expression means the block evaluates to void
                // This is fine - the function might return via explicit return statements
                Ty::Void
            };

            ctx.pop_scope();
            result
        }

        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            // Check condition against Bool type (checking mode)
            check_expr(ctx, *condition, body, &Ty::Bool);

            // Check for instanceof narrowing (same as infer_expr)
            let instanceof_narrowing = extract_instanceof_narrowing(ctx, *condition, body);

            // Check then-branch with narrowed type if applicable
            let then_ty = if let Some((var_name, narrowed_ty)) = &instanceof_narrowing {
                ctx.push_scope();
                ctx.define(var_name.clone(), narrowed_ty.clone());
                let ty = check_expr(ctx, *then_branch, body, expected);
                ctx.pop_scope();
                ty
            } else {
                check_expr(ctx, *then_branch, body, expected)
            };

            let else_ty = if let Some(else_expr) = else_branch {
                check_expr(ctx, *else_expr, body, expected)
            } else {
                Ty::Void
            };

            // In checking mode, don't generalize - the branches were checked against
            // the expected type, so return the union of actual types (or expected if they match)
            if then_ty == else_ty {
                then_ty
            } else if else_branch.is_none() {
                // if without else returns optional
                Ty::Union(vec![then_ty, Ty::Null])
            } else {
                Ty::Union(vec![then_ty, else_ty])
            }
        }

        Expr::Array { elements } => {
            // If we expect a specific list type, use it to check elements
            if let Ty::List(expected_elem) = expected {
                if elements.is_empty() {
                    Ty::List(expected_elem.clone())
                } else {
                    // Check all elements against the expected element type
                    for &elem in elements {
                        let elem_ty = check_expr(ctx, elem, body, expected_elem);
                        if !ctx.is_subtype_of(&elem_ty, expected_elem) {
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: (**expected_elem).clone(),
                                found: generalize_for_error(expected_elem, &elem_ty),
                                location,
                                info_location: None,
                            });
                        }
                    }
                    expected.clone()
                }
            } else {
                // Fall back to synthesis
                let ty = infer_expr(ctx, expr_id, body);
                if !ctx.is_subtype_of(&ty, expected)
                    && !expected.is_unknown()
                    && !expected.is_error()
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: generalize_for_error(expected, &ty),
                        location,
                        info_location: None,
                    });
                }
                ty
            }
        }

        Expr::Object {
            type_name,
            fields,
            spreads: _,
        } => {
            // Store resolution for IDE features if this is a class instantiation
            if let Some(name) = type_name {
                if ctx.class_names.contains(name) {
                    use baml_compiler_hir::FullyQualifiedName;
                    ctx.set_expr_resolution(
                        expr_id,
                        ResolvedValue::Class(FullyQualifiedName::local(name.clone())),
                    );
                }
            }

            // If we expect a specific class type, we can use its field types
            if let Ty::Class(expected_fqn) = expected {
                use baml_compiler_hir::FullyQualifiedName;
                // Check field types against the expected class fields
                for (field_name, value_expr) in fields {
                    // Clone the field type to avoid borrow issues
                    let expected_field_ty = ctx
                        .lookup_class_field(&expected_fqn.name, field_name)
                        .cloned();
                    if let Some(field_ty) = expected_field_ty {
                        check_expr(ctx, *value_expr, body, &field_ty);
                    } else {
                        // Field doesn't exist in expected type - still infer it for error reporting
                        infer_expr(ctx, *value_expr, body);
                    }
                }

                // Return the expected type if type_name matches
                if type_name.as_ref() == Some(&expected_fqn.name) {
                    expected.clone()
                } else if let Some(name) = type_name {
                    Ty::Class(FullyQualifiedName::local(name.clone()))
                } else {
                    Ty::Unknown
                }
            } else if let Ty::TypeAlias(expected_fqn) = expected {
                use baml_compiler_hir::FullyQualifiedName;
                // Similar handling for TypeAlias types
                for (field_name, value_expr) in fields {
                    let expected_field_ty = ctx
                        .lookup_class_field(&expected_fqn.name, field_name)
                        .cloned();
                    if let Some(field_ty) = expected_field_ty {
                        check_expr(ctx, *value_expr, body, &field_ty);
                    } else {
                        infer_expr(ctx, *value_expr, body);
                    }
                }

                if type_name.as_ref() == Some(&expected_fqn.name) {
                    expected.clone()
                } else if let Some(name) = type_name {
                    Ty::TypeAlias(FullyQualifiedName::local(name.clone()))
                } else {
                    Ty::Unknown
                }
            } else {
                // Fall back to synthesis
                let ty = infer_expr(ctx, expr_id, body);
                if !ctx.is_subtype_of(&ty, expected)
                    && !expected.is_unknown()
                    && !expected.is_error()
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: generalize_for_error(expected, &ty),
                        location,
                        info_location: None,
                    });
                }
                ty
            }
        }

        Expr::Map { entries } => {
            // If we expect a specific map type, use it to check entries
            if let Ty::Map {
                key: expected_key,
                value: expected_value,
            } = expected
            {
                if entries.is_empty() {
                    Ty::Map {
                        key: expected_key.clone(),
                        value: expected_value.clone(),
                    }
                } else {
                    // Check all entries against the expected key/value types
                    for &(key_expr, value_expr) in entries {
                        let key_ty = check_expr(ctx, key_expr, body, expected_key);
                        if !ctx.is_subtype_of(&key_ty, expected_key) {
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: (**expected_key).clone(),
                                found: generalize_for_error(expected_key, &key_ty),
                                location,
                                info_location: None,
                            });
                        }
                        let value_ty = check_expr(ctx, value_expr, body, expected_value);
                        if !ctx.is_subtype_of(&value_ty, expected_value) {
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: (**expected_value).clone(),
                                found: generalize_for_error(expected_value, &value_ty),
                                location,
                                info_location: None,
                            });
                        }
                    }
                    expected.clone()
                }
            } else {
                // Fall back to synthesis
                let ty = infer_expr(ctx, expr_id, body);
                if !ctx.is_subtype_of(&ty, expected)
                    && !expected.is_unknown()
                    && !expected.is_error()
                {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: generalize_for_error(expected, &ty),
                        location,
                        info_location: None,
                    });
                }
                ty
            }
        }

        // For all other cases, synthesize then check
        _ => {
            let ty = infer_expr(ctx, expr_id, body);
            if !ctx.is_subtype_of(&ty, expected)
                && !expected.is_unknown()
                && !expected.is_error()
                && !ty.is_unknown()
            {
                // Generalize found type for clearer error messages
                // e.g., "Expected int[], found int" instead of "Expected int[], found 42"
                // But preserve literals when expected is also a literal (e.g., "Expected 4, found 3")
                ctx.push_error(TypeError::TypeMismatch {
                    expected: expected.clone(),
                    found: generalize_for_error(expected, &ty),
                    location,
                    info_location: None,
                });
            }
            ty
        }
    };

    ctx.set_expr_type(expr_id, ty.clone());
    ty
}

/// Extract binding name and narrowed type from a match pattern.
///
/// Returns `(Some(name), narrowed_type)` for binding patterns, or `(None, scrutinee_type)` for
/// patterns that don't introduce bindings (literals, enum variants, unions).
///
/// # Type Narrowing Rules
/// - `name: Type` binds `name` with type `Type` (from the type annotation)
/// - `name` (without type) binds `name` with the scrutinee type (catch-all)
/// - `_` is a special case of binding that's semantically discarded later
/// - Literals, enum variants, and union patterns don't introduce bindings
fn extract_pattern_binding(
    ctx: &TypeContext<'_>,
    pattern: &Pattern,
    scrutinee_ty: &Ty,
    _body: &ExprBody,
) -> (Option<Name>, Ty) {
    match pattern {
        // Typed binding: `s: Success` -> s has type Success
        Pattern::TypedBinding { name, ty } => {
            let narrowed_ty = ctx.lower_type_resolved(ty, Span::default());
            (Some(name.clone()), narrowed_ty)
        }

        // Simple binding: `x` or `_` -> binds with scrutinee type (catch-all)
        Pattern::Binding(name) => {
            // `_` is semantically discarded but still creates a binding during type checking
            // The "discard" behavior is handled in codegen, not here
            (Some(name.clone()), scrutinee_ty.clone())
        }

        // Literal patterns don't introduce bindings
        Pattern::Literal(_) => (None, scrutinee_ty.clone()),

        // Enum variant patterns don't introduce bindings
        // (they match by value equality, not type)
        Pattern::EnumVariant { .. } => (None, scrutinee_ty.clone()),

        // Union patterns don't introduce bindings
        // (they're unions of literals or enum variants)
        Pattern::Union(_) => (None, scrutinee_ty.clone()),
    }
}

// ============================================================================
// Match Exhaustiveness and Unreachability Checking
// ============================================================================
//
// Exhaustiveness checking is implemented in the `exhaustiveness` module using
// a value-based model (`ValueSet`). See `exhaustiveness.rs` for details.
//
// Key design principle: Pattern matching operates on VALUES, not types.
// - `Status.Active` matches one specific value
// - `s: Status` matches all values of type Status
// - `_` or `other` matches everything (catch-all)
//
// The `ExhaustivenessChecker` tracks which value sets have been covered
// by match arms and reports:
// - Non-exhaustive matches (uncovered cases)
// - Unreachable arms (arms that can never match)
// ============================================================================

/// Check match exhaustiveness and detect unreachable arms.
///
/// This function implements the exhaustiveness checking rules from BEP-002:
/// 1. All cases must be covered explicitly or via catch-all
/// 2. Guards do NOT contribute to exhaustiveness
/// 3. Catch-all (`_` or untyped binding) covers remaining cases
/// 4. Arms after a catch-all are unreachable
///
/// # Errors
/// - `TypeError::NonExhaustiveMatch` if not all cases are covered
/// - `TypeError::UnreachableArm` if an arm can never match
fn check_match_exhaustiveness(
    ctx: &mut TypeContext<'_>,
    scrutinee_ty: &Ty,
    arm_ids: &[MatchArmId],
    body: &ExprBody,
    match_expr_id: ExprId,
) {
    // Skip exhaustiveness checking for unknown/error types
    if scrutinee_ty.is_unknown() || scrutinee_ty.is_error() {
        return;
    }

    // Use the new value-based exhaustiveness checker
    let checker = ExhaustivenessChecker::new(
        &ctx.enum_variants,
        &ctx.type_aliases,
        &ctx.class_names,
        &ctx.enum_names,
        &ctx.known_types,
    );

    let result = checker.check(scrutinee_ty, arm_ids, body);

    // Report unreachable arms using position-independent MatchArmId
    for arm_idx in result.unreachable_arms {
        let arm_id = arm_ids[arm_idx];
        ctx.push_error(TypeError::UnreachableArm {
            location: ErrorLocation::MatchArm(arm_id),
        });
    }

    // Report non-exhaustive match (points to the match expression itself)
    if !result.is_exhaustive {
        let missing_cases: Vec<String> = result
            .uncovered
            .iter()
            .map(std::string::ToString::to_string)
            .collect();

        ctx.push_error(TypeError::NonExhaustiveMatch {
            scrutinee_type: scrutinee_ty.clone(),
            missing_cases,
            location: ErrorLocation::Expr(match_expr_id),
        });
    } else {
        // Record that this match is exhaustive for codegen optimization
        ctx.exhaustive_matches.insert(match_expr_id);
    }
}

/// Infer the type of a literal.
///
/// Returns literal types (singleton types) for better bidirectional type checking.
/// For example, the literal `42` has type `Ty::Literal(LiteralValue::Int(42))`,
/// which is a subtype of `Ty::Int`.
fn infer_literal(lit: &baml_compiler_hir::Literal) -> Ty {
    use crate::types::LiteralValue;
    match lit {
        baml_compiler_hir::Literal::Int(n) => Ty::Literal(LiteralValue::Int(*n)),
        baml_compiler_hir::Literal::Float(f) => Ty::Literal(LiteralValue::Float(f.clone())),
        baml_compiler_hir::Literal::String(s) => Ty::Literal(LiteralValue::String(s.clone())),
        baml_compiler_hir::Literal::Bool(b) => Ty::Literal(LiteralValue::Bool(*b)),
        baml_compiler_hir::Literal::Null => Ty::Null,
    }
}

/// Generalize a literal type to its base type (reference version for error messages).
///
/// Used in error messages where we want to show "int" instead of "42".
/// For operator errors, the issue is type compatibility, not the specific value.
fn generalize(ty: &Ty) -> Ty {
    use crate::types::LiteralValue;
    match ty {
        Ty::Literal(LiteralValue::Int(_)) => Ty::Int,
        Ty::Literal(LiteralValue::Float(_)) => Ty::Float,
        Ty::Literal(LiteralValue::String(_)) => Ty::String,
        Ty::Literal(LiteralValue::Bool(_)) => Ty::Bool,
        other => other.clone(),
    }
}

/// Generalize the found type for error messages, but preserve literals when expected is also a literal.
///
/// When expected is a literal type (like `4`), we want to show "Expected `4`, found `3`"
/// rather than "Expected `4`, found `int`". But when expected is a base type like `int[]`,
/// we want to show "Expected `int[]`, found `int`" rather than "Expected `int[]`, found `42`".
fn generalize_for_error(expected: &Ty, found: &Ty) -> Ty {
    if matches!(expected, Ty::Literal(_)) {
        // Keep literal types when expected is also a literal
        found.clone()
    } else {
        generalize(found)
    }
}

/// Extract instanceof narrowing info from a condition expression.
///
/// If the condition is `x instanceof Foo`, returns `Some((x, Foo_type))`.
/// Otherwise returns `None`.
fn extract_instanceof_narrowing(
    _ctx: &TypeContext<'_>,
    condition: ExprId,
    body: &ExprBody,
) -> Option<(Name, Ty)> {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[condition];

    // Check if this is an instanceof expression
    if let Expr::Binary { op, lhs, rhs } = expr {
        if *op == baml_compiler_hir::BinaryOp::Instanceof {
            // LHS should be a simple path (variable name)
            if let Expr::Path(segments) = &body.exprs[*lhs] {
                if segments.len() == 1 {
                    let var_name = segments[0].clone();

                    // RHS should be a simple path (type name)
                    if let Expr::Path(type_segments) = &body.exprs[*rhs] {
                        if type_segments.len() == 1 {
                            use baml_compiler_hir::FullyQualifiedName;
                            let type_name = type_segments[0].clone();
                            // Return the variable name and the narrowed type
                            // Use TypeAlias as a fallback - will be resolved during normalization
                            return Some((
                                var_name,
                                Ty::TypeAlias(FullyQualifiedName::local(type_name)),
                            ));
                        }
                    }
                }
            }
        }
    }

    None
}

/// Infer the result type of a binary operation.
fn infer_binary_op(
    ctx: &mut TypeContext<'_>,
    op: baml_compiler_hir::BinaryOp,
    lhs: &Ty,
    rhs: &Ty,
    location: ErrorLocation,
) -> Ty {
    use baml_compiler_hir::BinaryOp::{
        Add, And, BitAnd, BitOr, BitXor, Div, Eq, Ge, Gt, Instanceof, Le, Lt, Mod, Mul, Ne, Or,
        Shl, Shr, Sub,
    };

    use crate::types::LiteralValue;

    // Don't emit errors for operations involving unknown or error types - the root cause
    // (e.g., unknown variable) has already been reported
    if lhs.is_unknown() || lhs.is_error() || rhs.is_unknown() || rhs.is_error() {
        return Ty::Unknown;
    }

    // Helper to check if a type is int-like (Int or Int literal)
    let is_int_like = |ty: &Ty| matches!(ty, Ty::Int | Ty::Literal(LiteralValue::Int(_)));
    // Helper to check if a type is float-like (Float or Float literal)
    let is_float_like = |ty: &Ty| matches!(ty, Ty::Float | Ty::Literal(LiteralValue::Float(_)));
    // Helper to check if a type is string-like (String or String literal)
    let is_string_like = |ty: &Ty| matches!(ty, Ty::String | Ty::Literal(LiteralValue::String(_)));
    // Helper to check if a type is bool-like (Bool or Bool literal)
    let is_bool_like = |ty: &Ty| matches!(ty, Ty::Bool | Ty::Literal(LiteralValue::Bool(_)));

    match op {
        // Arithmetic operations (and string concatenation for Add)
        Add => {
            if is_int_like(lhs) && is_int_like(rhs) {
                Ty::Int
            } else if (is_int_like(lhs) || is_float_like(lhs))
                && (is_int_like(rhs) || is_float_like(rhs))
            {
                Ty::Float
            } else if is_string_like(lhs) && is_string_like(rhs) {
                // String concatenation
                Ty::String
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error
            }
        }
        Sub | Mul | Div | Mod => {
            if is_int_like(lhs) && is_int_like(rhs) {
                Ty::Int
            } else if (is_int_like(lhs) || is_float_like(lhs))
                && (is_int_like(rhs) || is_float_like(rhs))
            {
                Ty::Float
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error
            }
        }

        // Comparison operations
        Eq | Ne => Ty::Bool,

        Lt | Le | Gt | Ge => {
            let numeric_lhs = is_int_like(lhs) || is_float_like(lhs);
            let numeric_rhs = is_int_like(rhs) || is_float_like(rhs);
            if numeric_lhs && numeric_rhs {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error
            }
        }

        // Logical operations
        And | Or => {
            if is_bool_like(lhs) && is_bool_like(rhs) {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error
            }
        }

        // Bitwise operations
        BitAnd | BitOr | BitXor | Shl | Shr => {
            if is_int_like(lhs) && is_int_like(rhs) {
                Ty::Int
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: generalize(lhs),
                    rhs: generalize(rhs),
                    location,
                });
                Ty::Error
            }
        }

        // Type checking operations
        Instanceof => Ty::Bool,
    }
}

/// Infer the result type of a unary operation.
fn infer_unary_op(
    ctx: &mut TypeContext<'_>,
    op: baml_compiler_hir::UnaryOp,
    operand: &Ty,
    location: ErrorLocation,
) -> Ty {
    use baml_compiler_hir::UnaryOp::{Neg, Not};

    use crate::types::LiteralValue;

    // Don't emit errors for operations involving unknown or error types - the root cause
    // has already been reported
    if operand.is_unknown() || operand.is_error() {
        return Ty::Unknown;
    }

    match op {
        Not => {
            if matches!(operand, Ty::Bool | Ty::Literal(LiteralValue::Bool(_))) {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "!".to_string(),
                    operand: generalize(operand),
                    location,
                });
                Ty::Error
            }
        }
        Neg => match operand {
            Ty::Int | Ty::Literal(LiteralValue::Int(_)) => Ty::Int,
            Ty::Float | Ty::Literal(LiteralValue::Float(_)) => Ty::Float,
            _ => {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "-".to_string(),
                    operand: generalize(operand),
                    location,
                });
                Ty::Error
            }
        },
    }
}

/// Infer the type of a field access.
///
/// For class types, this handles both field access and method access.
/// For primitive types (arrays, strings, maps), this handles builtin methods.
fn infer_field_access(
    ctx: &mut TypeContext<'_>,
    base: &Ty,
    field: &Name,
    location: ErrorLocation,
) -> Ty {
    // Special case: $watch accessor on any type
    // The actual watched check happens at MIR lowering time
    if field.as_str() == "$watch" {
        return Ty::WatchAccessor(Box::new(base.clone()));
    }

    // Special case: methods on WatchAccessor type
    if let Ty::WatchAccessor(inner_ty) = base {
        match field.as_str() {
            "options" => {
                // $watch.options(filter) - filter can be a function, "manual", or "never"
                // Returns null (void operation)
                return Ty::Function {
                    // First param is receiver (the watched value), second is filter
                    params: vec![*inner_ty.clone(), Ty::Unknown], // Filter type is flexible
                    ret: Box::new(Ty::Null),
                };
            }
            "notify" => {
                // $watch.notify() - manually trigger notification
                // Returns null (void operation)
                return Ty::Function {
                    params: vec![*inner_ty.clone()], // Just the receiver
                    ret: Box::new(Ty::Null),
                };
            }
            _ => {
                ctx.push_error(TypeError::NoSuchField {
                    ty: base.clone(),
                    field: field.to_string(),
                    location,
                });
                return Ty::Unknown;
            }
        }
    }

    // First, try class field lookup for named types
    let found_field = match base {
        Ty::TypeAlias(fqn) => ctx
            .lookup(field)
            .or(ctx.lookup_class_field(&fqn.name, field))
            .cloned(),
        Ty::Class(fqn) => {
            // First try to find a method (global function lookup)
            if let Some(method_ty) = ctx.lookup(field) {
                return method_ty.clone();
            }
            // Check the context's class_fields for this class name
            ctx.lookup_class_field(&fqn.name, field).cloned()
        }
        Ty::Unknown => return Ty::Unknown,
        _ => None,
    };

    if let Some(ty) = found_field {
        return ty;
    }

    // Try builtin method lookup
    if let Some((def, bindings)) = builtins::lookup_method(base, field.as_str()) {
        // Build the function type from the builtin definition.
        // If this is a method (has a receiver), include the receiver type as the first param
        // since the Call handler will pass the receiver as the first argument.
        let mut param_types: Vec<Ty> = Vec::new();
        if def.receiver.is_some() {
            param_types.push(base.clone());
        }
        for (_, pattern) in &def.params {
            param_types.push(builtins::substitute(pattern, &bindings));
        }
        let return_type = builtins::substitute(&def.returns, &bindings);

        return Ty::Function {
            params: param_types,
            ret: Box::new(return_type),
        };
    }

    // Field/method not found
    ctx.push_error(TypeError::NoSuchField {
        ty: base.clone(),
        field: field.to_string(),
        location,
    });
    Ty::Unknown
}

/// Infer the type of an index access.
fn infer_index_access(
    ctx: &mut TypeContext<'_>,
    base: &Ty,
    index: &Ty,
    location: ErrorLocation,
) -> Ty {
    match base {
        Ty::List(elem) => {
            // Index must be int
            if !ctx.is_subtype_of(index, &Ty::Int) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int,
                    found: index.clone(),
                    location,
                    info_location: None,
                });
            }
            (**elem).clone()
        }
        Ty::Map { key, value } => {
            // Index must match key type
            if !ctx.is_subtype_of(index, key) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: (**key).clone(),
                    found: index.clone(),
                    location,
                    info_location: None,
                });
            }
            (**value).clone()
        }
        Ty::String => {
            // String indexing returns a character (string of length 1)
            if !ctx.is_subtype_of(index, &Ty::Int) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int,
                    found: index.clone(),
                    location,
                    info_location: None,
                });
            }
            Ty::String
        }
        Ty::Unknown => Ty::Unknown,
        _ => {
            ctx.push_error(TypeError::NotIndexable {
                ty: base.clone(),
                location,
            });
            Ty::Unknown
        }
    }
}

/// Type check a statement.
fn check_stmt(ctx: &mut TypeContext<'_>, stmt_id: StmtId, body: &ExprBody) {
    check_stmt_with_return(ctx, stmt_id, body, None);
}

/// Type check a statement with an optional expected return type for better checking.
///
/// When `expected_return` is provided, return statements and let initializers
/// can use bidirectional type checking for better error messages and inference.
fn check_stmt_with_return(
    ctx: &mut TypeContext<'_>,
    stmt_id: StmtId,
    body: &ExprBody,
    expected_return: Option<&Ty>,
) {
    use baml_compiler_hir::Stmt;

    let stmt = &body.stmts[stmt_id];

    match stmt {
        Stmt::Let {
            pattern,
            type_annotation,
            type_span,
            initializer,
            is_watched,
        } => {
            let ty = if let Some(init) = initializer {
                // If there's a type annotation, use check_expr for bidirectional typing
                if let Some(annot) = type_annotation {
                    let span = ctx.build_span_default(type_span);
                    let annot_ty = ctx.lower_type_resolved(annot, span);
                    // Use check_expr when we have an expected type
                    // check_expr already reports any type mismatch errors
                    check_expr(ctx, *init, body, &annot_ty);
                    annot_ty
                } else {
                    // No type annotation - infer and generalize for mutable variables
                    // This ensures `let x = 5` gives `x : int`, not `x : 5`
                    let inferred = infer_expr(ctx, *init, body);
                    generalize(&inferred)
                }
            } else if let Some(annot) = type_annotation {
                ctx.lower_type_resolved(annot, Span::default())
            } else {
                Ty::Unknown
            };

            // Extract variable name from pattern and track watched status
            let pat = &body.patterns[*pattern];
            match pat {
                Pattern::Binding(name) => {
                    ctx.define_with_site(name.clone(), ty, DefinitionSite::Statement(stmt_id));
                    if *is_watched {
                        ctx.mark_watched(name.clone());
                    }
                }
                Pattern::TypedBinding { name, ty: _ } => {
                    // TODO: Check declared type matches inferred type
                    ctx.define_with_site(name.clone(), ty, DefinitionSite::Statement(stmt_id));
                    if *is_watched {
                        ctx.mark_watched(name.clone());
                    }
                }
                Pattern::Literal(_) | Pattern::EnumVariant { .. } | Pattern::Union(_) => {
                    // Literals/enum variants/unions don't introduce bindings in let statements
                    // This would be a semantic error, but we'll handle it elsewhere
                }
            }
        }

        Stmt::Expr(expr) => {
            infer_expr(ctx, *expr, body);
        }

        Stmt::Return(expr) => {
            let return_ty = if let Some(e) = expr {
                // If we have an expected return type, use check_expr for bidirectional typing
                if let Some(expected) = expected_return {
                    check_expr(ctx, *e, body, expected)
                } else {
                    infer_expr(ctx, *e, body)
                }
            } else {
                Ty::Void
            };
            // Record return type (span resolved at render time if needed)
            ctx.record_return(return_ty, Span::default());
        }

        Stmt::While {
            condition,
            body: while_body,
            after,
            origin: _, // origin is used for diagnostics, not type checking
        } => {
            // Check condition against Bool (bidirectional)
            check_expr(ctx, *condition, body, &Ty::Bool);
            infer_expr(ctx, *while_body, body);
            // Type-check the after statement (for desugared C-style for loops)
            if let Some(after_stmt) = after {
                check_stmt_with_return(ctx, *after_stmt, body, expected_return);
            }
        }

        Stmt::Break | Stmt::Continue => {
            // These are control flow statements with no expressions to type-check.
            // Loop context validation could be added here in the future.
        }

        Stmt::Assign { target, value } => {
            // Type-check both the target and value expressions
            let target_ty = infer_expr(ctx, *target, body);
            let value_ty = infer_expr(ctx, *value, body);
            // Check that value type is compatible with target type
            if !ctx.is_subtype_of(&value_ty, &target_ty) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: target_ty.clone(),
                    found: generalize_for_error(&target_ty, &value_ty),
                    location: ErrorLocation::Expr(*value),
                    info_location: None,
                });
            }
        }

        Stmt::AssignOp {
            target,
            op: _,
            value,
        } => {
            // Type-check both the target and value expressions
            let target_ty = infer_expr(ctx, *target, body);
            let value_ty = infer_expr(ctx, *value, body);
            // Check that value type is compatible with target type
            if !ctx.is_subtype_of(&value_ty, &target_ty) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: target_ty.clone(),
                    found: generalize_for_error(&target_ty, &value_ty),
                    location: ErrorLocation::Expr(*value),
                    info_location: None,
                });
            }
        }

        Stmt::Assert { condition } => {
            // Type-check the condition expression (bidirectional)
            check_expr(ctx, *condition, body, &Ty::Bool);
        }

        Stmt::Missing => {}

        Stmt::HeaderComment { .. } => {
            // Header comments don't need type checking - they're just annotations
        }
    }
}
