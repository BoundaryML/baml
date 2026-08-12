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
use baml_compiler2_hir::loc::{ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc};
use baml_compiler2_hir_ty::infer as hir_infer;
use baml_type::{Name, Ty as Tir2Ty};
use rustc_hash::{FxHashMap, FxHashSet};

use baml_compiler2_ast::ExprId as AstExprId;
use baml_compiler2_ast::PatId as AstPatId;

// --- MIR's consumption vocabulary -------------------------------------------
//
// MIR is plain-typed; these are the shapes its lowering reads, owned HERE at
// the seam (rustc's discipline: codegen consumes its own erased view of the
// type system, never the inference engine's native tables). hir_ty's interned
// tables materialize into them once per body below - the permanent boundary.
// The retiring TIR arm feeds the same shapes through a near-identity bridge
// that dies with TIR.

/// How a member access resolved - the structural path MIR lowers through.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MemberResolution<'db> {
    /// A class field access (e.g. `p.name`).
    Field {
        class_loc: ClassLoc<'db>,
        field_name: Name,
    },
    /// An enum variant access (e.g. `Status.Active`).
    Variant {
        enum_loc: EnumLoc<'db>,
        variant_name: Name,
    },
    /// A free item accessed via a package/namespace path.
    Free { func_loc: FunctionLoc<'db> },
    /// A bound method reference: root is a value; type has `self` stripped.
    BoundMethod {
        class_loc: ClassLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    /// An unbound method reference: root is a type name; type keeps `self`.
    UnboundMethod {
        class_loc: ClassLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    /// A VIRTUAL interface-method call: only the slot (interface + member)
    /// is statically known; dispatch resolves to the receiver's runtime impl.
    InterfaceVirtualMethod {
        iface_loc: InterfaceLoc<'db>,
        method: Name,
    },
    /// A CONCRETE interface-method call through a statically-matched impl.
    InterfaceConcreteMethod {
        impl_loc: ImplLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    /// A VIRTUAL interface-field access through the realized declaring view.
    InterfaceVirtualField {
        iface_loc: InterfaceLoc<'db>,
        interface: Tir2Ty,
        field_index: u32,
        field: Name,
    },
}

/// One call's argument/parameter pairing plus its runtime type arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CallPlan {
    pub(crate) bindings: Vec<ParamBinding>,
    pub(crate) type_args: Vec<Tir2Ty>,
    /// Hidden call metadata which is not part of the callee's parameter list.
    pub(crate) side_channels: CallSideChannels,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CallSideChannels {
    /// The trailing `boundary.LocalId` expression supplied as `$id = ...`.
    pub(crate) runtime_id: Option<AstExprId>,
}

impl CallPlan {
    pub(crate) fn provided_args(&self) -> impl Iterator<Item = AstExprId> + '_ {
        self.bindings.iter().filter_map(|binding| match binding {
            ParamBinding::Provided { arg, .. } => Some(*arg),
            ParamBinding::OmittedDefault { .. } => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParamBinding {
    Provided {
        param_index: usize,
        arg: AstExprId,
    },
    OmittedDefault {
        param_index: usize,
        param_name: Name,
    },
}

/// A function value accepted at a runtime-incompatible parameter shape - the
/// adapter MIR emits (source shape from the value, target from the slot).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FunctionCoercion {
    pub(crate) source_params: Vec<baml_type::FunctionParamTy>,
    pub(crate) target_params: Vec<baml_type::FunctionParamTy>,
    pub(crate) target_return: Tir2Ty,
}

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

/// The one table store behind the `tir_*` accessors: converted ONCE at
/// context construction, whichever engine produced them.
pub(crate) enum ProviderTables<'db> {
    /// hir_ty keys lambdas in the owner arena and defaults as their own
    /// body owner, so every `Body` scope reads the one body table.
    Hir {
        body: ConvertedTables<'db>,
        defaults: ConvertedTables<'db>,
    },
    /// TIR's per-scope tables, bridged shape-for-shape (sweep-only since
    /// the flip; dies with TIR).
    Tir {
        scopes: FxHashMap<baml_compiler2_hir::scope::FileScopeId, ScopePair<'db>>,
    },
}

/// One scope's body tables plus its parameter-default tables (the same
/// pairing the metadata scopes encode).
pub(crate) struct ScopePair<'db> {
    body: ConvertedTables<'db>,
    defaults: ConvertedTables<'db>,
}

impl<'db> ProviderTables<'db> {
    /// Whether the retiring TIR arm backs this run (engine-side choices
    /// like the L1 impl substrate follow the engine).
    pub(crate) fn is_tir(&self) -> bool {
        matches!(self, ProviderTables::Tir { .. })
    }

    pub(crate) fn for_scope(
        &self,
        scope: baml_compiler2_hir::semantic_index::ExprMetadataScope,
    ) -> Option<&ConvertedTables<'db>> {
        use baml_compiler2_hir::semantic_index::ExprMetadataScope;
        match self {
            ProviderTables::Hir { body, defaults } => Some(match scope {
                ExprMetadataScope::Body(_) => body,
                ExprMetadataScope::ParameterDefault(_) => defaults,
            }),
            ProviderTables::Tir { scopes } => match scope {
                ExprMetadataScope::Body(fsi) => scopes.get(&fsi).map(|pair| &pair.body),
                ExprMetadataScope::ParameterDefault(fsi) => {
                    scopes.get(&fsi).map(|pair| &pair.defaults)
                }
            },
        }
    }
}

/// The two engines record match exhaustiveness with opposite polarity:
/// hir_ty the NON-exhaustive set (absence = proved exhaustive), TIR the
/// exhaustive set. Only match expressions are ever queried
/// (`lower_match`), so both answers are total there. Dies to the
/// hir_ty arm at TIR deletion.
#[derive(Default)]
enum MatchExhaustiveness {
    #[default]
    Empty,
    NonExhaustiveSet(FxHashSet<AstExprId>),
    ExhaustiveSet(FxHashSet<AstExprId>),
}

impl MatchExhaustiveness {
    fn is_exhaustive(&self, expr: AstExprId) -> bool {
        match self {
            MatchExhaustiveness::Empty => true,
            MatchExhaustiveness::NonExhaustiveSet(set) => !set.contains(&expr),
            MatchExhaustiveness::ExhaustiveSet(set) => set.contains(&expr),
        }
    }
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
    exhaustiveness: MatchExhaustiveness,
}

impl<'db> ProviderTables<'db> {
    pub(crate) fn for_function(
        db: &'db dyn crate::Db,
        function: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> ProviderTables<'db> {
        ProviderTables::Hir {
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
    ) -> ProviderTables<'db> {
        ProviderTables::Hir {
            body: convert(hir_infer::infer_body(db, BodyOwnerId::Let(let_binding))),
            defaults: ConvertedTables::default(),
        }
    }

    /// The retiring arm: TIR's salsa-cached per-scope tables, bridged
    /// shape-for-shape into MIR's vocabulary. Sweep-only since the flip.
    pub(crate) fn from_tir(
        scopes: impl IntoIterator<
            Item = (
                baml_compiler2_hir::scope::FileScopeId,
                &'db baml_compiler2_tir::inference::ScopeInference<'db>,
            ),
        >,
    ) -> ProviderTables<'db> {
        ProviderTables::Tir {
            scopes: scopes
                .into_iter()
                .map(|(fsi, inference)| {
                    (
                        fsi,
                        ScopePair {
                            body: tir_body_tables(inference),
                            defaults: tir_default_tables(inference),
                        },
                    )
                })
                .collect(),
        }
    }
}

fn tir_body_tables<'db>(
    inference: &baml_compiler2_tir::inference::ScopeInference<'db>,
) -> ConvertedTables<'db> {
    ConvertedTables {
        expr_types: inference.expressions.clone(),
        pat_types: inference.pattern_types.clone(),
        resolutions: inference
            .resolutions
            .iter()
            .map(|(&expr, resolution)| (expr, tir_resolution(resolution)))
            .collect(),
        path_root_types: inference.path_root_types.clone(),
        path_segment_types: inference.path_segment_types.clone(),
        path_member_resolutions: inference
            .path_member_resolutions
            .iter()
            .map(|(&expr, resolutions)| (expr, resolutions.iter().map(tir_resolution).collect()))
            .collect(),
        call_plans: inference
            .call_plans
            .iter()
            .map(|(&expr, plan)| (expr, tir_call_plan(plan)))
            .collect(),
        function_coercions: inference
            .function_coercions
            .iter()
            .map(|(&expr, coercion)| (expr, tir_coercion(coercion)))
            .collect(),
        exhaustiveness: MatchExhaustiveness::ExhaustiveSet(inference.exhaustive_matches.clone()),
    }
}

fn tir_default_tables<'db>(
    inference: &baml_compiler2_tir::inference::ScopeInference<'db>,
) -> ConvertedTables<'db> {
    let defaults = &inference.parameter_defaults;
    ConvertedTables {
        expr_types: defaults.expressions.clone(),
        pat_types: defaults.pattern_types.clone(),
        resolutions: defaults
            .resolutions
            .iter()
            .map(|(&expr, resolution)| (expr, tir_resolution(resolution)))
            .collect(),
        path_root_types: defaults.path_root_types.clone(),
        path_segment_types: defaults.path_segment_types.clone(),
        path_member_resolutions: defaults
            .path_member_resolutions
            .iter()
            .map(|(&expr, resolutions)| (expr, resolutions.iter().map(tir_resolution).collect()))
            .collect(),
        call_plans: defaults
            .call_plans
            .iter()
            .map(|(&expr, plan)| (expr, tir_call_plan(plan)))
            .collect(),
        function_coercions: defaults
            .function_coercions
            .iter()
            .map(|(&expr, coercion)| (expr, tir_coercion(coercion)))
            .collect(),
        exhaustiveness: MatchExhaustiveness::ExhaustiveSet(defaults.exhaustive_matches.clone()),
    }
}

fn tir_resolution<'db>(
    resolution: &baml_compiler2_tir::inference::MemberResolution<'db>,
) -> MemberResolution<'db> {
    use baml_compiler2_tir::inference::MemberResolution as Tir;
    match resolution {
        Tir::Field {
            class_loc,
            field_name,
        } => MemberResolution::Field {
            class_loc: *class_loc,
            field_name: field_name.clone(),
        },
        Tir::Variant {
            enum_loc,
            variant_name,
        } => MemberResolution::Variant {
            enum_loc: *enum_loc,
            variant_name: variant_name.clone(),
        },
        Tir::Free { func_loc } => MemberResolution::Free {
            func_loc: *func_loc,
        },
        Tir::BoundMethod {
            class_loc,
            func_loc,
        } => MemberResolution::BoundMethod {
            class_loc: *class_loc,
            func_loc: *func_loc,
        },
        Tir::UnboundMethod {
            class_loc,
            func_loc,
        } => MemberResolution::UnboundMethod {
            class_loc: *class_loc,
            func_loc: *func_loc,
        },
        Tir::InterfaceVirtualMethod { iface_loc, method } => {
            MemberResolution::InterfaceVirtualMethod {
                iface_loc: *iface_loc,
                method: method.clone(),
            }
        }
        Tir::InterfaceConcreteMethod { impl_loc, func_loc } => {
            MemberResolution::InterfaceConcreteMethod {
                impl_loc: *impl_loc,
                func_loc: *func_loc,
            }
        }
        Tir::InterfaceVirtualField {
            iface_loc,
            interface,
            field_index,
            field,
        } => MemberResolution::InterfaceVirtualField {
            iface_loc: *iface_loc,
            interface: interface.clone(),
            field_index: *field_index,
            field: field.clone(),
        },
    }
}

fn tir_call_plan(plan: &baml_compiler2_tir::inference::CallPlan) -> CallPlan {
    use baml_compiler2_tir::inference::ParamBinding as Tir;
    CallPlan {
        bindings: plan
            .bindings
            .iter()
            .map(|binding| match binding {
                Tir::Provided { param_index, arg } => ParamBinding::Provided {
                    param_index: *param_index,
                    arg: *arg,
                },
                Tir::OmittedDefault {
                    param_index,
                    param_name,
                } => ParamBinding::OmittedDefault {
                    param_index: *param_index,
                    param_name: param_name.clone(),
                },
            })
            .collect(),
        type_args: plan.type_args.clone(),
        side_channels: CallSideChannels {
            runtime_id: plan.side_channels.runtime_id,
        },
    }
}

fn tir_coercion(coercion: &baml_compiler2_tir::inference::FunctionCoercion) -> FunctionCoercion {
    FunctionCoercion {
        source_params: coercion.source_params.clone(),
        target_params: coercion.target_params.clone(),
        target_return: coercion.target_return.clone(),
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
        self.exhaustiveness.is_exhaustive(expr)
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
    out.exhaustiveness = MatchExhaustiveness::NonExhaustiveSet(
        result.non_exhaustive_matches.iter().copied().collect(),
    );
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
