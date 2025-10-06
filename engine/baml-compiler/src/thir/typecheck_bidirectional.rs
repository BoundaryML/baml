/// Bidirectional Typechecking for the BAML language.
///
/// This module implements bidirectional typing as described in "Bidirectional Typing"
/// by Jana Dunfield and Neelakantan Krishnaswami (arXiv:1908.05839).
///
/// # Overview of Bidirectional Typing
///
/// Bidirectional typing combines two modes of typing:
/// - **Type Checking** (⇐): Checks that a program satisfies a known type
/// - **Type Synthesis** (⇒): Determines a type from the program
///
/// Using checking enables bidirectional typing to support features for which inference
/// is undecidable; using synthesis enables bidirectional typing to avoid the large
/// annotation burden of explicitly typed languages.
///
/// # Key Principles
///
/// ## The Pfenning Recipe
///
/// We follow a modified version of the "Pfenning recipe" for bidirectional typing:
///
/// 1. **Introduction rules check**: Terms that introduce a type connective (like
///    lambda abstractions for functions) have checking conclusions.
///    - `λx. e ⇐ A → B` checks the lambda against a function type
///
/// 2. **Elimination rules synthesize**: Terms that eliminate a type connective (like
///    function applications) have synthesis conclusions in their principal judgment.
///    - `f e ⇒ B` synthesizes the result type of an application
///
/// 3. **Variables synthesize**: Looking up a variable in the context synthesizes its type.
///    - `x ⇒ A` when `x: A` is in the context
///
/// 4. **Subsumption switches modes**: A subsumption rule allows switching from synthesis
///    to checking by verifying type compatibility.
///    - If `e ⇒ A` and `A <: B`, then `e ⇐ B`
///
/// ## Information Flow
///
/// - **Checking judgment** `Γ ⊢ e ⇐ A`:
///   - Inputs: context Γ, expression e, type A
///   - Output: typed expression (THIR) or type error
///   - The type A is known and guides typechecking
///
/// - **Synthesis judgment** `Γ ⊢ e ⇒ A`:
///   - Inputs: context Γ, expression e
///   - Outputs: typed expression (THIR) and synthesized type A
///   - The type A is discovered during typechecking
///
/// # Mode Correctness
///
/// A rule is mode-correct if there exists a strategy for deriving premises such that:
/// 1. Every input to each premise is known from earlier premises or the conclusion
/// 2. All outputs of the conclusion are known after deriving all premises
///
/// # Annotation Character
///
/// Good bidirectional systems have predictable annotation requirements:
/// - Annotations are needed primarily at "introduction meets elimination" boundaries
/// - For example: `(λx. e) arg` needs an annotation because the lambda (intro) is
///   immediately applied (elim)
/// - But `f (λx. e)` may not need an annotation if `f`'s type tells us what type
///   the lambda should have
///
/// # Subformula Property
///
/// Types that appear in a derivation are either:
/// - Subformulas of types in the conclusion (from annotations or context)
/// - "Obvious" types of literals (like `true: bool`, `42: int`)
///
/// This property ensures that problematic type connectives only appear when explicitly
/// requested by the programmer through annotations.
///
/// # BAML-Specific Considerations
///
/// BAML has several features that affect bidirectional typing:
///
/// ## Subtyping
/// BAML supports subtyping (e.g., nullable types, union types), which interacts
/// with the subsumption rule. The subsumption rule becomes:
///   `Γ ⊢ e ⇒ A    A <: B`
///   `─────────────────────`
///   `    Γ ⊢ e ⇐ B      `
///
/// ## Mutable Variables
/// BAML has mutable variables, which requires tracking whether variables are
/// mutable in the context.
///
/// ## Method Calls
/// Method calls like `obj.method(args)` need special handling:
/// - First synthesize the type of `obj`
/// - Look up the method in that type
/// - Check arguments against method parameter types
///
/// ## Class Constructors
/// Class constructors introduce class types, so they check:
///   `Γ ⊢ field_i ⇐ A_i` for each field
///   `──────────────────────────────────`
///   `Γ ⊢ new Class { fields } ⇐ Class`
///
/// ## Polymorphism and Generic Functions
///
/// BAML supports polymorphic functions like `baml.deep_copy<T>(T) -> T` and
/// `baml.fetch_as<T>(Request) -> T`. Bidirectional typing handles polymorphism
/// through **greedy instantiation** and **checking-guided synthesis**.
///
/// ### Two Approaches to Polymorphism
///
/// There are two main approaches to polymorphism in bidirectional typing:
///
/// 1. **Explicit type arguments** (predicative, like Java):
///    ```
///    result = baml.fetch_as<User>(request)  // Type argument explicit
///    ```
///
/// 2. **Implicit type arguments** (higher-rank, like Haskell/ML):
///    ```
///    result = baml.fetch_as(request)  // Type inferred from context
///    ```
///
/// BAML supports both! When type arguments are omitted, we use the expected type
/// from the checking context to instantiate the polymorphic function.
///
/// ### The Instantiation Problem
///
/// When we synthesize the type of a polymorphic function, we face a problem:
/// ```
/// Γ ⊢ baml.deep_copy ⇒ ∀T. T → T
/// ```
///
/// But to apply this function, we need a concrete type! The instantiation `τ` for
/// `T` is not an input to the synthesis judgment, so we can't synthesize it directly.
///
/// ### Solution: Greedy Instantiation from Checking Context
///
/// Following Dunfield & Krishnaswami (2013), we use "greedy instantiation":
///
/// When checking a polymorphic function call against an expected type, we:
/// 1. Extract the expected result type from the checking context
/// 2. Use that to instantiate the function's type parameter(s)
/// 3. Check arguments against the instantiated parameter types
///
/// **Example:**
/// ```baml
/// function process_user(u: User) -> string { ... }
///
/// // Explicit type arguments
/// x: User = baml.fetch_as<User>(req)  // T = User from explicit <User>
///
/// // Implicit inference from expected type (checking context)
/// y: User = baml.fetch_as(req)         // T = User inferred from ⇐ User
/// result = process_user(baml.fetch_as(req))  // T = User from parameter type
///
/// // Type argument inferred from return position
/// z: string = baml.unstable.to_string(user)  // returns string
/// ```
///
/// ### Typing Rules for Polymorphic Functions
///
/// **Universal elimination (application) - Checking mode:**
/// ```text
/// Γ ⊢ f ⇒ ∀α. A → B    [τ/α]A = A'    Γ ⊢ arg ⇐ A'    [τ/α]B = B'    B' = Expected
/// ────────────────────────────────────────────────────────────────────────────────
///                        Γ ⊢ f arg ⇐ Expected
/// ```
///
/// When we check `f arg` against an expected type, we:
/// 1. Synthesize `f`'s type (which may be polymorphic)
/// 2. If `f : ∀α. A → B`, find `τ` such that `[τ/α]B` matches `Expected`
/// 3. Check `arg` against `[τ/α]A`
/// 4. Result has type `Expected`
///
/// **Universal elimination - Synthesis mode (requires explicit instantiation):**
/// ```text
/// Γ ⊢ f ⇒ ∀α. A → B    τ explicit    Γ ⊢ arg ⇐ [τ/α]A
/// ──────────────────────────────────────────────────────
///              Γ ⊢ f<τ> arg ⇒ [τ/α]B
/// ```
///
/// When explicit type arguments are provided, we:
/// 1. Synthesize `f`'s type
/// 2. Use the explicit `τ` to instantiate the type parameter
/// 3. Check `arg` against the instantiated parameter type
/// 4. Synthesize the instantiated result type
///
/// ### Predicative vs. Impredicative Polymorphism
///
/// BAML uses **predicative polymorphism**: type variables can only be instantiated
/// with monotypes (non-polymorphic types). This means:
///
/// ✓ Valid: `baml.deep_copy<User>(user)` where `User` is a class
/// ✓ Valid: `baml.deep_copy<int[]>(array)` where `int[]` is an array type
/// ✗ Invalid: `baml.deep_copy<∀T. T → T>(fn)` where `∀T. T → T` is polymorphic
///
/// This restriction makes type inference decidable and avoids the need for
/// backtracking during type checking.
///
/// ### Constrained Polymorphism
///
/// Some polymorphic functions have constraints on their type parameters.
/// For example, a generic comparison might require:
/// ```
/// function compare<T: Comparable>(a: T, b: T) -> int
/// ```
///
/// In BAML's current design, constraints are checked when instantiating:
/// 1. When we instantiate `T` with a concrete type `τ`
/// 2. We verify that `τ` satisfies all constraints on `T`
/// 3. If not, report a type error
///
/// ### Implementation Strategy
///
/// Our implementation handles polymorphism through:
///
/// 1. **Type schemes in the context**: Store polymorphic functions as `∀α. τ`
/// 2. **Instantiation during application**: When typechecking a call, instantiate
///    the type scheme with concrete types
/// 3. **Greedy instantiation**: Use the first plausible instantiation based on
///    the checking context
/// 4. **Explicit type arguments**: Support `f<T>` syntax for explicit instantiation
///
/// This approach avoids the complexity of unification variables and constraint
/// solving, making the typechecker predictable and efficient.
///
/// ### Why This Works for BAML
///
/// The greedy instantiation approach works well because:
/// - BAML's polymorphic functions have clear, simple type signatures
/// - Type arguments can usually be inferred from the expected type
/// - When ambiguous, programmers can provide explicit type arguments
/// - No higher-rank polymorphism simplifies the inference problem
///
/// # Implementation Notes
///
/// The implementation uses:
/// - `TypeContext` to track symbols, classes, enums, and mutable variables
/// - `check_expr` for the checking judgment (⇐)
/// - `synth_expr` for the synthesis judgment (⇒)
/// - `subsume` for the mode-switching subsumption rule
/// - `instantiate_polymorphic_type` to handle universal quantifiers
/// - Helper functions for specific expression forms

use std::{borrow::Cow, sync::Arc};

use baml_types::{
    ir_type::{ArrowGeneric, TypeIR},
    BamlMap, BamlMediaType, BamlValueWithMeta, TypeValue,
};
use internal_baml_ast::ast::WithSpan;
use internal_baml_diagnostics::{DatamodelError, Diagnostics, Span};

use crate::{
    emit::{EmitSpec, EmitWhen},
    hir::{self, dump::TypeDocumentRender, BinaryOperator, Hir},
    thir::{self as thir, ExprMetadata, THir},
};

/// Metadata for typed expressions.
/// Contains the span and optional type information.
type IRMeta = (Span, Option<TypeIR>);

/// Information about a mutable variable.
#[derive(Debug, Clone)]
pub struct MutableVarInfo {
    pub ty: TypeIR,
    pub span: Span,
}

/// Information about an immutable variable.
#[derive(Debug, Clone)]
pub struct VarInfo {
    pub ty: TypeIR,
}

// ============================================================================
// POLYMORPHISM SUPPORT - TYPE SCHEMES
// ============================================================================

/// Represents a polymorphic type scheme with quantified type variables.
///
/// In the literature, this is written as `∀α. τ` or `∀α₁...αₙ. τ`.
///
/// **IMPORTANT LIMITATION**: BAML's current type system (`TypeIR`) does not yet
/// support generic type variables. This means we cannot directly represent
/// polymorphic types like `∀T. T -> T` in the type system.
///
/// For now, `TypeScheme` is a placeholder that will wrap monomorphic types.
/// Full polymorphism support requires:
/// 1. Adding `TypeIR::Generic(String)` variant to represent type variables
/// 2. Implementing type substitution [τ/α] for all TypeIR variants
/// 3. Adding type parameter tracking to ArrowGeneric
///
/// Until then, this serves as documentation of the intended design.
///
/// Example (future): `baml.deep_copy<T>(T) -> T` would be represented as:
/// ```
/// TypeScheme {
///     type_params: vec!["T"],
///     body: Arrow([Generic("T")], Generic("T"))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TypeScheme {
    /// Type parameters (e.g., ["T"], ["K", "V"])
    /// Currently unused - will be populated when TypeIR supports generics
    pub type_params: Vec<String>,
    /// The body of the type scheme
    /// For now, this will always be a monomorphic type
    pub body: TypeIR,
}

impl TypeScheme {
    /// Create a monomorphic type scheme (no type parameters)
    pub fn mono(ty: TypeIR) -> Self {
        TypeScheme {
            type_params: Vec::new(),
            body: ty,
        }
    }

    /// Create a polymorphic type scheme (placeholder for future implementation)
    ///
    /// **Note**: Until TypeIR supports generic type variables, this will simply
    /// store the body type without actual polymorphism.
    pub fn poly(_type_params: Vec<String>, body: TypeIR) -> Self {
        // TODO: When TypeIR supports generics, store type_params properly
        TypeScheme {
            type_params: Vec::new(), // Ignored for now
            body,
        }
    }

    /// Check if this is a monomorphic type (no type parameters)
    pub fn is_mono(&self) -> bool {
        // For now, everything is monomorphic since we don't support generics yet
        true
    }

    /// Get the underlying type.
    ///
    /// Since we don't support polymorphism yet, this just returns the body.
    pub fn as_type(&self) -> &TypeIR {
        &self.body
    }
}

/// The typing context for bidirectional typechecking.
///
/// This context is threaded through the typechecking algorithm and maintains:
/// - Symbol table mapping names to types (functions, etc.)
/// - Available classes and enums
/// - Mutable and immutable variable bindings
///
/// In a more sophisticated bidirectional system (like with higher-rank polymorphism),
/// this would be an "ordered context" that tracks existential variable solutions.
/// For BAML, we keep a simpler structure.
#[derive(Debug, Clone)]
pub struct TypeContext<'func> {
    /// Symbol table: maps names to type schemes (monomorphic or polymorphic functions)
    pub symbols: BamlMap<String, TypeScheme>,

    /// Available classes
    pub classes: BamlMap<String, hir::Class>,

    /// Available enums
    pub enums: BamlMap<String, hir::Enum>,

    /// Mutable variables in scope
    pub mutable_vars: BamlMap<String, MutableVarInfo>,

    /// Immutable variables in scope
    pub vars: BamlMap<String, VarInfo>,

    /// Reference to the HIR (for looking up definitions)
    pub hir: Option<&'func Hir>,
}

impl Default for TypeContext<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'func> TypeContext<'func> {
    /// Create a new empty typing context.
    pub fn new() -> Self {
        TypeContext {
            symbols: BamlMap::new(),
            classes: BamlMap::new(),
            enums: BamlMap::new(),
            mutable_vars: BamlMap::new(),
            vars: BamlMap::new(),
            hir: None,
        }
    }

    /// Look up a variable (mutable or immutable) in the context.
    pub fn lookup_var(&self, name: &str) -> Option<TypeIR> {
        self.mutable_vars
            .get(name)
            .map(|info| info.ty.clone())
            .or_else(|| self.vars.get(name).map(|info| info.ty.clone()))
    }

    /// Add an immutable variable to the context.
    pub fn add_var(&mut self, name: String, ty: TypeIR) {
        self.vars.insert(name, VarInfo { ty });
    }

    /// Add a mutable variable to the context.
    pub fn add_mutable_var(&mut self, name: String, ty: TypeIR, span: Span) {
        self.mutable_vars.insert(name, MutableVarInfo { ty, span });
    }

    /// Create a new context with an additional immutable variable.
    /// This is useful for let-bindings and lambda parameters.
    pub fn with_var(&self, name: String, ty: TypeIR) -> Self {
        let mut new_ctx = self.clone();
        new_ctx.add_var(name, ty);
        new_ctx
    }
}

/// Entry point for bidirectional typechecking.
///
/// Converts HIR to THIR while collecting type errors.
pub fn typecheck(hir: &Hir, diagnostics: &mut Diagnostics) -> THir<ExprMetadata> {
    let (thir, _) = typecheck_returning_context(hir, diagnostics);
    thir
}

/// Convert HIR to THIR while collecting type errors and returning the final context.
pub fn typecheck_returning_context<'a>(
    hir: &'a Hir,
    diagnostics: &mut Diagnostics,
) -> (THir<ExprMetadata>, TypeContext<'a>) {
    // TODO: Initialize context with all classes, enums, functions, etc.
    // This will be ported from the original typecheck.rs

    let hir_classes: BamlMap<String, hir::Class> = hir
        .classes
        .clone()
        .into_iter()
        .map(|c| (c.name.clone(), c))
        .collect();

    let hir_enums: BamlMap<String, hir::Enum> = hir
        .enums
        .clone()
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect();

    let mut typing_context = TypeContext::new();
    typing_context.hir = Some(hir);
    typing_context.classes.extend(hir_classes.clone());
    typing_context.enums.extend(hir_enums.clone());

    // TODO: Add all functions, methods, and builtins to the context
    // (to be ported from original typecheck.rs)

    // Typecheck all functions
    let thir_functions = Vec::new(); // TODO: typecheck functions

    // Convert HIR classes to THIR classes (for now, just placeholder conversion)
    let thir_classes: BamlMap<String, thir::Class<ExprMetadata>> = hir_classes
        .into_iter()
        .map(|(name, c)| {
            (
                name,
                thir::Class {
                    name: c.name,
                    fields: c.fields,
                    methods: Vec::new(), // TODO: typecheck methods
                    span: c.span,
                },
            )
        })
        .collect();

    // Convert HIR enums to THIR enums
    let thir_enums: BamlMap<String, thir::Enum> = hir_enums
        .into_iter()
        .map(|(name, e)| {
            // We need to create a TypeIR for the enum
            let ty = TypeIR::Enum {
                name: e.name.clone(),
                dynamic: false, // TODO: Get this from enum definition
                meta: Default::default(),
            };
            (
                name,
                thir::Enum {
                    name: e.name,
                    variants: e.variants,
                    span: e.span,
                    ty,
                },
            )
        })
        .collect();

    let thir = THir {
        classes: thir_classes,
        enums: thir_enums,
        expr_functions: thir_functions,
        llm_functions: Vec::new(), // TODO
        global_assignments: BamlMap::new(), // TODO: typecheck global assignments
    };

    (thir, typing_context)
}

// ============================================================================
// CORE BIDIRECTIONAL TYPING JUDGMENTS
// ============================================================================

/// **Synthesis judgment** `Γ ⊢ e ⇒ A`
///
/// Synthesizes (infers) the type of an expression.
///
/// # Mode
/// - Input: context Γ, expression e
/// - Output: typed expression (THIR) and type A
///
/// # Rules
///
/// The synthesis judgment follows these principles:
/// - Variables synthesize their type from the context
/// - Elimination forms (applications, array/field access) synthesize
/// - Literals synthesize their obvious type
/// - When an introduction form must synthesize, we require an annotation
fn synth_expr(
    ctx: &TypeContext,
    expr: &hir::Expression,
    diagnostics: &mut Diagnostics,
) -> (thir::Expr<IRMeta>, Option<TypeIR>) {
    use hir::Expression::*;

    match expr {
        // Variables synthesize their type from the context
        // Γ, x: A ⊢ x ⇒ A
        Identifier(name, span) => {
            // First check if it's a local variable
            if let Some(ty) = ctx.lookup_var(name) {
                (
                    thir::Expr::Var(name.clone(), (span.clone(), Some(ty.clone()))),
                    Some(ty),
                )
            }
            // Then check if it's a function/symbol in the symbol table
            else if let Some(type_scheme) = ctx.symbols.get(name) {
                // If it's a monomorphic function, we can synthesize its type directly
                // If it's polymorphic, we need explicit type arguments (handled by synth_call)
                if type_scheme.is_mono() {
                    let ty = type_scheme.body.clone();
                    (
                        thir::Expr::Var(name.clone(), (span.clone(), Some(ty.clone()))),
                        Some(ty),
                    )
                } else {
                    // Polymorphic identifier without application - cannot synthesize
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "Cannot infer type arguments for polymorphic function `{}`. \
                             Try applying the function with explicit type arguments: {}<T>(...)",
                            name, name
                        ),
                        span.clone(),
                    ));
                    (
                        thir::Expr::Var(name.clone(), (span.clone(), None)),
                        None,
                    )
                }
            } else {
                // Unknown variable - report error
                diagnostics.push_error(DatamodelError::new_validation_error(
                    &format!("Unknown variable or function `{}`", name),
                    span.clone(),
                ));
                (
                    thir::Expr::Var(name.clone(), (span.clone(), None)),
                    None,
                )
            }
        }

        // Literals synthesize their obvious type
        BoolValue(value, span) => {
            let ty = TypeIR::bool();
            (
                thir::Expr::Value(BamlValueWithMeta::Bool(
                    *value,
                    (span.clone(), Some(ty.clone())),
                )),
                Some(ty),
            )
        }

        StringValue(value, span) => {
            let ty = TypeIR::string();
            (
                thir::Expr::Value(BamlValueWithMeta::String(
                    value.clone(),
                    (span.clone(), Some(ty.clone())),
                )),
                Some(ty),
            )
        }

        NumericValue(value, span) => {
            // Try to parse as int first, then float
            if let Ok(i) = value.parse::<i64>() {
                let ty = TypeIR::int();
                (
                    thir::Expr::Value(BamlValueWithMeta::Int(
                        i,
                        (span.clone(), Some(ty.clone())),
                    )),
                    Some(ty),
                )
            } else if let Ok(f) = value.parse::<f64>() {
                let ty = TypeIR::float();
                (
                    thir::Expr::Value(BamlValueWithMeta::Float(
                        f,
                        (span.clone(), Some(ty.clone())),
                    )),
                    Some(ty),
                )
            } else {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    &format!("Invalid numeric literal: {}", value),
                    span.clone(),
                ));
                (
                    thir::Expr::Value(BamlValueWithMeta::Null((span.clone(), None))),
                    None,
                )
            }
        }

        // Function application synthesizes the result type
        // Γ ⊢ f ⇒ A → B    Γ ⊢ arg ⇐ A
        // ─────────────────────────────
        //      Γ ⊢ f arg ⇒ B
        Call { function, type_args, args, span } => {
            synth_call(ctx, function, type_args, args, span, diagnostics)
        }

        // TODO: Implement other synthesis cases
        _ => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("Cannot synthesize type for expression: {:?}", expr),
                expr.span(),
            ));
            (
                thir::Expr::Value(BamlValueWithMeta::Null((expr.span(), None))),
                None,
            )
        }
    }
}

/// **Checking judgment** `Γ ⊢ e ⇐ A`
///
/// Checks that an expression has the expected type.
///
/// # Mode
/// - Input: context Γ, expression e, expected type A
/// - Output: typed expression (THIR) or type error
///
/// # Rules
///
/// The checking judgment follows these principles:
/// - Introduction forms (lambdas, class constructors) check
/// - When checking, we can use the expected type to guide typechecking
/// - Subsumption allows using synthesis: if `e ⇒ A` and `A <: B`, then `e ⇐ B`
fn check_expr(
    ctx: &TypeContext,
    expr: &hir::Expression,
    expected: &TypeIR,
    diagnostics: &mut Diagnostics,
) -> thir::Expr<IRMeta> {
    use hir::Expression::*;

    match expr {
        // Lambdas check against function types
        // (BAML doesn't have lambdas yet, but this shows the pattern)

        // Class constructors check against class types
        ClassConstructor(constructor, span) => {
            check_class_constructor(ctx, constructor, expected, span, diagnostics)
        }

        // For most expressions, we use subsumption:
        // synthesize the type, then check compatibility
        _ => {
            subsume(ctx, expr, expected, diagnostics)
        }
    }
}

/// **Subsumption rule** - switches from synthesis to checking
///
/// This is the key rule that allows us to use synthesis in a checking context:
///
/// ```text
///   Γ ⊢ e ⇒ A    A <: B
///   ──────────────────────
///       Γ ⊢ e ⇐ B
/// ```
///
/// # Mode Switching
///
/// Subsumption is what makes bidirectional typing "bidirectional":
/// - It allows us to synthesize a type when checking is required
/// - It verifies that the synthesized type is compatible with the expected type
/// - In systems with subtyping, this is where subtyping is checked
fn subsume(
    ctx: &TypeContext,
    expr: &hir::Expression,
    expected: &TypeIR,
    diagnostics: &mut Diagnostics,
) -> thir::Expr<IRMeta> {
    // Synthesize the type
    let (typed_expr, synthesized) = synth_expr(ctx, expr, diagnostics);

    // Check compatibility
    if let Some(actual) = synthesized {
        if !types_compatible(&actual, expected) {
            diagnostics.push_error(DatamodelError::new_type_mismatch_error(
                &expected.diagnostic_repr().to_string(),
                &actual.diagnostic_repr().to_string(),
                "", // TODO: Pass the actual source text
                expr.span(),
            ));
        }
    }

    // Return the typed expression with the expected type
    // (we override the synthesized type with the expected type)
    update_expr_type(typed_expr, expected.clone())
}

// ============================================================================
// HELPER FUNCTIONS FOR SPECIFIC EXPRESSION FORMS
// ============================================================================

/// Helper for synthesizing the type of a function call.
///
/// This function handles both monomorphic and polymorphic function calls.
/// For polymorphic functions, it supports:
/// - Explicit type arguments: `f<User>(arg)`
/// - Implicit type inference: `f(arg)` (but synthesis requires explicit args)
///
/// # Polymorphic Functions
///
/// When calling a polymorphic function in synthesis mode, explicit type arguments
/// are required. For example:
/// ```baml
/// result = baml.fetch_as<User>(request)  // OK: explicit type argument
/// ```
///
/// If type arguments are omitted, the function cannot be synthesized without
/// a checking context. This is a fundamental limitation of bidirectional typing.
fn synth_call(
    ctx: &TypeContext,
    function: &hir::Expression,
    type_args: &[hir::TypeArg],
    args: &[hir::Expression],
    span: &Span,
    diagnostics: &mut Diagnostics,
) -> (thir::Expr<IRMeta>, Option<TypeIR>) {
    // Special handling for identifiers that resolve to polymorphic functions
    if let hir::Expression::Identifier(func_name, func_span) = function {
        if let Some(type_scheme) = ctx.symbols.get(func_name) {
            return synth_call_with_scheme(
                ctx,
                func_name,
                func_span,
                type_scheme,
                type_args,
                args,
                span,
                None, // No expected type in synthesis mode
                diagnostics,
            );
        }
    }

    // Fallback: synthesize the function expression normally
    let (typed_func, func_ty) = synth_expr(ctx, function, diagnostics);

    let Some(func_ty) = func_ty else {
        // If function type is unknown, we can't synthesize the call type
        return (
            thir::Expr::Call {
                func: Arc::new(typed_func),
                type_args: Vec::new(),  // TODO: Handle type args when supported
                args: Vec::new(),
                meta: (span.clone(), None),
            },
            None,
        );
    };

    // Extract arrow type (A1, ..., An) -> R
    let arrow = match &func_ty {
        TypeIR::Arrow(arrow, _) => arrow,
        _ => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("Expected function type, got {}", func_ty.diagnostic_repr()),
                span.clone(),
            ));
            return (
                thir::Expr::Call {
                    func: Arc::new(typed_func),
                    type_args: Vec::new(),
                    args: Vec::new(),
                    meta: (span.clone(), None),
                },
                None,
            );
        }
    };

    // Check arity
    if args.len() != arrow.param_types.len() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            &format!(
                "Function expects {} arguments, got {}",
                arrow.param_types.len(),
                args.len()
            ),
            span.clone(),
        ));
    }

    // Check each argument against its expected type
    let typed_args: Vec<_> = args
        .iter()
        .zip(arrow.param_types.iter())
        .map(|(arg, expected_ty)| check_expr(ctx, arg, expected_ty, diagnostics))
        .collect();

    let result_ty = arrow.return_type.clone();

    (
        thir::Expr::Call {
            func: Arc::new(typed_func),
            type_args: Vec::new(),  // TODO: Handle type args when supported
            args: typed_args,
            meta: (span.clone(), Some(result_ty.clone())),
        },
        Some(result_ty),
    )
}

/// Helper for typechecking a call to a function with a known type scheme.
///
/// For now, this is simplified since BAML doesn't yet support generic type variables.
/// All type schemes are treated as monomorphic types.
///
/// # Arguments
/// - `func_name`: Name of the function (for better error messages)
/// - `func_span`: Span of the function identifier
/// - `type_scheme`: The type scheme of the function
/// - `hir_type_args`: Explicit type arguments from the call site (currently unsupported)
/// - `args`: Arguments to the function
/// - `span`: Span of the entire call
/// - `_expected_type`: Expected return type (unused for now, will be used for greedy instantiation)
///
/// # Returns
/// A typed expression and the result type (if synthesis succeeded)
fn synth_call_with_scheme(
    ctx: &TypeContext,
    func_name: &str,
    func_span: &Span,
    type_scheme: &TypeScheme,
    hir_type_args: &[hir::TypeArg],
    args: &[hir::Expression],
    span: &Span,
    _expected_type: Option<&TypeIR>,
    diagnostics: &mut Diagnostics,
) -> (thir::Expr<IRMeta>, Option<TypeIR>) {
    // Warn if type arguments are provided (not yet supported)
    if !hir_type_args.is_empty() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "Type arguments are not yet supported in BAML",
            span.clone(),
        ));
    }

    // Get the function type from the scheme
    let func_ty = type_scheme.as_type();

    // Extract arrow type
    let arrow = match func_ty {
        TypeIR::Arrow(arrow, _) => arrow,
        _ => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("Expected function type, got {}", func_ty.diagnostic_repr()),
                span.clone(),
            ));
            return (
                thir::Expr::Call {
                    func: Arc::new(thir::Expr::Var(
                        func_name.to_string(),
                        (func_span.clone(), None),
                    )),
                    type_args: Vec::new(),
                    args: Vec::new(),
                    meta: (span.clone(), None),
                },
                None,
            );
        }
    };

    // Check arity
    if args.len() != arrow.param_types.len() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            &format!(
                "Function `{}` expects {} arguments, got {}",
                func_name,
                arrow.param_types.len(),
                args.len()
            ),
            span.clone(),
        ));
    }

    // Check each argument against its expected type
    let typed_args: Vec<_> = args
        .iter()
        .zip(arrow.param_types.iter())
        .map(|(arg, expected_ty)| check_expr(ctx, arg, expected_ty, diagnostics))
        .collect();

    let result_ty = arrow.return_type.clone();

    (
        thir::Expr::Call {
            func: Arc::new(thir::Expr::Var(
                func_name.to_string(),
                (func_span.clone(), Some(func_ty.clone())),
            )),
            type_args: Vec::new(),
            args: typed_args,
            meta: (span.clone(), Some(result_ty.clone())),
        },
        Some(result_ty),
    )
}

/// Helper for checking class constructors.
fn check_class_constructor(
    ctx: &TypeContext,
    constructor: &hir::ClassConstructor,
    expected: &TypeIR,
    span: &Span,
    diagnostics: &mut Diagnostics,
) -> thir::Expr<IRMeta> {
    // TODO: Implement class constructor checking
    // This should:
    // 1. Verify that expected is a class type
    // 2. Look up the class definition
    // 3. Check each field against its declared type
    // 4. Ensure all required fields are present

    diagnostics.push_error(DatamodelError::new_validation_error(
        "Class constructor checking not yet implemented in bidirectional typechecker",
        span.clone(),
    ));

    thir::Expr::Value(BamlValueWithMeta::Null((span.clone(), None)))
}


// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Update the type annotation in an expression.
fn update_expr_type(expr: thir::Expr<IRMeta>, new_type: TypeIR) -> thir::Expr<IRMeta> {
    match expr {
        thir::Expr::Var(name, (span, _)) => {
            thir::Expr::Var(name, (span, Some(new_type)))
        }
        thir::Expr::Value(BamlValueWithMeta::Bool(v, (span, _))) => {
            thir::Expr::Value(BamlValueWithMeta::Bool(v, (span, Some(new_type))))
        }
        thir::Expr::Value(BamlValueWithMeta::String(v, (span, _))) => {
            thir::Expr::Value(BamlValueWithMeta::String(v, (span, Some(new_type))))
        }
        thir::Expr::Value(BamlValueWithMeta::Int(v, (span, _))) => {
            thir::Expr::Value(BamlValueWithMeta::Int(v, (span, Some(new_type))))
        }
        thir::Expr::Value(BamlValueWithMeta::Float(v, (span, _))) => {
            thir::Expr::Value(BamlValueWithMeta::Float(v, (span, Some(new_type))))
        }
        // TODO: Handle other expression forms
        _ => expr,
    }
}

/// Check if two types are compatible (for subsumption).
///
/// This implements the subtyping relation `A <: B`.
/// In a simple system, this is just equality.
/// In a system with subtyping, this checks if A is a subtype of B.
fn types_compatible(actual: &TypeIR, expected: &TypeIR) -> bool {
    // TODO: Implement proper subtyping
    // For now, just check equality
    actual == expected
}

// ============================================================================
// TESTS
// Note: These are mostly for testing small parts of the typechecker.
// More thorough typechecking tests should go in the validation test suite.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synth_bool_literal() {
        let ctx = TypeContext::new();
        let mut diagnostics = Diagnostics::new();

        let expr = hir::Expression::BoolValue(true, Span::default());
        let (typed, ty) = synth_expr(&ctx, &expr, &mut diagnostics);

        assert!(ty.is_some());
        assert_eq!(ty.unwrap(), TypeIR::bool());
        assert!(diagnostics.errors().is_empty());
    }

    #[test]
    fn test_check_bool_against_bool() {
        let ctx = TypeContext::new();
        let mut diagnostics = Diagnostics::new();

        let expr = hir::Expression::BoolValue(true, Span::default());
        let expected = TypeIR::bool();

        let _typed = check_expr(&ctx, &expr, &expected, &mut diagnostics);
        assert!(diagnostics.errors().is_empty());
    }

    #[test]
    fn test_subsumption_type_mismatch() {
        let ctx = TypeContext::new();
        let mut diagnostics = Diagnostics::new();

        let expr = hir::Expression::BoolValue(true, Span::default());
        let expected = TypeIR::int();

        let _typed = check_expr(&ctx, &expr, &expected, &mut diagnostics);
        assert!(!diagnostics.errors().is_empty());
    }
}
