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

use baml_compiler2_ast::{ExprId as AstExprId, PatId as AstPatId, StmtId as AstStmtId};
use baml_compiler2_hir::body::BodyOwnerId;
pub(crate) use baml_compiler2_hir_ty::infer::Receiver;

impl<'db> MemberResolution<'db> {
    /// The callable a resolution statically names — the mirror of `hir_ty`'s
    /// accessor: a free function, an inherent or impl-provided method, or, for
    /// a virtual slot, the interface's own declaration of the method.
    pub(crate) fn callable(&self, db: &'db dyn crate::Db) -> Option<FunctionRef<'db>> {
        use baml_compiler2_hir::loc::DeclRef;
        match self {
            MemberResolution::Free { func_loc } => Some(*func_loc),
            MemberResolution::Method { callee, .. } => match callee {
                MethodCallee::Inherent(func_loc) | MethodCallee::Concrete { func_loc, .. } => {
                    Some(*func_loc)
                }
                MethodCallee::Virtual { iface_loc, method } => match iface_loc {
                    DeclRef::Source(interface) => {
                        baml_compiler2_ppir::item_data::interface_data(db, *interface)
                            .methods
                            .iter()
                            .copied()
                            .find(|&func| {
                                baml_compiler2_ppir::item_data::function_data(db, func).name
                                    == *method
                            })
                            .map(DeclRef::Source)
                    }
                    DeclRef::External(interface) => {
                        baml_compiler2_hir_ty::extern_loc::extern_interface_method(
                            db,
                            interface.head(db),
                            method,
                        )
                        .map(DeclRef::External)
                    }
                },
            },
            MemberResolution::Field { .. }
            | MemberResolution::Variant { .. }
            | MemberResolution::InterfaceVirtualField { .. } => None,
        }
    }
}
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef, FunctionRef, ImplRef, InterfaceRef},
    infer as hir_infer,
};
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
        class_loc: ClassRef<'db>,
        field_name: Name,
    },
    /// An enum variant access (e.g. `Status.Active`).
    Variant {
        enum_loc: EnumRef<'db>,
        variant_name: Name,
    },
    /// A free item accessed via a package/namespace path.
    Free { func_loc: FunctionRef<'db> },
    /// A method access: WHAT is called and whether the access binds its
    /// `self` — `hir_ty`'s two independent axes, mirrored. `Bound` (`recv.m`)
    /// has `self` stripped from the access's type; `Unbound` (`Type.m`,
    /// `I.m(recv, ..)`) keeps it, the receiver being the written first
    /// argument when the callee takes one.
    Method {
        callee: MethodCallee<'db>,
        receiver: Receiver,
    },
    /// A VIRTUAL interface-field access through the realized declaring view.
    InterfaceVirtualField {
        iface_loc: InterfaceRef<'db>,
        interface: Tir2Ty,
        field_index: u32,
        field: Name,
    },
}

/// What a method access calls — the mirror of `hir_ty`'s `MethodCallee`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MethodCallee<'db> {
    /// A class-inherent method.
    Inherent(FunctionRef<'db>),
    /// A VIRTUAL interface slot: only interface + member are statically
    /// known; dispatch resolves to the receiver's runtime impl.
    Virtual {
        iface_loc: InterfaceRef<'db>,
        method: Name,
    },
    /// A CONCRETE interface method through a statically-matched impl.
    Concrete {
        impl_loc: ImplRef<'db>,
        func_loc: FunctionRef<'db>,
        /// The callee's OWNER frame, carried from resolution: the impl's
        /// generic bindings (declaration order) for a provided method,
        /// `[Self = receiver, iface args..]` for an adopted default. The
        /// call site emits these ahead of the method's own type args per the
        /// `[owner ++ own]` frame invariant — never re-derived by name.
        frame_type_args: Vec<Tir2Ty>,
        /// `true` when `func_loc` is the interface's default body.
        from_interface_default: bool,
    },
}

/// One call's argument/parameter pairing plus its runtime type arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CallPlan {
    pub(crate) bindings: Vec<ParamBinding>,
    /// The value slots the call was checked against, without a bound receiver.
    pub(crate) argument_layout: baml_type::CallLayout,
    /// Full solved owner + callable generic frame, in declared order.
    pub(crate) type_args: Vec<Tir2Ty>,
    pub(crate) own_offset: usize,
    pub(crate) explicit: bool,
    pub(crate) slots: Vec<CallTypeArgPlan>,
    /// Hidden call metadata which is not part of the callee's parameter list.
    pub(crate) side_channels: CallSideChannels,
}

/// One written generic slot as MIR consumes it. Only the WRITTEN shape
/// survives the conversion: inference's solved `ty` decides the call's
/// instantiation before MIR runs, and lowering emits from `emission_ty`
/// alone.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CallTypeArgPlan {
    pub(crate) emission_ty: Tir2Ty,
}

/// One lexical `type T = …` binding: the rigid parameter MIR reserves a
/// frame slot for, and where its runtime type comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScopedTypeBinding {
    pub(crate) name: Name,
    pub(crate) parameter: baml_type::ParamTy,
    pub(crate) source: ScopedTypeSource,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ScopedTypeSource {
    /// `unreflect(expr)`: the operand's `reflect.Type` value.
    Runtime(AstExprId),
    /// A static type, loaded as a template in the enclosing frame.
    Static(Tir2Ty),
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
    type_bindings: FxHashMap<AstStmtId, ScopedTypeBinding>,
    /// Condition expressions the checker marked for truthiness coercion
    /// (`Adjust::Truthy`, B-1563): lowering wraps the operand in the
    /// truthy test so the branch itself stays strict-bool.
    truthy_conditions: FxHashSet<AstExprId>,
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
    pub(crate) fn type_binding(&self, stmt: AstStmtId) -> Option<&ScopedTypeBinding> {
        self.type_bindings.get(&stmt)
    }
    pub(crate) fn truthy_condition(&self, expr: AstExprId) -> bool {
        self.truthy_conditions.contains(&expr)
    }
    pub(crate) fn is_exhaustive_match(&self, expr: AstExprId) -> bool {
        self.exhaustiveness.is_exhaustive(expr)
    }
}

/// Materializes one `InferenceResult` into TIR-shaped tables: interned
/// types to the plain family, the resolution enum variant-for-variant,
/// the path ladder into TIR's three keyings, and truthiness adjustments.
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
        out.expr_types.insert(expr, ty.clone());
    }
    for (&pat, ty) in &result.type_of_pat {
        out.pat_types.insert(pat, ty.clone());
    }
    for (&expr, resolution) in &result.member_resolutions {
        out.resolutions.insert(expr, convert_resolution(resolution));
    }
    for (&expr, path) in &result.path_resolutions {
        if let Some(root) = path.segments.first() {
            out.path_root_types.insert(expr, root.ty.clone());
        }
        for (index, segment) in path.segments.iter().enumerate() {
            out.path_segment_types
                .insert((expr, index), segment.ty.clone());
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
                argument_layout: plan.argument_layout.clone(),
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
                type_args: plan.type_args.iter().map(baml_type::Ty::clone).collect(),
                own_offset: plan.own_offset,
                explicit: plan.explicit,
                slots: plan
                    .slots
                    .iter()
                    .map(|slot| CallTypeArgPlan {
                        emission_ty: slot.emission_ty.clone(),
                    })
                    .collect(),
                side_channels: CallSideChannels {
                    runtime_id: plan.runtime_id,
                },
            },
        );
    }
    out.type_bindings = result
        .type_bindings
        .iter()
        .map(|(&stmt, binding)| (stmt, convert_scoped_type_binding(binding)))
        .collect();
    for (&expr, adjustments) in &result.expr_adjustments {
        for adjustment in adjustments {
            match adjustment.kind {
                hir_infer::Adjust::Truthy => {
                    out.truthy_conditions.insert(expr);
                }
            }
        }
    }
    out.exhaustiveness = MatchExhaustiveness::NonExhaustiveSet(
        result.non_exhaustive_matches.iter().copied().collect(),
    );
    out
}

fn convert_scoped_type_binding(binding: &hir_infer::ScopedTypeBinding) -> ScopedTypeBinding {
    ScopedTypeBinding {
        name: binding.name.clone(),
        parameter: binding.parameter.clone(),
        source: match &binding.source {
            hir_infer::ScopedTypeSource::Runtime(operand) => ScopedTypeSource::Runtime(*operand),
            hir_infer::ScopedTypeSource::Static(ty) => ScopedTypeSource::Static(ty.clone()),
        },
    }
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
        hir_infer::MemberResolution::Method { callee, receiver } => MemberResolution::Method {
            callee: match callee {
                hir_infer::MethodCallee::Inherent(func) => MethodCallee::Inherent(*func),
                hir_infer::MethodCallee::Virtual { interface, method } => MethodCallee::Virtual {
                    iface_loc: *interface,
                    method: method.clone(),
                },
                hir_infer::MethodCallee::Concrete {
                    impl_block,
                    func,
                    frame_type_args,
                    from_interface_default,
                } => MethodCallee::Concrete {
                    impl_loc: *impl_block,
                    func_loc: *func,
                    frame_type_args: frame_type_args.clone(),
                    from_interface_default: *from_interface_default,
                },
            },
            receiver: *receiver,
        },
        hir_infer::MemberResolution::InterfaceVirtualField {
            interface,
            view,
            field_index,
            field,
        } => MemberResolution::InterfaceVirtualField {
            iface_loc: *interface,
            interface: view.clone(),
            field_index: *field_index,
            field: field.clone(),
        },
    }
}
