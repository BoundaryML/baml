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

use std::collections::{HashMap, HashSet};

use baml_base::{FileId, Name, Span};
use baml_diagnostics::compiler_error::TypeError;
use baml_hir::{ExprBody, ExprId, FunctionBody, FunctionLoc, FunctionSignature, Pattern, StmtId};
use baml_workspace::Project;

pub mod builtins;
mod exhaustiveness;
mod lower;
pub mod pretty;
mod types;

pub use builtins::{
    Bindings, lookup_function, lookup_method, match_pattern, method_param_types,
    method_return_type, substitute,
};
pub use exhaustiveness::{ExhaustivenessChecker, ExhaustivenessResult, ValueSet};
pub use lower::{TypeLoweringContext, lower_type_ref, lower_type_ref_validated};
pub use pretty::{expr_to_string, render_body_tree, render_function_tree};
use text_size::TextRange;
pub use types::*;

/// Substitute type variable bindings into a `TypePattern`, falling back to `Ty::Unknown`
/// for unbound type variables.
///
/// This is used for builtin function type inference where some type variables may be
/// bound from arguments but others might not be.
fn substitute_with_fallback<'db>(
    pattern: &baml_vm::TypePattern,
    bindings: &Bindings<'db>,
) -> Ty<'db> {
    use baml_vm::TypePattern;
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
/// This trait extends `baml_hir::Db` and provides access to all TIR-related
/// Salsa queries, including type inference and the initial typing context.
#[salsa::db]
pub trait Db: baml_hir::Db {}

// ============================================================================
// Tracked Struct for Enum Variants (no Ty<'db>, so this works)
// ============================================================================

/// Tracked struct holding enum variants (enum name -> variant names).
/// This works because it doesn't contain `Ty<'db>`.
#[salsa::tracked]
pub struct EnumVariantsMap<'db> {
    #[tracked]
    #[returns(ref)]
    pub enums: HashMap<Name, Vec<Name>>,
}

// ============================================================================
// TIR Queries
// ============================================================================

/// Query: Get enum variants for a project.
///
/// Maps enum names to their variant names, e.g.:
/// `Status` -> `[Active, Inactive, Pending]`
///
/// This is a proper Salsa query because it doesn't return `Ty<'db>`.
#[salsa::tracked]
pub fn enum_variants(db: &dyn Db, project: Project) -> EnumVariantsMap<'_> {
    let items = baml_hir::project_items(db, project);
    let mut enums = HashMap::new();

    for item in items.items(db) {
        if let baml_hir::ItemId::Enum(enum_loc) = item {
            let file = enum_loc.file(db);
            let item_tree = baml_hir::file_item_tree(db, file);
            let enum_data = &item_tree[enum_loc.id(db)];

            let variants: Vec<Name> = enum_data.variants.iter().map(|v| v.name.clone()).collect();
            enums.insert(enum_data.name.clone(), variants);
        }
    }

    EnumVariantsMap::new(db, enums)
}

// ============================================================================
// Non-Salsa Helper Functions
// ============================================================================
//
// These functions return `Ty<'db>` which cannot be stored in Salsa tracked
// structs because `Ty` is not interned. To make these proper Salsa queries,
// we would need to intern `Ty` (make it `#[salsa::interned]`).
//
// For now, these are computed on-demand. The underlying HIR queries they
// depend on (project_items, project_class_fields, etc.) ARE cached by Salsa.

/// Get the typing context for a project.
///
/// Maps function names to their arrow types, e.g.:
/// `Foo` -> `(int) -> int` for `function Foo(x: int) -> int`
pub fn typing_context<'db>(db: &'db dyn Db, project: Project) -> HashMap<Name, Ty<'db>> {
    let files = baml_workspace::project_files(db, project);
    let mut context = HashMap::new();

    for file in files {
        let items_struct = baml_hir::file_items(db, file);
        let items = items_struct.items(db);

        for item in items {
            if let baml_hir::ItemId::Function(func_loc) = item {
                let signature = baml_hir::function_signature(db, *func_loc);

                let param_types: Vec<Ty<'db>> = signature
                    .params
                    .iter()
                    .map(|p| lower_type_ref(db, &p.type_ref))
                    .collect();

                let return_type = lower_type_ref(db, &signature.return_type);

                let func_type = Ty::Function {
                    params: param_types,
                    ret: Box::new(return_type),
                };

                context.insert(signature.name.clone(), func_type);
            }
        }
    }

    context
}

/// Get class field types for a project.
///
/// Maps class names to their field types, e.g.:
/// `Baz` -> { `name` -> `String` }
pub fn class_field_types(db: &dyn Db, project: Project) -> HashMap<Name, HashMap<Name, Ty<'_>>> {
    let hir_fields = baml_hir::project_class_fields(db, project);

    hir_fields
        .classes(db)
        .iter()
        .map(|(class_name, fields)| {
            let lowered_fields = fields
                .iter()
                .map(|(field_name, type_ref)| (field_name.clone(), lower_type_ref(db, type_ref)))
                .collect();
            (class_name.clone(), lowered_fields)
        })
        .collect()
}

/// Get type alias definitions for a project.
///
/// Maps type alias names to their resolved types, e.g.:
/// `Result` -> `Success | Failure`
pub fn type_aliases(db: &dyn Db, project: Project) -> HashMap<Name, Ty<'_>> {
    let items = baml_hir::project_items(db, project);
    let mut aliases = HashMap::new();

    for item in items.items(db) {
        if let baml_hir::ItemId::TypeAlias(alias_loc) = item {
            let file = alias_loc.file(db);
            let item_tree = baml_hir::file_item_tree(db, file);
            let alias_data = &item_tree[alias_loc.id(db)];

            let lowered_ty = lower_type_ref(db, &alias_data.type_ref);
            aliases.insert(alias_data.name.clone(), lowered_ty);
        }
    }

    aliases
}

// ============================================================================
// Type Inference Results
// ============================================================================

/// Result of type inference for a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResult<'db> {
    /// Inferred return type of the function.
    pub return_type: Ty<'db>,
    /// Types of parameters.
    pub param_types: HashMap<Name, Ty<'db>>,
    /// Types inferred for each expression.
    pub expr_types: HashMap<ExprId, Ty<'db>>,
    /// For multi-segment path expressions, the type of each segment.
    /// For `o.inner.value` where `o: Outer`, stores `[Outer, Inner, int]`.
    /// Used by codegen to look up field indices at each step.
    pub path_segment_types: HashMap<ExprId, Vec<Ty<'db>>>,
    /// Expressions that are enum variant values (e.g., `Status.Active`).
    /// Maps expression ID to (`enum_name`, `variant_name`).
    /// Used by codegen to emit enum variant construction.
    pub enum_variant_exprs: HashMap<ExprId, (Name, Name)>,
    /// Match expressions that are exhaustive (all cases covered).
    /// Used by codegen to emit `unreachable` for fallthrough paths,
    /// enabling phi-like optimization for match results.
    pub exhaustive_matches: HashSet<ExprId>,
    /// Type checking errors.
    pub errors: Vec<TypeError<Ty<'db>>>,
}

// ============================================================================
// Type Context
// ============================================================================

/// Context for type inference, tracking scopes and accumulated results.
pub struct TypeContext<'db> {
    db: &'db dyn Db,
    /// Stack of variable scopes (innermost last).
    scopes: Vec<HashMap<Name, Ty<'db>>>,
    /// Class field types: `class_name` -> (`field_name` -> `field_type`)
    class_fields: HashMap<Name, HashMap<Name, Ty<'db>>>,
    /// Type alias definitions: `alias_name` -> `resolved_type`
    type_aliases: HashMap<Name, Ty<'db>>,
    /// Enum variant definitions: `enum_name` -> `Vec<variant_name>`
    enum_variants: HashMap<Name, Vec<Name>>,
    /// Inferred types for expressions.
    expr_types: HashMap<ExprId, Ty<'db>>,
    /// For multi-segment paths, the type of each segment.
    path_segment_types: HashMap<ExprId, Vec<Ty<'db>>>,
    /// Expressions that are enum variant values.
    enum_variant_exprs: HashMap<ExprId, (Name, Name)>,
    /// Match expressions that are exhaustive (all cases covered).
    exhaustive_matches: HashSet<ExprId>,
    /// Types of all return statements encountered during inference.
    /// Used to validate that all return paths match the declared return type.
    return_types: Vec<(Ty<'db>, Span)>,
    /// Accumulated type errors.
    errors: Vec<TypeError<Ty<'db>>>,
    /// The current file being typechecked
    file_id: FileId,
    /// Variables declared with `watch let` (tracked for $watch validation).
    watched_vars: HashSet<Name>,
}

impl<'db> TypeContext<'db> {
    /// Create a new type context with an initial scope of global bindings.
    ///
    /// The initial scope typically contains top-level function types, allowing
    /// function calls to be properly typed. Pass an empty `HashMap` for no globals.
    pub fn new(db: &'db dyn Db, globals: HashMap<Name, Ty<'db>>, file_id: FileId) -> Self {
        TypeContext {
            db,
            scopes: vec![globals],
            class_fields: HashMap::new(),
            type_aliases: HashMap::new(),
            enum_variants: HashMap::new(),
            expr_types: HashMap::new(),
            path_segment_types: HashMap::new(),
            enum_variant_exprs: HashMap::new(),
            exhaustive_matches: HashSet::new(),
            return_types: Vec::new(),
            errors: Vec::new(),
            file_id,
            watched_vars: HashSet::new(),
        }
    }

    /// Create a new type context with global bindings and class field information.
    pub fn with_class_fields(
        db: &'db dyn Db,
        globals: HashMap<Name, Ty<'db>>,
        class_fields: HashMap<Name, HashMap<Name, Ty<'db>>>,
        file_id: FileId,
    ) -> Self {
        TypeContext {
            db,
            scopes: vec![globals],
            class_fields,
            type_aliases: HashMap::new(),
            enum_variants: HashMap::new(),
            expr_types: HashMap::new(),
            path_segment_types: HashMap::new(),
            enum_variant_exprs: HashMap::new(),
            exhaustive_matches: HashSet::new(),
            return_types: Vec::new(),
            errors: Vec::new(),
            file_id,
            watched_vars: HashSet::new(),
        }
    }

    /// Create a new type context with full type resolution info.
    pub fn with_type_info(
        db: &'db dyn Db,
        globals: HashMap<Name, Ty<'db>>,
        class_fields: HashMap<Name, HashMap<Name, Ty<'db>>>,
        type_aliases: HashMap<Name, Ty<'db>>,
        enum_variants: HashMap<Name, Vec<Name>>,
        file_id: FileId,
    ) -> Self {
        TypeContext {
            db,
            scopes: vec![globals],
            class_fields,
            type_aliases,
            enum_variants,
            expr_types: HashMap::new(),
            path_segment_types: HashMap::new(),
            enum_variant_exprs: HashMap::new(),
            exhaustive_matches: HashSet::new(),
            return_types: Vec::new(),
            errors: Vec::new(),
            file_id,
            watched_vars: HashSet::new(),
        }
    }

    /// Record a return type encountered during inference.
    pub fn record_return(&mut self, ty: Ty<'db>, span: Span) {
        self.return_types.push((ty, span));
    }

    /// Look up a type alias definition.
    pub fn lookup_type_alias(&self, name: &Name) -> Option<&Ty<'db>> {
        self.type_aliases.get(name)
    }

    /// Look up enum variants.
    pub fn lookup_enum_variants(&self, name: &Name) -> Option<&Vec<Name>> {
        self.enum_variants.get(name)
    }

    /// Look up a field in a class.
    pub fn lookup_class_field(&self, class_name: &Name, field_name: &Name) -> Option<&Ty<'db>> {
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
    pub fn define(&mut self, name: Name, ty: Ty<'db>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable in the scope chain.
    pub fn lookup(&self, name: &Name) -> Option<&Ty<'db>> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Record the type of an expression.
    pub fn set_expr_type(&mut self, expr: ExprId, ty: Ty<'db>) {
        self.expr_types.insert(expr, ty);
    }

    /// Get the type of an expression.
    #[allow(dead_code)]
    pub fn get_expr_type(&self, expr: ExprId) -> Option<&Ty<'db>> {
        self.expr_types.get(&expr)
    }

    /// Add a type error.
    pub fn push_error(&mut self, error: TypeError<Ty<'db>>) {
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
    param_types: HashMap<Name, Ty<'db>>,
    expected_return: &Ty<'db>,
    globals: Option<HashMap<Name, Ty<'db>>>,
    class_fields: Option<HashMap<Name, HashMap<Name, Ty<'db>>>>,
    type_aliases: Option<HashMap<Name, Ty<'db>>>,
    enum_variants: Option<HashMap<Name, Vec<Name>>>,
    function_loc: FunctionLoc<'db>,
) -> InferenceResult<'db> {
    let file_id = function_loc.file(db).file_id(db);
    let mut ctx = TypeContext::with_type_info(
        db,
        globals.unwrap_or_default(),
        class_fields.unwrap_or_default(),
        type_aliases.unwrap_or_default(),
        enum_variants.unwrap_or_default(),
        file_id,
    );

    // Add parameters to the current scope (on top of globals)
    for (name, ty) in &param_types {
        ctx.define(name.clone(), ty.clone());
    }

    // Type check the body and get the trailing expression type
    let trailing_expr_type = match body {
        FunctionBody::Expr(expr_body) => {
            if let Some(root_expr) = expr_body.root_expr {
                infer_expr(&mut ctx, root_expr, expr_body)
            } else {
                Ty::Void
            }
        }
        FunctionBody::Llm(_) => {
            // LLM functions return their declared return type
            expected_return.clone()
        }
        FunctionBody::Missing => Ty::Unknown,
    };

    // Check all return statement types against expected return type
    for (return_ty, span) in &ctx.return_types {
        if !return_ty.is_subtype_of(expected_return)
            && !return_ty.is_unknown()
            && !expected_return.is_unknown()
        {
            ctx.errors.push(TypeError::TypeMismatch {
                expected: expected_return.clone(),
                found: return_ty.clone(),
                span: *span,
            });
        }
    }

    // Check trailing expression type against expected return type
    // A trailing expression is an implicit return, so it must match
    // BUT only if there are no explicit return statements (those are checked separately)
    if ctx.return_types.is_empty()
        && !trailing_expr_type.is_subtype_of(expected_return)
        && !trailing_expr_type.is_unknown()
        && !expected_return.is_unknown()
    {
        // TODO: we actually want the span of the last expression here.
        let error = TypeError::TypeMismatch {
            expected: expected_return.clone(),
            found: trailing_expr_type.clone(),
            span: Span::default(),
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
    }
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
#[allow(clippy::too_many_arguments)]
pub fn infer_function<'db>(
    db: &'db dyn Db,
    signature: &FunctionSignature,
    body: &FunctionBody,
    globals: Option<HashMap<Name, Ty<'db>>>,
    class_fields: Option<HashMap<Name, HashMap<Name, Ty<'db>>>>,
    type_aliases: Option<HashMap<Name, Ty<'db>>>,
    enum_variants: Option<HashMap<Name, Vec<Name>>>,
    function_loc: FunctionLoc<'db>,
) -> InferenceResult<'db> {
    // Query known type names from the project (Salsa-cached)
    let project = db.project();
    let known_type_names = baml_hir::project_type_names(db, project);
    let known_types: std::collections::HashSet<_> =
        known_type_names.names(db).iter().cloned().collect();

    let file_id = function_loc.file(db).file_id(db);
    // Use a placeholder span for now - ideally we'd have spans on TypeRef
    let placeholder_span = Span::new(file_id, TextRange::empty(0.into()));

    let mut type_errors: Vec<TypeError<Ty<'db>>> = Vec::new();

    // Convert parameter TypeRefs to Tys with validation
    let param_types: HashMap<Name, Ty<'db>> = signature
        .params
        .iter()
        .map(|param| {
            let (ty, errors) =
                lower_type_ref_validated(&param.type_ref, &known_types, placeholder_span);
            type_errors.extend(errors);
            (param.name.clone(), ty)
        })
        .collect();

    // Convert return type with validation
    let (expected_return, errors) =
        lower_type_ref_validated(&signature.return_type, &known_types, placeholder_span);
    type_errors.extend(errors);

    // Delegate to the body inference function
    let mut result = infer_function_body(
        db,
        body,
        param_types,
        &expected_return,
        globals,
        class_fields,
        type_aliases,
        enum_variants,
        function_loc,
    );

    // Prepend type lowering errors to the result
    // (they should appear before type checking errors)
    type_errors.extend(result.errors);
    result.errors = type_errors;

    result
}

/// Infer the type of an expression (synthesize mode).
fn infer_expr<'db>(ctx: &mut TypeContext<'db>, expr_id: ExprId, body: &ExprBody) -> Ty<'db> {
    use baml_hir::Expr;

    let expr = &body.exprs[expr_id];

    // Create a placeholder span for errors (ideally we'd track spans in ExprBody)
    let span = body.get_expr_span(expr_id).unwrap_or_default();

    let ty = match expr {
        Expr::Literal(lit) => infer_literal(lit),

        Expr::Path(segments) => {
            if segments.is_empty() {
                Ty::Unknown
            } else if segments.len() == 1 {
                // Single segment: simple variable lookup
                let name = &segments[0];
                if let Some(ty) = ctx.lookup(name) {
                    ty.clone()
                } else {
                    ctx.push_error(TypeError::UnknownVariable {
                        name: name.to_string(),
                        span,
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
                    // It's a builtin function - return its function type
                    let mut param_types: Vec<Ty<'db>> = Vec::new();
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
                            // This is a valid enum variant - record it and return the enum type
                            ctx.enum_variant_exprs
                                .insert(expr_id, (enum_name.clone(), variant_name.clone()));
                            return Ty::Named(enum_name.clone());
                        }
                        // Enum exists but variant doesn't
                        ctx.push_error(TypeError::UnknownEnumVariant {
                            enum_name: enum_name.to_string(),
                            variant_name: variant_name.to_string(),
                            span,
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
                        span,
                    });
                    return Ty::Unknown;
                };

                // Record segment types for codegen (first segment type, then each field access result)
                let mut segment_types = vec![ty.clone()];

                // Apply field accesses for remaining segments
                for field in &segments[1..] {
                    ty = infer_field_access(ctx, &ty, field, span);
                    segment_types.push(ty.clone());
                }

                // Store segment types for this path expression
                ctx.path_segment_types.insert(expr_id, segment_types);

                ty
            }
        }

        Expr::Binary { lhs, op, rhs } => {
            // Special case: instanceof operator - RHS is a type reference, not an expression
            if *op == baml_hir::BinaryOp::Instanceof {
                let _lhs_ty = infer_expr(ctx, *lhs, body);
                // For instanceof, don't try to resolve RHS as a variable.
                // The RHS is a type name and will be resolved at runtime.
                // Just return bool since instanceof always returns a boolean.
                Ty::Bool
            } else {
                let lhs_ty = infer_expr(ctx, *lhs, body);
                let rhs_ty = infer_expr(ctx, *rhs, body);
                infer_binary_op(ctx, *op, &lhs_ty, &rhs_ty, span)
            }
        }

        Expr::Unary { op, expr: inner } => {
            let inner_ty = infer_expr(ctx, *inner, body);
            infer_unary_op(ctx, *op, &inner_ty, span)
        }

        Expr::Call { callee, args } => {
            // Check if this is a method call (callee is a FieldAccess or multi-segment Path)
            // If so, we need to pass the receiver as the first argument
            let (callee_ty, effective_args) = match &body.exprs[*callee] {
                Expr::FieldAccess { base, field: _ } => {
                    // Method call: receiver.method(args) -> Type.method(receiver, args)
                    // This handles complex expressions like `f().method()` or `arr[0].method()`
                    let receiver_ty = infer_expr(ctx, *base, body);
                    let callee_ty = infer_expr(ctx, *callee, body);

                    // Build effective args: [receiver_type, ...explicit_args]
                    let mut effective_args = vec![receiver_ty];
                    for arg in args {
                        effective_args.push(infer_expr(ctx, *arg, body));
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
                        let arg_types: Vec<Ty<'db>> =
                            args.iter().map(|arg| infer_expr(ctx, *arg, body)).collect();

                        // Build parameter patterns and match against argument types to
                        // extract type variable bindings
                        let mut param_patterns: Vec<&baml_vm::TypePattern> = Vec::new();
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
                        let param_types: Vec<Ty<'db>> = param_patterns
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
                        (callee_ty, arg_types)
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
                                ty = infer_field_access(ctx, &ty, field, span);
                            }
                            ty
                        };

                        let callee_ty = infer_expr(ctx, *callee, body);

                        // Build effective args: [receiver_type, ...explicit_args]
                        let mut effective_args = vec![receiver_ty];
                        for arg in args {
                            effective_args.push(infer_expr(ctx, *arg, body));
                        }
                        (callee_ty, effective_args)
                    }
                }
                _ => {
                    // Regular function call (single-segment Path or other expression)
                    let callee_ty = infer_expr(ctx, *callee, body);
                    let arg_types: Vec<Ty<'db>> =
                        args.iter().map(|arg| infer_expr(ctx, *arg, body)).collect();
                    (callee_ty, arg_types)
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
                            span,
                        });
                    }

                    // Check argument types
                    for (arg_ty, param_ty) in effective_args.iter().zip(params.iter()) {
                        if !arg_ty.is_subtype_of(param_ty) {
                            ctx.push_error(TypeError::TypeMismatch {
                                expected: param_ty.clone(),
                                found: arg_ty.clone(),
                                span, // Ideally we'd have the span of each arg
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
                        span,
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
                                span,
                            });
                        }
                    }
                    _ => {
                        // Not a simple variable (e.g., arr[0].$watch, obj.field.$watch)
                        ctx.push_error(TypeError::WatchOnNonVariable { span });
                    }
                }
            }

            let base_ty = infer_expr(ctx, *base, body);
            infer_field_access(ctx, &base_ty, field, span)
        }

        Expr::Index { base, index } => {
            let base_ty = infer_expr(ctx, *base, body);
            let index_ty = infer_expr(ctx, *index, body);
            infer_index_access(ctx, &base_ty, &index_ty, span)
        }

        Expr::Array { elements } => {
            if elements.is_empty() {
                Ty::List(Box::new(Ty::Unknown))
            } else {
                // Infer element type from first element
                let elem_ty = infer_expr(ctx, elements[0], body);
                // Check all elements have compatible types
                for &elem in &elements[1..] {
                    let other_ty = infer_expr(ctx, elem, body);
                    if !other_ty.is_subtype_of(&elem_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: elem_ty.clone(),
                            found: other_ty,
                            span,
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
                Ty::Named(name.clone())
            } else {
                Ty::Unknown
            };

            // Type check spread expressions - they must be the same type as the object
            for spread in spreads {
                let spread_ty = infer_expr(ctx, spread.expr, body);
                // If we have a named type, verify the spread is compatible
                if !matches!(obj_ty, Ty::Unknown) && !spread_ty.is_subtype_of(&obj_ty) {
                    ctx.push_error(TypeError::TypeMismatch {
                        expected: obj_ty.clone(),
                        found: spread_ty,
                        span,
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
                // Infer key and value types from first entry
                let key_ty = infer_expr(ctx, entries[0].0, body);
                let value_ty = infer_expr(ctx, entries[0].1, body);

                // Check all entries have compatible types
                for &(key, value) in &entries[1..] {
                    let other_key_ty = infer_expr(ctx, key, body);
                    let other_value_ty = infer_expr(ctx, value, body);
                    if !other_key_ty.is_subtype_of(&key_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: key_ty.clone(),
                            found: other_key_ty,
                            span,
                        });
                    }
                    if !other_value_ty.is_subtype_of(&value_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: value_ty.clone(),
                            found: other_value_ty,
                            span,
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
            if !cond_ty.is_subtype_of(&Ty::Bool) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Bool,
                    found: cond_ty,
                    span,
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

            // Use the actual match expression span if available, otherwise fall back to placeholder
            let match_span = body.get_expr_span(expr_id).unwrap_or(span);

            if arms.is_empty() {
                // Empty match is non-exhaustive (unless scrutinee is uninhabited).
                // An uninhabited type has no possible values, so an empty match is
                // actually exhaustive—there are no cases to handle.
                // See `Ty::is_uninhabited()` for the full definition and rationale.
                if !scrutinee_ty.is_uninhabited() {
                    ctx.push_error(TypeError::NonExhaustiveMatch {
                        scrutinee_type: scrutinee_ty.clone(),
                        missing_cases: vec!["all cases".to_string()],
                        span: match_span,
                    });
                }
                Ty::Unknown
            } else {
                // Perform exhaustiveness checking and unreachable arm detection
                check_match_exhaustiveness(ctx, &scrutinee_ty, arms, body, expr_id, match_span);

                // Collect result types from all arms
                let arm_types: Vec<Ty> = arms
                    .iter()
                    .map(|arm| {
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
                            if !guard_ty.is_subtype_of(&Ty::Bool) && !guard_ty.is_unknown() {
                                ctx.push_error(TypeError::TypeMismatch {
                                    expected: Ty::Bool,
                                    found: guard_ty,
                                    span,
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
fn extract_pattern_binding<'db>(
    ctx: &TypeContext<'db>,
    pattern: &Pattern,
    scrutinee_ty: &Ty<'db>,
    _body: &ExprBody,
) -> (Option<Name>, Ty<'db>) {
    match pattern {
        // Typed binding: `s: Success` -> s has type Success
        Pattern::TypedBinding { name, ty } => {
            let narrowed_ty = lower_type_ref(ctx.db(), ty);
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
fn check_match_exhaustiveness<'db>(
    ctx: &mut TypeContext<'db>,
    scrutinee_ty: &Ty<'db>,
    arms: &[baml_hir::MatchArm],
    body: &ExprBody,
    match_expr_id: ExprId,
    match_span: Span,
) {
    // Skip exhaustiveness checking for unknown/error types
    if scrutinee_ty.is_unknown() || scrutinee_ty.is_error() {
        return;
    }

    // Use the new value-based exhaustiveness checker
    let checker = ExhaustivenessChecker::new(ctx.db(), &ctx.enum_variants, &ctx.type_aliases);

    let result = checker.check(scrutinee_ty, arms, body);

    // Get arm spans if available (for accurate error locations)
    let arm_spans = body.get_match_arm_spans(match_expr_id);

    // Report unreachable arms with accurate spans
    for arm_idx in result.unreachable_arms {
        // Use the arm's specific span if available, otherwise fall back to match span
        let span = arm_spans
            .and_then(|spans| spans.get(arm_idx))
            .map(|s| s.arm_span)
            .unwrap_or(match_span);

        ctx.push_error(TypeError::UnreachableArm { span });
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
            span: match_span,
        });
    } else {
        // Record that this match is exhaustive for codegen optimization
        ctx.exhaustive_matches.insert(match_expr_id);
    }
}

/// Infer the type of a literal.
fn infer_literal(lit: &baml_hir::Literal) -> Ty<'static> {
    match lit {
        baml_hir::Literal::Int(_) => Ty::Int,
        baml_hir::Literal::Float(_) => Ty::Float,
        baml_hir::Literal::String(_) => Ty::String,
        baml_hir::Literal::Bool(_) => Ty::Bool,
        baml_hir::Literal::Null => Ty::Null,
    }
}

/// Extract instanceof narrowing info from a condition expression.
///
/// If the condition is `x instanceof Foo`, returns `Some((x, Foo_type))`.
/// Otherwise returns `None`.
fn extract_instanceof_narrowing<'db>(
    _ctx: &TypeContext<'db>,
    condition: ExprId,
    body: &ExprBody,
) -> Option<(Name, Ty<'db>)> {
    use baml_hir::Expr;

    let expr = &body.exprs[condition];

    // Check if this is an instanceof expression
    if let Expr::Binary { op, lhs, rhs } = expr {
        if *op == baml_hir::BinaryOp::Instanceof {
            // LHS should be a simple path (variable name)
            if let Expr::Path(segments) = &body.exprs[*lhs] {
                if segments.len() == 1 {
                    let var_name = segments[0].clone();

                    // RHS should be a simple path (type name)
                    if let Expr::Path(type_segments) = &body.exprs[*rhs] {
                        if type_segments.len() == 1 {
                            let type_name = type_segments[0].clone();
                            // Return the variable name and the narrowed type
                            // We use Ty::Named here since user-defined types are represented this way
                            return Some((var_name, Ty::Named(type_name)));
                        }
                    }
                }
            }
        }
    }

    None
}

/// Infer the result type of a binary operation.
fn infer_binary_op<'db>(
    ctx: &mut TypeContext<'db>,
    op: baml_hir::BinaryOp,
    lhs: &Ty<'db>,
    rhs: &Ty<'db>,
    span: Span,
) -> Ty<'db> {
    use baml_hir::BinaryOp::{
        Add, And, BitAnd, BitOr, BitXor, Div, Eq, Ge, Gt, Instanceof, Le, Lt, Mod, Mul, Ne, Or,
        Shl, Shr, Sub,
    };

    match op {
        // Arithmetic operations (and string concatenation for Add)
        Add => match (lhs, rhs) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) => Ty::Float,
            (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Float, Ty::Int) => Ty::Float,
            // String concatenation
            (Ty::String, Ty::String) => Ty::String,
            _ => {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        },
        Sub | Mul | Div | Mod => match (lhs, rhs) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) => Ty::Float,
            (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Float, Ty::Int) => Ty::Float,
            _ => {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        },

        // Comparison operations
        Eq | Ne => Ty::Bool,

        Lt | Le | Gt | Ge => {
            if (lhs.is_subtype_of(&Ty::Int) || lhs.is_subtype_of(&Ty::Float))
                && (rhs.is_subtype_of(&Ty::Int) || rhs.is_subtype_of(&Ty::Float))
            {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // Logical operations
        And | Or => {
            if lhs.is_subtype_of(&Ty::Bool) && rhs.is_subtype_of(&Ty::Bool) {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // Bitwise operations
        BitAnd | BitOr | BitXor | Shl | Shr => {
            if lhs.is_subtype_of(&Ty::Int) && rhs.is_subtype_of(&Ty::Int) {
                Ty::Int
            } else {
                ctx.push_error(TypeError::InvalidBinaryOp {
                    op: format!("{op:?}"),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span,
                });
                Ty::Error
            }
        }

        // Type checking operations
        Instanceof => Ty::Bool,
    }
}

/// Infer the result type of a unary operation.
fn infer_unary_op<'db>(
    ctx: &mut TypeContext<'db>,
    op: baml_hir::UnaryOp,
    operand: &Ty<'db>,
    span: Span,
) -> Ty<'db> {
    use baml_hir::UnaryOp::{Neg, Not};

    match op {
        Not => {
            if operand.is_subtype_of(&Ty::Bool) {
                Ty::Bool
            } else {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "!".to_string(),
                    operand: operand.clone(),
                    span,
                });
                Ty::Error
            }
        }
        Neg => {
            if operand.is_subtype_of(&Ty::Int) {
                Ty::Int
            } else if operand.is_subtype_of(&Ty::Float) {
                Ty::Float
            } else {
                ctx.push_error(TypeError::InvalidUnaryOp {
                    op: "-".to_string(),
                    operand: operand.clone(),
                    span,
                });
                Ty::Error
            }
        }
    }
}

/// Infer the type of a field access.
///
/// For class types, this handles both field access and method access.
/// For primitive types (arrays, strings, maps), this handles builtin methods.
fn infer_field_access<'db>(
    ctx: &mut TypeContext<'db>,
    base: &Ty<'db>,
    field: &Name,
    span: Span,
) -> Ty<'db> {
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
                    span,
                });
                return Ty::Unknown;
            }
        }
    }

    // First, try class field lookup for named types
    let found_field = match base {
        Ty::Named(class_name) => ctx
            .lookup(field)
            .or(ctx.lookup_class_field(class_name, field))
            .cloned(),
        Ty::Class(class_id) => {
            let class_fields_data = baml_hir::class_fields(ctx.db(), *class_id);
            let fields = class_fields_data.fields(ctx.db());
            fields
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, type_ref)| lower_type_ref(ctx.db(), type_ref))
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
        let mut param_types: Vec<Ty<'db>> = Vec::new();
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
        span,
    });
    Ty::Unknown
}

/// Infer the type of an index access.
fn infer_index_access<'db>(
    ctx: &mut TypeContext<'db>,
    base: &Ty<'db>,
    index: &Ty<'db>,
    span: Span,
) -> Ty<'db> {
    match base {
        Ty::List(elem) => {
            // Index must be int
            if !index.is_subtype_of(&Ty::Int) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int,
                    found: index.clone(),
                    span,
                });
            }
            (**elem).clone()
        }
        Ty::Map { key, value } => {
            // Index must match key type
            if !index.is_subtype_of(key) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: (**key).clone(),
                    found: index.clone(),
                    span,
                });
            }
            (**value).clone()
        }
        Ty::String => {
            // String indexing returns a character (string of length 1)
            if !index.is_subtype_of(&Ty::Int) {
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Int,
                    found: index.clone(),
                    span,
                });
            }
            Ty::String
        }
        Ty::Unknown => Ty::Unknown,
        _ => {
            ctx.push_error(TypeError::NotIndexable {
                ty: base.clone(),
                span,
            });
            Ty::Unknown
        }
    }
}

/// Type check a statement.
fn check_stmt(ctx: &mut TypeContext<'_>, stmt_id: StmtId, body: &ExprBody) {
    use baml_hir::Stmt;

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
                let init_ty = infer_expr(ctx, *init, body);

                // If there's a type annotation, check it matches
                if let Some(annot) = type_annotation {
                    // TODO: currently type_span and type_annotations are separate `Option`s
                    // turn it into one tuple.
                    // this unwrap is safe because if type_ann is populated, so is type_span
                    let span = ctx.build_span_default(type_span);
                    let annot_ty = lower_type_ref(ctx.db(), annot);
                    if !init_ty.is_subtype_of(&annot_ty) {
                        ctx.push_error(TypeError::TypeMismatch {
                            expected: annot_ty.clone(),
                            found: init_ty,
                            span,
                        });
                    }
                    annot_ty
                } else {
                    init_ty
                }
            } else if let Some(annot) = type_annotation {
                lower_type_ref(ctx.db(), annot)
            } else {
                Ty::Unknown
            };

            // Extract variable name from pattern and track watched status
            let pat = &body.patterns[*pattern];
            match pat {
                Pattern::Binding(name) => {
                    ctx.define(name.clone(), ty);
                    if *is_watched {
                        ctx.mark_watched(name.clone());
                    }
                }
                Pattern::TypedBinding { name, ty: _ } => {
                    // TODO: Check declared type matches inferred type
                    ctx.define(name.clone(), ty);
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
            let span = Span::new(
                baml_base::FileId::new(0),
                text_size::TextRange::empty(0.into()),
            );
            let return_ty = if let Some(e) = expr {
                infer_expr(ctx, *e, body)
            } else {
                Ty::Void
            };
            ctx.record_return(return_ty, span);
        }

        Stmt::While {
            condition,
            body: while_body,
            after,
            origin: _, // origin is used for diagnostics, not type checking
        } => {
            let cond_ty = infer_expr(ctx, *condition, body);
            if !cond_ty.is_subtype_of(&Ty::Bool) {
                let span = body.get_expr_span(*condition).unwrap_or_default();
                ctx.push_error(TypeError::TypeMismatch {
                    expected: Ty::Bool,
                    found: cond_ty,
                    span,
                });
            }
            infer_expr(ctx, *while_body, body);
            // Type-check the after statement (for desugared C-style for loops)
            if let Some(after_stmt) = after {
                check_stmt(ctx, *after_stmt, body);
            }
        }

        Stmt::Break | Stmt::Continue => {
            // These are control flow statements with no expressions to type-check.
            // Loop context validation could be added here in the future.
        }

        Stmt::Assign { target, value } => {
            // Type-check both the target and value expressions
            infer_expr(ctx, *target, body);
            infer_expr(ctx, *value, body);
            // TODO: Check that target is assignable (variable or field access)
            // TODO: Check that value type is compatible with target type
        }

        Stmt::AssignOp {
            target,
            op: _,
            value,
        } => {
            // Type-check both the target and value expressions
            infer_expr(ctx, *target, body);
            infer_expr(ctx, *value, body);
            // TODO: Check that target is assignable
            // TODO: Check that the operation is valid for the types
        }

        Stmt::Missing => {}

        Stmt::HeaderComment { .. } => {
            // Header comments don't need type checking - they're just annotations
        }
    }
}
