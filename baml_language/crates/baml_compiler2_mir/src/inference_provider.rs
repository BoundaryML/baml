//! The inference provider seam (S16): MIR consumes inference through the
//! ten `tir_*` accessors on `LoweringContext`, and this module supplies
//! their second backend - `hir_ty`'s `InferenceResult` materialized into
//! the same TIR-shaped views the accessors already serve. The dual
//! provider is rustc's migration playbook (`-Z borrowck=compare` ran the
//! AST and MIR borrow checkers side by side until the diff was clean,
//! then the old engine and the flag were deleted); the differential MIR
//! gate diffs pretty-printed bodies per function across the corpus.
//!
//! Conversion happens ONCE per body at context construction (`hir_ty`
//! tables are engine-native interned types; the consumer boundary
//! materializes to the plain family), and the accessors then borrow from
//! the converted tables exactly as they borrow from `ScopeInference`.
//! Keying collapses by construction: `hir_ty` types lambdas in their
//! owner's arena and parameter defaults as their own body owner, so the
//! per-scope dispatch reduces to body-vs-defaults.

use baml_compiler2_ast::{ExprId as AstExprId, PatId as AstPatId};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    loc::{ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc},
};
use baml_compiler2_hir_ty::infer as hir_infer;
use baml_type::{Name, Ty as Tir2Ty};
use rustc_hash::{FxHashMap, FxHashSet};

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

/// The one table store behind the `tir_*` accessors: converted ONCE at
/// context construction, whichever engine produced them.
pub(crate) struct ProviderTables<'db> {
    /// `hir_ty` keys lambdas in the owner arena and defaults as their own
    /// body owner, so every `Body` scope reads the one body table.
    body: ConvertedTables<'db>,
    defaults: ConvertedTables<'db>,
}

impl<'db> ProviderTables<'db> {
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

/// The two engines record match exhaustiveness with opposite polarity:
/// `hir_ty` the NON-exhaustive set (absence = proved exhaustive), TIR the
/// exhaustive set. Only match expressions are ever queried
/// (`lower_match`), so both answers are total there. Dies to the
/// `hir_ty` arm at TIR deletion.
#[derive(Default)]
enum MatchExhaustiveness {
    #[default]
    Empty,
    NonExhaustiveSet(FxHashSet<AstExprId>),
}

impl MatchExhaustiveness {
    fn is_exhaustive(&self, expr: AstExprId) -> bool {
        match self {
            MatchExhaustiveness::Empty => true,
            MatchExhaustiveness::NonExhaustiveSet(set) => !set.contains(&expr),
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
        ProviderTables {
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
        ProviderTables {
            body: convert(hir_infer::infer_body(db, BodyOwnerId::Let(let_binding))),
            defaults: ConvertedTables::default(),
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
        out.expr_types.insert(expr, ty.to_plain());
    }
    for (&pat, ty) in &result.type_of_pat {
        out.pat_types.insert(pat, ty.to_plain());
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
        for adjustment in adjustments {
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
                result
                    .type_of_expr
                    .get(&expr)
                    .map(baml_type::interned::Ty::to_plain),
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
