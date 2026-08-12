//! The inference provider seam (S16): MIR consumes inference through the
//! ten `tir_*` accessors on `LoweringContext`, and this module supplies
//! their second backend - hir_ty's `InferenceResult` materialized into
//! the same TIR-shaped views the accessors already serve. The dual
//! provider is rustc's migration playbook (`-Z borrowck=compare` ran the
//! AST and MIR borrow checkers side by side until the diff was clean,
//! then the old engine and the flag were deleted); the differential MIR
//! gate diffs pretty-printed bodies per function across the corpus.
//!
//! Conversion happens ONCE per body at context construction (hir_ty
//! tables are engine-native interned types; the consumer boundary
//! materializes to the plain family), and the accessors then borrow from
//! the converted tables exactly as they borrow from `ScopeInference`.
//! Keying collapses by construction: hir_ty types lambdas in their
//! owner's arena and parameter defaults as their own body owner, so the
//! per-scope dispatch reduces to body-vs-defaults.

use baml_compiler2_hir::body::BodyOwnerId;
use baml_compiler2_hir_ty::infer as hir_infer;
use baml_compiler2_tir::inference::{
    CallPlan, CallSideChannels, FunctionCoercion, MemberResolution, ParamBinding,
};
use baml_compiler2_tir::ty::Ty as Tir2Ty;
use rustc_hash::{FxHashMap, FxHashSet};

use baml_compiler2_ast::ExprId as AstExprId;
use baml_compiler2_ast::PatId as AstPatId;

/// Which engine backs the `tir_*` accessors for one lowering run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InferenceProvider {
    /// TIR's `ScopeInference` side tables (the production default until
    /// the S16 flip).
    #[default]
    Tir,
    /// hir_ty's `InferenceResult`, converted at construction. TIR is not
    /// consulted at all under this provider.
    HirTy,
}

/// One owner's converted tables plus its parameter-default arena's.
pub(crate) struct HirTables<'db> {
    body: ConvertedTables<'db>,
    defaults: ConvertedTables<'db>,
}

#[derive(Default)]
pub(crate) struct ConvertedTables<'db> {
    expr_types: FxHashMap<AstExprId, Tir2Ty>,
    pat_types: FxHashMap<AstPatId, Tir2Ty>,
    resolutions: FxHashMap<AstExprId, MemberResolution<'db>>,
    path_root_types: FxHashMap<AstExprId, Tir2Ty>,
    path_segment_types: FxHashMap<(AstExprId, usize), Tir2Ty>,
    path_member_resolutions: FxHashMap<AstExprId, Vec<MemberResolution<'db>>>,
    call_plans: FxHashMap<AstExprId, CallPlan>,
    function_coercions: FxHashMap<AstExprId, FunctionCoercion>,
    /// hir_ty records the NON-exhaustive set; the accessor inverts. Only
    /// match expressions are ever queried (`lower_match`), so the
    /// inversion is total there.
    non_exhaustive_matches: FxHashSet<AstExprId>,
}

impl<'db> HirTables<'db> {
    pub(crate) fn for_function(
        db: &'db dyn crate::Db,
        function: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> HirTables<'db> {
        HirTables {
            body: convert(hir_infer::infer_body(db, BodyOwnerId::Function(function))),
            defaults: convert(hir_infer::infer_body(
                db,
                BodyOwnerId::ParameterDefaults(function),
            )),
        }
    }

    pub(crate) fn for_let(
        db: &'db dyn crate::Db,
        let_binding: baml_compiler2_hir::loc::LetLoc<'db>,
    ) -> HirTables<'db> {
        HirTables {
            body: convert(hir_infer::infer_body(db, BodyOwnerId::Let(let_binding))),
            defaults: ConvertedTables::default(),
        }
    }

    /// The table set a metadata scope reads from: hir_ty keys lambdas in
    /// the owner arena, so every `Body` scope (owner and lambda alike)
    /// reads the one body table.
    pub(crate) fn for_scope(
        &self,
        scope: baml_compiler2_hir::semantic_index::ExprMetadataScope,
    ) -> &ConvertedTables<'db> {
        use baml_compiler2_hir::semantic_index::ExprMetadataScope;
        match scope {
            ExprMetadataScope::Body(_) => &self.body,
            ExprMetadataScope::ParameterDefault(_) => &self.defaults,
        }
    }
}

impl<'db> ConvertedTables<'db> {
    pub(crate) fn expr_type(&self, expr: AstExprId) -> Option<&Tir2Ty> {
        self.expr_types.get(&expr)
    }
    pub(crate) fn pat_type(&self, pat: AstPatId) -> Option<&Tir2Ty> {
        self.pat_types.get(&pat)
    }
    pub(crate) fn resolution(&self, expr: AstExprId) -> Option<&MemberResolution<'db>> {
        self.resolutions.get(&expr)
    }
    pub(crate) fn path_root_type(&self, expr: AstExprId) -> Option<&Tir2Ty> {
        self.path_root_types.get(&expr)
    }
    pub(crate) fn path_segment_type(&self, expr: AstExprId, segment: usize) -> Option<&Tir2Ty> {
        self.path_segment_types.get(&(expr, segment))
    }
    pub(crate) fn path_member_resolutions(
        &self,
        expr: AstExprId,
    ) -> Option<&[MemberResolution<'db>]> {
        self.path_member_resolutions.get(&expr).map(Vec::as_slice)
    }
    pub(crate) fn call_plan(&self, expr: AstExprId) -> Option<&CallPlan> {
        self.call_plans.get(&expr)
    }
    pub(crate) fn function_coercion(&self, expr: AstExprId) -> Option<&FunctionCoercion> {
        self.function_coercions.get(&expr)
    }
    pub(crate) fn is_exhaustive_match(&self, expr: AstExprId) -> bool {
        !self.non_exhaustive_matches.contains(&expr)
    }
}

/// Materializes one `InferenceResult` into TIR-shaped tables: interned
/// types to the plain family, the resolution enum variant-for-variant,
/// the path ladder into TIR's three keyings, and adjustments into
/// `FunctionCoercion` (source shape from `type_of_expr`, target from the
/// adjustment - the redundancy TIR stored, reconstructed at the
/// boundary).
fn convert<'db>(result: &hir_infer::InferenceResult<'db>) -> ConvertedTables<'db> {
    let mut out = ConvertedTables::default();
    for (&expr, ty) in &result.type_of_expr {
        // Sugar callees present as UNTYPED (TIR's convention: MIR keys
        // the to_string/to_json/from_json desugars on the absence of a
        // recorded callee type). hir_ty records the type AND the sugar
        // decision; the provider materializes the absence. Post-flip,
        // MIR reads desugared_callees directly instead.
        if result.desugared_callees.contains(&expr) {
            continue;
        }
        out.expr_types
            .insert(expr, widen_fresh_throws(ty).to_plain());
    }
    for (&pat, ty) in &result.type_of_pat {
        out.pat_types.insert(pat, widen_fresh_throws(ty).to_plain());
    }
    for (&expr, resolution) in &result.member_resolutions {
        out.resolutions.insert(expr, convert_resolution(resolution));
    }
    for (&expr, path) in &result.path_resolutions {
        if let Some(root) = path.segments.first() {
            out.path_root_types.insert(expr, root.ty.to_plain());
        }
        for (index, segment) in path.segments.iter().enumerate() {
            out.path_segment_types
                .insert((expr, index), segment.ty.to_plain());
        }
        // TIR's vec holds one entry per MEMBER segment (the suffix after
        // the root); a ladder with an unresolved member records no vec -
        // absent, so MIR falls back, rather than misaligned.
        let members: Option<Vec<MemberResolution<'db>>> = path.segments[1..]
            .iter()
            .map(|segment| segment.resolution.as_ref().map(convert_resolution))
            .collect();
        if let Some(members) = members {
            out.path_member_resolutions.insert(expr, members);
        }
    }
    for (&call, plan) in &result.call_plans {
        out.call_plans.insert(
            call,
            CallPlan {
                bindings: plan
                    .bindings
                    .iter()
                    .map(|binding| match binding {
                        hir_infer::ParamBinding::Provided { param_index, arg } => {
                            ParamBinding::Provided {
                                param_index: *param_index,
                                arg: *arg,
                            }
                        }
                        hir_infer::ParamBinding::OmittedDefault {
                            param_index,
                            param_name,
                        } => ParamBinding::OmittedDefault {
                            param_index: *param_index,
                            param_name: param_name.clone(),
                        },
                    })
                    .collect(),
                // The runtime convention (TIR's runtime_call_type_args):
                // only the OWN suffix threads as call operands - the
                // receiver/impl frame supplies the owner prefix - and
                // fresh literals WIDEN (class type args match invariantly
                // at runtime; an escaped `literal "hi"` would never match
                // `is Box<string>`). Turbofish calls carry NO plan args
                // (MIR lowers the written types; TIR's
                // `!explicit_args_used` gate).
                type_args: if plan.explicit {
                    Vec::new()
                } else {
                    plan.type_args[plan.own_offset..]
                        .iter()
                        .map(|ty| ty.to_plain().widen_fresh())
                        .collect()
                },
                // Not consumed by MIR (TIR's throws machinery only).
                instantiated_throws: None,
                side_channels: CallSideChannels {
                    runtime_id: plan.runtime_id,
                },
            },
        );
    }
    for (&expr, adjustments) in &result.expr_adjustments {
        for adjustment in adjustments.iter() {
            let hir_infer::Adjust::FunctionAdapter = adjustment.kind;
            let (
                Some(Tir2Ty::Function {
                    params: source_params,
                    ..
                }),
                Tir2Ty::Function {
                    params: target_params,
                    ret: target_return,
                    ..
                },
            ) = (
                result.type_of_expr.get(&expr).map(|ty| ty.to_plain()),
                adjustment.target.to_plain(),
            )
            else {
                continue;
            };
            out.function_coercions.insert(
                expr,
                FunctionCoercion {
                    source_params,
                    target_params,
                    target_return: *target_return,
                },
            );
        }
    }
    out.non_exhaustive_matches = result.non_exhaustive_matches.iter().copied().collect();
    out
}

/// Literals in `throws` position widen to their bases at the runtime
/// boundary (the same boundary rule `type_args` follows below): hir_ty
/// keeps literal-grain effect surfaces engine-side (the S13 fixtures'
/// catch-fact subtraction), but the runtime's error contract is
/// base-typed - TIR widens every thrown-literal contribution, ratified
/// by `reflect.signature` reconstructing `string` from a
/// `throw "negative"` lambda.
fn widen_fresh_throws(ty: &baml_type::interned::Ty) -> baml_type::interned::Ty {
    use baml_type::interned::{Ty as HirTy, TyKind};
    fn widen_member(ty: &HirTy) -> HirTy {
        let TyKind::Literal(literal, _, attr) = ty.kind() else {
            return ty.clone();
        };
        let attr = attr.clone();
        HirTy::intern(match literal {
            baml_type::Literal::Int(_) => TyKind::Int { attr },
            baml_type::Literal::Bigint(_) => TyKind::Bigint { attr },
            baml_type::Literal::Float(_) => TyKind::Float { attr },
            baml_type::Literal::String(_) => TyKind::String { attr },
            baml_type::Literal::Bool(_) => TyKind::Bool { attr },
        })
    }
    let rebuilt = ty.kind().map_children(|child| widen_fresh_throws(child));
    let TyKind::Function {
        params,
        ret,
        throws,
        attr,
    } = rebuilt
    else {
        return HirTy::intern(rebuilt);
    };
    let throws = match throws.kind() {
        TyKind::Union(members, union_attr) => HirTy::intern(TyKind::Union(
            members.iter().map(widen_member).collect(),
            union_attr.clone(),
        )),
        _ => widen_member(&throws),
    };
    HirTy::intern(TyKind::Function {
        params,
        ret,
        throws,
        attr,
    })
}

fn convert_resolution<'db>(resolution: &hir_infer::MemberResolution<'db>) -> MemberResolution<'db> {
    match resolution {
        hir_infer::MemberResolution::Field { class, field } => MemberResolution::Field {
            class_loc: *class,
            field_name: field.clone(),
        },
        hir_infer::MemberResolution::Variant { enum_loc, variant } => MemberResolution::Variant {
            enum_loc: *enum_loc,
            variant_name: variant.clone(),
        },
        hir_infer::MemberResolution::Free { func } => MemberResolution::Free { func_loc: *func },
        hir_infer::MemberResolution::BoundMethod { class, func } => MemberResolution::BoundMethod {
            class_loc: *class,
            func_loc: *func,
        },
        hir_infer::MemberResolution::UnboundMethod { class, func } => {
            MemberResolution::UnboundMethod {
                class_loc: *class,
                func_loc: *func,
            }
        }
        hir_infer::MemberResolution::InterfaceVirtualMethod { interface, method } => {
            MemberResolution::InterfaceVirtualMethod {
                iface_loc: *interface,
                method: method.clone(),
            }
        }
        hir_infer::MemberResolution::InterfaceConcreteMethod { impl_block, func } => {
            MemberResolution::InterfaceConcreteMethod {
                impl_loc: *impl_block,
                func_loc: *func,
            }
        }
        hir_infer::MemberResolution::InterfaceVirtualField {
            interface,
            view,
            field_index,
            field,
        } => MemberResolution::InterfaceVirtualField {
            iface_loc: *interface,
            interface: view.to_plain(),
            field_index: *field_index,
            field: field.clone(),
        },
    }
}
