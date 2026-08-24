//! Per-function signature queries.
//!
//! Reads from the `ItemTree` (full AST data stored in Phase 1) — no CST access
//! needed. The semantic data (`TypeExpr`, no spans) and the source map (spans
//! only) are split into separate queries for Salsa early-cutoff: whitespace
//! changes re-run the source map query but NOT the signature query.

use std::sync::Arc;

use baml_base::Name;
use baml_compiler2_ast::{FunctionDefaults, FunctionTypeParam, TypeExpr, TypeExprKind};
use rustc_hash::FxHashSet;
use text_size::TextRange;

use crate::{item_tree::DefaultExprRef, loc::FunctionLoc};

/// Compiler2 function signature — param names + unresolved `ast::TypeExpr`.
///
/// The `SignatureSourceMap` twin holds the item-level spans. This struct is NOT
/// fully span-free, though: `ast::TypeExpr` carries its own `span` inline (its
/// `PartialEq` ignores that span but transitively compares `RawAttribute` spans),
/// so a whitespace edit near an attribute can still bust this query's cutoff. The
/// span-free successor is `ppir::function_data` over the `TypeRef` arena; this
/// struct is retained until its consumers migrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub name: baml_base::Name,
    /// Parameter names paired with their unresolved type expressions.
    pub params: Vec<SignatureParam>,
    /// Return type (None if omitted).
    pub return_type: Option<TypeExpr>,
    /// Declared throws contract type (None if omitted).
    pub throws: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureParam {
    pub name: Name,
    pub ty: TypeExpr,
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParameterDefaults {
    /// One default-expression reference per parameter, parallel to
    /// `FunctionSignature::params`.
    pub params: Vec<Option<DefaultExprRef>>,
    /// The definition-local default expression arena and source map.
    pub defaults: FunctionDefaults,
}

impl FunctionParameterDefaults {
    pub fn param_default(&self, index: usize) -> Option<&DefaultExprRef> {
        self.params.get(index).and_then(Option::as_ref)
    }
}

/// Canonical callable-signature view used by TIR.
///
/// This keeps the user-written top-level throws contract optional (inferred
/// from the body, `TYPE_SYSTEM.md` rule 3). Callback parameter roots — the
/// parameter type itself, or the function type reached through optionality
/// (`T?` / `T | null`) — with omitted throws are opened to a fresh synthetic
/// effect parameter
/// (rule 4). Every other function type must declare its `throws` explicitly
/// (rule 5) — an omitted clause is left as `None` here and rejected during TIR
/// lowering (`FunctionTypeMissingThrows`, E0151).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedFunctionSignature {
    pub name: Name,
    pub user_generic_params: Vec<Name>,
    pub synthetic_effect_params: Vec<Name>,
    pub params: Vec<SignatureParam>,
    pub return_type: Option<TypeExpr>,
    pub throws: Option<TypeExpr>,
}

/// Parallel span storage for a signature.
///
/// Kept separate from `FunctionSignature` so that whitespace-only source
/// changes only invalidate `function_signature_source_map`, not
/// `function_signature`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureSourceMap {
    /// One span per parameter, parallel to `FunctionSignature::params`.
    pub param_spans: Vec<TextRange>,
    /// One span per parameter's type expression (just the type, not the name).
    /// `None` when the parameter has no explicit type annotation.
    pub param_type_spans: Vec<Option<TextRange>>,
    /// Span of the return type annotation, if present.
    pub return_type_span: Option<TextRange>,
    /// Span of the throws type annotation, if present.
    pub throws_type_span: Option<TextRange>,
}

/// Shared implementation — reads from the `ItemTree` (full AST data),
/// splits into semantic (`TypeExpr`, no spans) + source map (spans only).
fn function_signature_with_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> (Arc<FunctionSignature>, SignatureSourceMap) {
    let file = function.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    // Build semantic signature — strip spans, keep TypeExpr
    let params: Vec<_> = func_data
        .params
        .iter()
        .map(|p| {
            let type_expr = p.type_expr.clone().unwrap_or_else(|| {
                TypeExprKind::Unknown { attrs: vec![] }.at(TextRange::default())
            });
            SignatureParam {
                name: p.name.clone(),
                ty: type_expr,
                has_default: p.default.is_some(),
            }
        })
        .collect();

    let return_type = func_data.return_type.clone();

    let sig = Arc::new(FunctionSignature {
        name: func_data.name.clone(),
        params,
        return_type,
        throws: func_data.throws.clone(),
    });

    // Build source map — spans only (separate for early-cutoff)
    let source_map = SignatureSourceMap {
        param_spans: func_data.params.iter().map(|p| p.span).collect(),
        param_type_spans: func_data
            .params
            .iter()
            .map(|p| p.type_expr.as_ref().map(|te| te.span))
            .collect(),
        return_type_span: func_data.return_type.as_ref().map(|te| te.span),
        throws_type_span: func_data.throws.as_ref().map(|te| te.span),
    };

    (sig, source_map)
}

fn type_expr_for_effect_param(name: Name) -> TypeExpr {
    TypeExprKind::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
    .at(TextRange::default())
}

fn fresh_effect_param_name(used_names: &mut FxHashSet<Name>) -> Name {
    let mut index = 0usize;
    loop {
        let candidate = Name::new(format!("__effect_param_{index}"));
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn elaborate_immediate_callback_param(
    params: Vec<FunctionTypeParam>,
    ret: TypeExpr,
    attrs: Vec<baml_compiler2_ast::RawAttribute>,
    effect_param: Name,
) -> TypeExpr {
    TypeExprKind::Function {
        params,
        ret: Box::new(ret),
        throws: Some(Box::new(type_expr_for_effect_param(effect_param))),
        attrs,
    }
    .at(TextRange::default())
}

/// Open one parameter's callback root to a synthetic effect param.
///
/// The callback root is the parameter type itself, or the function type
/// reached from it through optionality only — `((v: int) -> int)?`, and the
/// longhand `((v: int) -> int) | null` that denotes the same type. An
/// optional callback is still a *callback slot* the caller fills at the call
/// site, so it participates in the same per-call-site effect inference as the
/// immediate form (rule 4); passing `null` simply leaves the effect variable
/// unconstrained, which defaults to `never`.
///
/// Every other nesting (list element, map value, class field, alias body, a
/// returned function type, a callback's own parameter, a union with a
/// non-null arm) is deliberately NOT opened — those are stored/structural
/// positions with no single call site to instantiate against, and an omitted
/// `throws` there stays an E0151.
fn elaborate_callback_param_root(
    ty: TypeExpr,
    used_names: &mut FxHashSet<Name>,
    synthetic_effect_params: &mut Vec<Name>,
) -> TypeExpr {
    match ty.kind {
        TypeExprKind::Function {
            params,
            ret,
            throws: None,
            attrs,
        } => {
            let effect_param = fresh_effect_param_name(used_names);
            synthetic_effect_params.push(effect_param.clone());
            elaborate_immediate_callback_param(params, *ret, attrs, effect_param)
        }
        TypeExprKind::Optional { inner, attrs } => {
            let inner = elaborate_callback_param_root(*inner, used_names, synthetic_effect_params);
            TypeExprKind::Optional {
                inner: Box::new(inner),
                attrs,
            }
            .at(ty.span)
        }
        // `T | null` is the same type as `T?`, so it opens the same way. A
        // union carrying any other arm is not an optional callback and is
        // left alone.
        TypeExprKind::Union { variants, attrs }
            if variants
                .iter()
                .filter(|variant| !matches!(variant.kind, TypeExprKind::Null { .. }))
                .count()
                == 1 =>
        {
            let variants = variants
                .into_iter()
                .map(|variant| match variant.kind {
                    TypeExprKind::Null { .. } => variant,
                    _ => {
                        elaborate_callback_param_root(variant, used_names, synthetic_effect_params)
                    }
                })
                .collect();
            TypeExprKind::Union { variants, attrs }.at(ty.span)
        }
        other => other.at(ty.span),
    }
}

pub fn elaborate_function_signature_parts(
    name: Name,
    user_generic_params: Vec<Name>,
    reserved_effect_param_names: &[Name],
    params: Vec<SignatureParam>,
    return_type: Option<TypeExpr>,
    throws: Option<TypeExpr>,
) -> ElaboratedFunctionSignature {
    let mut used_names: FxHashSet<Name> = user_generic_params.iter().cloned().collect();
    used_names.extend(reserved_effect_param_names.iter().cloned());

    let mut synthetic_effect_params = Vec::new();
    let params = params
        .into_iter()
        .map(|param| {
            let elaborated = elaborate_callback_param_root(
                param.ty,
                &mut used_names,
                &mut synthetic_effect_params,
            );
            SignatureParam {
                name: param.name,
                ty: elaborated,
                has_default: param.has_default,
            }
        })
        .collect();

    ElaboratedFunctionSignature {
        name,
        user_generic_params,
        synthetic_effect_params,
        params,
        return_type,
        throws,
    }
}

fn elaborated_function_signature_with_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> (Arc<ElaboratedFunctionSignature>, SignatureSourceMap) {
    let file = function.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    let params: Vec<_> = func_data
        .params
        .iter()
        .map(|p| {
            let type_expr = p.type_expr.clone().unwrap_or_else(|| {
                TypeExprKind::Unknown { attrs: vec![] }.at(TextRange::default())
            });
            SignatureParam {
                name: p.name.clone(),
                ty: type_expr,
                has_default: p.default.is_some(),
            }
        })
        .collect();

    let return_type = func_data.return_type.clone();
    let throws = func_data.throws.clone();
    let reserved_effect_param_names: Vec<Name> = item_tree
        .enclosing_type_generic_params(function.id(db))
        .iter()
        .map(|param| param.name.clone())
        .collect();
    let signature = Arc::new(elaborate_function_signature_parts(
        func_data.name.clone(),
        func_data
            .generic_params
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        &reserved_effect_param_names,
        params,
        return_type,
        throws,
    ));

    let source_map = SignatureSourceMap {
        param_spans: func_data.params.iter().map(|p| p.span).collect(),
        param_type_spans: func_data
            .params
            .iter()
            .map(|p| p.type_expr.as_ref().map(|te| te.span))
            .collect(),
        return_type_span: func_data.return_type.as_ref().map(|te| te.span),
        throws_type_span: func_data.throws.as_ref().map(|te| te.span),
    };

    (signature, source_map)
}

/// Salsa query: semantic function signature (no spans).
///
/// Cached independently of the source map. Downstream type-checking queries
/// depend on this and will NOT re-run on whitespace-only file changes.
#[salsa::tracked]
pub fn function_signature<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Arc<FunctionSignature> {
    let (signature, _) = function_signature_with_source_map(db, function);
    signature
}

/// Salsa query: function signature source map (spans only).
///
/// Re-runs on any file change (including whitespace), but because downstream
/// type queries only depend on `function_signature`, they are unaffected.
#[salsa::tracked]
pub fn function_signature_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> SignatureSourceMap {
    let (_, source_map) = function_signature_with_source_map(db, function);
    source_map
}

/// Salsa query: function parameter default-expression data.
///
/// Kept separate from `FunctionSignature` so changing a default expression does
/// not invalidate consumers that only need callable shape or optionality.
#[salsa::tracked]
pub fn function_parameter_defaults<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Arc<FunctionParameterDefaults> {
    let file = function.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    Arc::new(FunctionParameterDefaults {
        params: func_data
            .params
            .iter()
            .map(|param| param.default.clone())
            .collect(),
        defaults: func_data.defaults.clone(),
    })
}

/// Salsa query: elaborated callable signature used by TIR consumers.
#[salsa::tracked]
pub fn elaborated_function_signature<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Arc<ElaboratedFunctionSignature> {
    let (signature, _) = elaborated_function_signature_with_source_map(db, function);
    signature
}

/// Salsa query: source map for the elaborated callable signature.
///
/// The elaboration rewrites types but does not change source spans, so this is
/// intentionally parallel to `function_signature_source_map`.
#[salsa::tracked]
pub fn elaborated_function_signature_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> SignatureSourceMap {
    let (_, source_map) = elaborated_function_signature_with_source_map(db, function);
    source_map
}
