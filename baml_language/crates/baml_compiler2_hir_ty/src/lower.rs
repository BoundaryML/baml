//! Declaration lowering: type syntax -> interned [`Ty`], with name
//! resolution - the rust-analyzer `TyLoweringContext` analog (S4).
//!
//! ONE syntax surface: everything lowers through span-free `TypeRef`s -
//! signatures, fields, and aliases from ppir's per-item stores, body
//! annotations from ppir's per-body stores (`body_type_refs`). This crate
//! never sees `ast::TypeExpr`. Name
//! resolution mirrors TIR's `resolve_type_in` algorithm exactly - namespace-
//! relative first, then `root.`-absolute, then package-prefixed, then the
//! `$stream` companion fallback - against ppir's canonical `package_items`
//! (which includes synthesized `*$stream` items).
//!
//! Semantics mirrored from TIR's `lower_type_expr` (reference, not a
//! dependency): aliases stay NOMINAL at lowering (expansion is lazy and
//! cycle-guarded, done by consumers through the fact oracle); `T?` is sugar
//! for `T | null`; enum variants read as literal types via the
//! path-resolution fallback; generic arity is enforced by truncate/pad-with-
//! `Error`; `baml.future.Future<V, E>` lowers to the dedicated `Future`
//! kind. Diagnostics are not emitted yet (S17); every failure lowers to the
//! `Error` sentinel.
//!
//! Not yet mirrored (later slices): free-impl method frames (their
//! bodies as inference roots), map-key validation and other diagnostics
//! (S17).

use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc, InterfaceLoc, TypeAliasLoc},
    package::PackageId,
    type_ref::{TypeRefId, TypeRefKind, TypeRefStore},
};
use baml_compiler2_ppir::item_data::MethodOwner;
use baml_type::{
    Freshness, LoweringFunctionParamTy, LoweringInterface, LoweringTy, Name, ParamTy, TyAttr,
    TypeName,
    interned::{InferTy, Ty},
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone)]
enum ResolvedTypeDefinition<'db> {
    Source(Definition<'db>),
    Exported(Box<crate::package_interface::ExportedType>),
}

/// Everything needed to lower type syntax appearing in one file, for one
/// generic frame.
/// One unresolved written type (E0002), anchored at its `TypeRefId`.
///
/// The caller resolves the id through the source map paired with the store
/// passed to the one-shot lowering operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweringDiag {
    pub type_ref: TypeRefId,
    pub kind: LoweringDiagKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweringDiagKind {
    /// `unreflect(...)` appeared where no body scope can own its runtime slot.
    RuntimeTypeHasNoScope,
    /// The written path resolved nowhere (E0002).
    Unresolved {
        name: Name,
        /// "Did you mean" candidates, each a fully qualified `root...` path.
        suggestions: Box<[Name]>,
    },
    /// A map key type provably outside the string domain (B-267's
    /// contract; `checked_map_key` lowers it to the Error sentinel).
    InvalidMapKey { key: baml_type::Ty },
    /// A type error already in the shared vocabulary (projection
    /// determination, existential completeness), carried through the
    /// sink verbatim.
    Projection(Box<crate::diagnostics::TirTypeError>),
    /// Written type-argument count disagrees with the declaration
    /// (E0001's type-arity spelling).
    WrongArgCount {
        name: Name,
        expected: usize,
        got: usize,
    },
    /// A written function TYPE without a `throws` clause in a position
    /// elaboration could not legalize (E0151).
    FnTypeMissingThrows,
}

/// Where a type reference stands - TIR's `TypePosition`. An existential
/// (value) position denotes one complete interface instantiation: omitted
/// defaulted associated types fill eagerly, and a member with neither a
/// written pin nor a default is diagnosed (Rust's E0191-analog). The head
/// of a constraint - a generic bound, an `implements`/`requires` target,
/// a projection qualifier - pins only what it writes. (TIR's third
/// variant, `ConstructorHead`, has no counterpart here: `hir_ty`'s
/// construction road types heads through `infer_object`, not this path.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypePosition {
    Existential,
    ConstraintHead,
    /// The outer function contract supplied to the exact
    /// `reflect.Package.get_function<F>` method. It is otherwise an
    /// existential; only an omitted OUTER `throws` differs, becoming the
    /// runtime wildcard instead of E0151 + `never` recovery.
    ExtractionContract,
}

pub struct LowerCtx<'db> {
    /// A short-lived sink used only by the one-shot diagnostic lowering APIs.
    /// Persistent lowering contexts never retain diagnostics across stores.
    diags: Option<std::cell::RefCell<Vec<LoweringDiag>>>,
    /// The innermost `TypeRefId` currently lowering - the anchor an
    /// unresolved path reports at.
    current_ref: std::cell::Cell<Option<TypeRefId>>,
    db: &'db dyn baml_compiler2_ppir::Db,
    package_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns_context: Vec<Name>,
    /// The flattened generic frame, innermost params last (lookup searches
    /// in reverse so inner frames shadow outer ones).
    generic_params: Vec<ParamTy>,
    /// The concrete `Self` an enclosing impl provides (r-a's
    /// `Resolver::impl_def` self type): set for class-owned and
    /// free-impl method signatures/bodies, `None` for interface owners
    /// (there `Self` is the frame's universal slot 0, found by the
    /// param fallback first).
    self_ty: Option<LoweringTy>,
    /// The impl's written interface target when this scope is an
    /// implements-block (or free-impl) method: the qualifier `Self.Member`
    /// projects through (rustc resolves `Self::Assoc` in an impl via the
    /// impl's trait ref the same way).
    self_impl_target: Option<baml_type::Interface>,
    /// The frame's declared interface bounds (I2's param env): each
    /// param's CONJUNCTION. Projections (`T.Output`) determine their
    /// interface through these.
    bounds: FxHashMap<ParamTy, Vec<baml_type::Interface>>,
    /// Body-local runtime type atoms replaced by their synthesized rigid
    /// parameters. Empty for declaration signatures.
    runtime_type_params: FxHashMap<TypeRefId, ParamTy>,
}

/// A lowering context for type syntax written in `file`, with an empty
/// generic frame.
pub fn lower_ctx_for_file(
    db: &dyn baml_compiler2_ppir::Db,
    file: baml_base::SourceFile,
) -> LowerCtx<'_> {
    let info = baml_compiler2_hir::file_package::file_package(db, file);
    let package_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, info.package.clone()));
    LowerCtx {
        diags: None,
        current_ref: std::cell::Cell::new(None),
        db,
        package_items,
        ns_context: info.namespace_path,
        generic_params: Vec::new(),
        self_ty: None,
        self_impl_target: None,
        bounds: FxHashMap::default(),
        runtime_type_params: FxHashMap::default(),
    }
}

/// A lowering context over an explicit package view - the same pair the
/// file form derives from a `SourceFile`. MIR's declaration-site road
/// (its callers hold `PackageItems` + a namespace path, not always a
/// file).
pub fn lower_ctx_for_package<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns_context: Vec<Name>,
) -> LowerCtx<'db> {
    LowerCtx {
        diags: None,
        current_ref: std::cell::Cell::new(None),
        db,
        package_items,
        ns_context,
        generic_params: Vec::new(),
        self_ty: None,
        self_impl_target: None,
        bounds: FxHashMap::default(),
        runtime_type_params: FxHashMap::default(),
    }
}

impl<'db> LowerCtx<'db> {
    /// The namespace path this context resolves relative names in.
    pub fn namespace_context(&self) -> &[Name] {
        &self.ns_context
    }

    /// Record a written-vs-declared type-argument count disagreement
    /// (`enforce_arity` silently pads/truncates; the sink names it).
    fn record_arity(&self, name: &Name, got: usize, expected: usize) {
        if got == expected {
            return;
        }
        if let (Some(diags), Some(type_ref)) = (&self.diags, self.current_ref.get()) {
            diags.borrow_mut().push(LoweringDiag {
                type_ref,
                kind: LoweringDiagKind::WrongArgCount {
                    name: name.clone(),
                    expected,
                    got,
                },
            });
        }
    }

    fn take_diagnostics(&self) -> Vec<LoweringDiag> {
        self.diags
            .as_ref()
            .map(|cell| std::mem::take(&mut *cell.borrow_mut()))
            .unwrap_or_default()
    }

    #[must_use]
    /// The in-scope rigid params (function + owner generics, `Self` in
    /// interface-owned bodies) - the overlap oracle's `vars` input.
    pub fn generic_params(&self) -> &[ParamTy] {
        &self.generic_params
    }

    #[must_use]
    pub fn with_frame(mut self, frame: Vec<ParamTy>) -> LowerCtx<'db> {
        self.generic_params = frame;
        self
    }
    /// See `LowerCtx::self_ty`.
    #[must_use]
    pub fn with_impl_target(mut self, target: Option<baml_type::Interface>) -> Self {
        self.self_impl_target = target;
        self
    }

    #[must_use]
    pub fn with_self_ty(mut self, self_ty: Option<baml_type::Ty>) -> Self {
        // Owner `Self` types are declaration-side (hole-free) plain types;
        // the widening into the lowering vocabulary is the zero-cost upcast.
        self.self_ty = self_ty.map(Into::into);
        self
    }

    #[must_use]
    pub fn with_bounds(
        mut self,
        bounds: FxHashMap<ParamTy, Vec<baml_type::Interface>>,
    ) -> LowerCtx<'db> {
        self.bounds = bounds;
        self
    }

    // -- Span-free surface (signatures, fields, aliases) ----------------------

    pub fn lower_type_ref(&self, store: &TypeRefStore, id: TypeRefId) -> LoweringTy {
        self.lower_type_ref_at(store, id, TypePosition::Existential)
    }

    /// Lower one type-reference tree and return only the diagnostics produced
    /// by that tree. The sink cannot outlive this call, so arena-local ids can
    /// never leak into diagnostics from a later store.
    pub fn lower_type_ref_with_diagnostics(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
    ) -> (LoweringTy, Vec<LoweringDiag>) {
        self.lower_type_ref_at_with_diagnostics(store, id, TypePosition::Existential)
    }

    /// [`Self::lower_type_ref_with_diagnostics`] at an explicit position.
    pub fn lower_type_ref_at_with_diagnostics(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
        position: TypePosition,
    ) -> (LoweringTy, Vec<LoweringDiag>) {
        self.lower_type_ref_with_overlay_and_diagnostics(store, id, position, &[])
    }

    /// [`Self::lower_type_ref`] at an explicit [`TypePosition`]. The
    /// position applies to the reference's HEAD only; nested references
    /// (generic args, union members, binding values) are existential.
    pub fn lower_type_ref_at(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
        position: TypePosition,
    ) -> LoweringTy {
        let saved = self.current_ref.replace(Some(id));
        let ty = self.lower_type_ref_inner(store, id, position);
        self.current_ref.set(saved);
        ty
    }

    /// Lower one body-owned type through a lexical rigid-parameter overlay.
    /// The declaration frame on this context remains immutable and reusable;
    /// diagnostics produced by the short-lived fork are merged back into this
    /// context's sink.
    pub fn lower_type_ref_with_overlay(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
        position: TypePosition,
        overlay: &[ParamTy],
    ) -> LoweringTy {
        if overlay.is_empty() {
            return self.lower_type_ref_at(store, id, position);
        }
        let fork = self.fork_with_overlay(overlay);
        let ty = fork.lower_type_ref_at(store, id, position);
        self.merge_fork_diagnostics(&fork);
        ty
    }

    /// Diagnostic-producing counterpart to
    /// [`Self::lower_type_ref_with_overlay`]. The returned diagnostics are
    /// scoped to `store` and this call only.
    pub fn lower_type_ref_with_overlay_and_diagnostics(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
        position: TypePosition,
        overlay: &[ParamTy],
    ) -> (LoweringTy, Vec<LoweringDiag>) {
        let fork = self.fork_with_overlay_and_diagnostics(overlay);
        let ty = fork.lower_type_ref_at(store, id, position);
        (ty, fork.take_diagnostics())
    }

    /// Lower a body-owned type through both its lexical name overlay and the
    /// synthesized bindings for nested `unreflect(...)` atoms. Diagnostics
    /// are scoped to this store and lowering operation.
    pub fn lower_type_ref_with_runtime_bindings_and_diagnostics(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
        position: TypePosition,
        overlay: &[ParamTy],
        runtime_type_params: &FxHashMap<TypeRefId, ParamTy>,
    ) -> (LoweringTy, Vec<LoweringDiag>) {
        let mut fork = self.fork_with_overlay_and_diagnostics(overlay);
        fork.runtime_type_params.clone_from(runtime_type_params);
        let ty = fork.lower_type_ref_at(store, id, position);
        (ty, fork.take_diagnostics())
    }
    /// [`Self::lower_type_path`] through the same body-local overlay.
    pub fn lower_type_path_with_overlay(
        &self,
        segments: &[Name],
        overlay: &[ParamTy],
    ) -> LoweringTy {
        if overlay.is_empty() {
            return self.lower_type_path(segments);
        }
        let fork = self.fork_with_overlay(overlay);
        let ty = fork.lower_type_path(segments);
        self.merge_fork_diagnostics(&fork);
        ty
    }

    fn fork_with_overlay(&self, overlay: &[ParamTy]) -> LowerCtx<'db> {
        let mut generic_params = self.generic_params.clone();
        generic_params.extend_from_slice(overlay);
        LowerCtx {
            diags: self
                .diags
                .as_ref()
                .map(|_| std::cell::RefCell::new(Vec::new())),
            current_ref: std::cell::Cell::new(None),
            db: self.db,
            package_items: self.package_items,
            ns_context: self.ns_context.clone(),
            generic_params,
            self_ty: self.self_ty.clone(),
            self_impl_target: self.self_impl_target.clone(),
            bounds: self.bounds.clone(),
            runtime_type_params: self.runtime_type_params.clone(),
        }
    }

    fn fork_with_overlay_and_diagnostics(&self, overlay: &[ParamTy]) -> LowerCtx<'db> {
        let mut fork = self.fork_with_overlay(overlay);
        fork.diags = Some(std::cell::RefCell::new(Vec::new()));
        fork
    }

    fn merge_fork_diagnostics(&self, fork: &LowerCtx<'db>) {
        if let Some(diags) = &self.diags {
            diags.borrow_mut().extend(fork.take_diagnostics());
        }
    }

    fn lower_type_ref_inner(
        &self,
        store: &TypeRefStore,
        id: TypeRefId,
        position: TypePosition,
    ) -> LoweringTy {
        let attr = TyAttr::default;
        let extraction_contract = position == TypePosition::ExtractionContract;
        let position = if extraction_contract {
            TypePosition::Existential
        } else {
            position
        };
        match &store[id].kind {
            TypeRefKind::Unreflect { .. } => {
                if let Some(param) = self.runtime_type_params.get(&id) {
                    LoweringTy::TypeVar(param.clone(), attr())
                } else {
                    if let Some(diags) = &self.diags {
                        diags.borrow_mut().push(LoweringDiag {
                            type_ref: id,
                            kind: LoweringDiagKind::RuntimeTypeHasNoScope,
                        });
                    }
                    LoweringTy::error()
                }
            }
            TypeRefKind::Int => LoweringTy::int(),
            TypeRefKind::Bigint => LoweringTy::Bigint { attr: attr() },
            TypeRefKind::Float => LoweringTy::float(),
            TypeRefKind::String => LoweringTy::string(),
            TypeRefKind::Bool => LoweringTy::bool(),
            TypeRefKind::Null => LoweringTy::null(),
            TypeRefKind::Never => LoweringTy::never(),
            TypeRefKind::Void => LoweringTy::void(),
            TypeRefKind::Uint8Array => LoweringTy::Uint8Array { attr: attr() },
            TypeRefKind::Media { kind } => LoweringTy::Media(*kind, attr()),
            TypeRefKind::Unknown => LoweringTy::Unknown { attr: attr() },
            TypeRefKind::Type => LoweringTy::Type { attr: attr() },
            TypeRefKind::Rust => LoweringTy::RustType { attr: attr() },
            TypeRefKind::Optional { inner } => {
                LoweringTy::optional(self.lower_type_ref(store, *inner))
            }
            TypeRefKind::List { inner } => LoweringTy::list(self.lower_type_ref(store, *inner)),
            TypeRefKind::Map { key, value } => LoweringTy::Map {
                key: Box::new(self.checked_map_key(self.lower_type_ref(store, *key))),
                value: Box::new(self.lower_type_ref(store, *value)),
                attr: attr(),
            },
            TypeRefKind::Union { variants } => LoweringTy::union(
                variants
                    .iter()
                    .map(|variant| self.lower_type_ref(store, *variant)),
            ),
            TypeRefKind::Literal { value } => {
                LoweringTy::Literal(value.clone(), Freshness::Regular, attr())
            }
            TypeRefKind::Function {
                params,
                ret,
                throws,
            } => LoweringTy::Function {
                params: params
                    .iter()
                    .map(|param| LoweringFunctionParamTy {
                        name: param.name.clone(),
                        ty: self.lower_type_ref(store, param.ty),
                        mode: if param.optional {
                            baml_type::FunctionParamMode::Optional
                        } else {
                            baml_type::FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(self.lower_type_ref(store, *ret)),
                // Elaboration rewrites every legal omitted throws into a
                // synthetic effect param; a survivor here is the ILLEGAL
                // position - it recovers as `never` (mirroring TIR) and
                // the sink names it (E0151).
                throws: throws
                    .map(|throws| self.lower_type_ref(store, throws))
                    .unwrap_or_else(|| {
                        if extraction_contract {
                            return LoweringTy::Unknown { attr: attr() };
                        }
                        if let (Some(diags), Some(type_ref)) = (&self.diags, self.current_ref.get())
                        {
                            diags.borrow_mut().push(LoweringDiag {
                                type_ref,
                                kind: LoweringDiagKind::FnTypeMissingThrows,
                            });
                        }
                        LoweringTy::never()
                    })
                    .into(),
                attr: attr(),
            },
            TypeRefKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
            } => {
                let args: Vec<LoweringTy> = generic_args
                    .iter()
                    .map(|arg| self.lower_type_ref(store, *arg))
                    .collect();
                let bindings: Vec<(Name, LoweringTy)> = associated_type_bindings
                    .iter()
                    .map(|binding| (binding.name.clone(), self.lower_type_ref(store, binding.ty)))
                    .collect();
                self.lower_path(segments, args, bindings, position)
            }
            // A projection node: `T.Output` / `(T as I).Output`. The
            // qualifying interface is the written one, or determined from
            // the base's bound conjunction (the unique bound declaring the
            // member). Reduction stays with the fact oracle (I5); lowering
            // just builds the node - the pure-lowering discipline.
            TypeRefKind::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => {
                let base_ty = self.lower_type_ref(store, *base);
                // The `Self`-rooted fast path keeps the impl-target
                // qualifier road (an implements-block method's
                // `Self.Member` projects through the block's WRITTEN
                // interface target).
                if interface.is_none()
                    && let Some(head) = self.projection_interface_for(&base_ty, member)
                {
                    return LoweringTy::AssociatedTypeProjection {
                        base: Box::new(base_ty),
                        interface: Box::new(head),
                        member: member.clone(),
                        attr: attr(),
                    };
                }
                // The qualifier names the interface to project *through* -
                // a constraint shape, not an existential value: written
                // pins participate in validating the projection, unwritten
                // members are neither demanded nor default-filled.
                let explicit = interface.map(|interface| {
                    self.lower_type_ref_at(store, interface, TypePosition::ConstraintHead)
                });
                // Determination needs closed shapes: a hole in the base or the
                // qualifier cannot name an interface. Narrow, and route the
                // failure through the projection's own unresolved diagnostic
                // rather than a panic (the pre-split code materialized the
                // hole and died in the plain algebra).
                let base_plain = baml_type::Ty::try_from(&base_ty);
                let explicit_plain = explicit.as_ref().map(baml_type::Ty::try_from).transpose();
                match (base_plain, explicit_plain) {
                    (Ok(base), Ok(explicit)) => {
                        let lowered = crate::interfaces::lower_projection(
                            self.db,
                            &self.plain_bounds_env(),
                            base,
                            explicit,
                            member.clone(),
                        );
                        self.record_type_errors(lowered.diagnostics);
                        lowered.ty.as_lowering_ty().clone()
                    }
                    // BUG: a hole-based projection (`(Box<_> as I).Item` in a
                    // `let` annotation) errors silently — matching the
                    // pre-split `Determination::InvalidBase` path, which also
                    // pushed no diagnostic. The unsolved-inference diagnostics
                    // thread owes this case a real error; today the only
                    // hole-position diagnostics are the signature/expression
                    // rejections.
                    _ => LoweringTy::error(),
                }
            }
            // `_` lowers to the hole node; consumers apply policy (signatures
            // reject holes, inference instantiates them as fresh table
            // variables) - the rust-analyzer pure-lowering + funnel discipline.
            TypeRefKind::Infer => LoweringTy::infer(),
            // `Missing` is an omitted annotation (a signature must be
            // explicit; the diagnostic arrives with S17), `Error` was
            // already diagnosed at parse time.
            TypeRefKind::Error | TypeRefKind::Missing => LoweringTy::error(),
        }
    }

    /// Resolve a projection's PREFIX as a written type path, with the
    /// sink muted (an unresolvable prefix is not a diagnostic here - the
    /// caller falls through to the ordinary unresolved-path report).
    /// `None` for prefixes that do not name a type, and for enum heads
    /// (an enum's dotted member is a variant, already given first
    /// refusal). Lowered through the full path cascade - so a prefix
    /// that is itself a projection (`IntHolder.Item` of
    /// `IntHolder.Item.Inner`) resolves recursively - at `ConstraintHead`:
    /// an interface-named prefix reaches the caller's projection-base
    /// rejection intact instead of tripping the existential completeness
    /// check first.
    fn probe_projection_prefix(&self, prefix: &[Name]) -> Option<LoweringTy> {
        let resolved = self.resolve_type(prefix);
        if matches!(
            resolved,
            Some(ResolvedTypeDefinition::Source(Definition::Enum(_)))
        ) || matches!(
            resolved,
            Some(ResolvedTypeDefinition::Exported(ref exported))
                if matches!(exported.as_ref(), crate::package_interface::ExportedType::Enum { .. })
        ) {
            return None;
        }
        let saved = self.diags.as_ref().map(|d| d.borrow().len());
        let ty = self.lower_path(prefix, Vec::new(), Vec::new(), TypePosition::ConstraintHead);
        let clean = match (&self.diags, saved) {
            (Some(diags), Some(saved)) => {
                let mut diags = diags.borrow_mut();
                let clean = diags.len() == saved;
                diags.truncate(saved);
                clean
            }
            _ => true,
        };
        (clean && !ty.contains_error()).then_some(ty)
    }

    /// The frame's bound env as the constraint map the projection
    /// determination judges through — the bounds' own (plain) vocabulary.
    fn plain_bounds_env(&self) -> rustc_hash::FxHashMap<ParamTy, Vec<baml_type::Interface>> {
        self.bounds.clone()
    }

    /// Record projection-determination diagnostics through the sink at the
    /// current ref.
    fn record_type_errors(&self, errors: Vec<crate::diagnostics::TirTypeError>) {
        if let (Some(diags), Some(type_ref)) = (&self.diags, self.current_ref.get()) {
            for error in errors {
                diags.borrow_mut().push(LoweringDiag {
                    type_ref,
                    kind: LoweringDiagKind::Projection(Box::new(error)),
                });
            }
        }
    }

    /// The interface a bare projection (`T.Output`) resolves through: for
    /// a type-var base, the UNIQUE bound in its conjunction whose
    /// interface declares `member` (0 or ambiguity is Error - S17's
    /// diagnostic); for an interface-existential base, itself.
    fn projection_interface_for(
        &self,
        base: &LoweringTy,
        member: &Name,
    ) -> Option<LoweringInterface> {
        let declares = |name: &TypeName| {
            crate::interfaces::interface_declares_member(
                self.db,
                name,
                member,
                crate::interfaces::MemberNamespace::Type,
            )
        };
        // The bound conjunctions and the impl target are declaration-side
        // (hole-free) plain constraints; the chosen one widens into the
        // lowering vocabulary at the return.
        let materialize = |target: &baml_type::Interface| {
            LoweringInterface::new(
                target.name.clone(),
                target.generics.iter().map(Into::into).collect(),
                target
                    .associated_types
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.into()))
                    .collect(),
            )
        };
        match base {
            LoweringTy::TypeVar(param, _) => {
                let candidates: Vec<&baml_type::Interface> = self
                    .bounds
                    .get(param)
                    .map(|bounds| {
                        bounds
                            .iter()
                            .filter(|bound| declares(&bound.name))
                            .collect()
                    })
                    .unwrap_or_default();
                match candidates.as_slice() {
                    [only] => Some(materialize(only)),
                    _ => None,
                }
            }
            LoweringTy::Interface(name, args, pins, _) => declares(name)
                .then(|| LoweringInterface::new(name.clone(), args.clone(), pins.clone())),
            // A chained step (`T.Item.Sub`): the previous member's
            // declared bound (`type Item extends J`), realized at its
            // qualifier, is what declares the next member. Realization is
            // an interned-oracle question, so the closed parts narrow at
            // the seam; a hole anywhere in the chain simply fails to
            // determine (the ordinary fall-through).
            LoweringTy::AssociatedTypeProjection {
                base: prev_base,
                interface,
                member: prev_member,
                ..
            } => {
                let target = baml_type::Interface::try_from(interface.as_ref().clone()).ok()?;
                let prev_base = baml_type::Ty::try_from(prev_base.as_ref()).ok()?;
                let bound = crate::impls::realized_assoc_bound_plain(
                    self.db,
                    &target,
                    &prev_base,
                    prev_member,
                )?;
                match &bound {
                    baml_type::Ty::Interface(name, args, pins, _) => declares(name).then(|| {
                        materialize(&baml_type::Interface::new(
                            name.clone(),
                            args.clone(),
                            pins.clone(),
                        ))
                    }),
                    _ => None,
                }
            }
            // A concrete impl subject: `Self.Member` in an implements
            // block projects through the impl's WRITTEN interface target
            // when that target declares the member; the registry's
            // resolved pin reduces it from there.
            _ if Some(base) == self.self_ty.as_ref() => self
                .self_impl_target
                .as_ref()
                .filter(|target| declares(&target.name))
                .map(materialize),
            _ => None,
        }
    }

    /// Map keys are strings by the language's contract: the VM's backing
    /// store assumes it, map literals cannot spell a non-string key, and
    /// `baml.Map`'s docstring states it (B-267). A key type not provably
    /// a subtype of `string` is diagnosed (E0067) - INCLUDING a type
    /// variable: no bound can prove one string-denoting, so `map<K, V>`
    /// could be instantiated at a non-string key (TIR's fail-closed
    /// rule; the stdlib's own `Map<K, V>` never writes a `map<K, V>`
    /// annotation). Inference holes and error recovery stay untouched.
    fn checked_map_key(&self, key: LoweringTy) -> LoweringTy {
        if key.contains_hole() || key.contains_error() {
            return key;
        }
        let facts = crate::facts::Facts::new(self.db);
        let string = baml_type::Ty::String {
            attr: TyAttr::default(),
        };
        // Past the gate the key is hole-free, so the reject fold is a pure
        // narrowing; the judgment stays in the plain vocabulary.
        if !baml_type::normalize::is_subtype(&reject_holes(&key), &string, &facts) {
            // Diagnose but keep the WRITTEN key (TIR's shape): the
            // diagnostic is the enforcement, and downstream surfaces
            // (codegen schemas, renders) still see what the user wrote.
            if let (Some(diags), Some(type_ref)) = (&self.diags, self.current_ref.get()) {
                diags.borrow_mut().push(LoweringDiag {
                    type_ref,
                    kind: LoweringDiagKind::InvalidMapKey {
                        key: reject_holes(&key),
                    },
                });
            }
        }
        key
    }

    // -- Path resolution -------------------------------------------------------

    /// Lowers a resolved (or fallback-resolved) type path. Mirrors TIR's
    /// `lower_path` dispatch and its failure fallbacks: in-scope type var,
    /// then enum-variant reading, then `Error`.
    fn lower_path(
        &self,
        segments: &[Name],
        args: Vec<LoweringTy>,
        bindings: Vec<(Name, LoweringTy)>,
        position: TypePosition,
    ) -> LoweringTy {
        let attr = TyAttr::default;
        let short = segments.last().expect("type paths are never empty");

        // Lexical generic names (including a body-local overlay appended to
        // the frame) shadow nominal type definitions. A multi-segment path
        // rooted in such a name is an associated projection, never a package
        // or namespace path with the same spelling.
        let generic_head = self
            .generic_params
            .iter()
            .rev()
            .find(|param| param.name() == &segments[0]);
        if segments.len() == 1
            && let Some(param) = generic_head
        {
            return LoweringTy::TypeVar(param.clone(), attr());
        }
        if segments.len() > 1
            && args.is_empty()
            && bindings.is_empty()
            && let Some(param) = generic_head
        {
            let mut ty = LoweringTy::TypeVar(param.clone(), attr());
            for member in &segments[1..] {
                if let Some(interface) = self.projection_interface_for(&ty, member) {
                    ty = LoweringTy::AssociatedTypeProjection {
                        base: Box::new(ty),
                        interface: Box::new(interface),
                        member: member.clone(),
                        attr: attr(),
                    };
                    continue;
                }
                // A chained projection over a hole-carrying prefix cannot
                // determine; poison silently (same contract as the inner
                // projection arm).
                let Ok(ty_plain) = baml_type::Ty::try_from(&ty) else {
                    return LoweringTy::error();
                };
                let lowered = crate::interfaces::lower_projection(
                    self.db,
                    &self.plain_bounds_env(),
                    ty_plain,
                    None,
                    member.clone(),
                );
                self.record_type_errors(lowered.diagnostics);
                ty = lowered.ty.as_lowering_ty().clone();
                if matches!(ty, LoweringTy::Error { .. }) {
                    return ty;
                }
            }
            return ty;
        }

        if let Some(def) = self.resolve_type(segments) {
            return match def {
                ResolvedTypeDefinition::Source(def) => {
                    self.lower_definition(def, short, args, bindings, position)
                }
                ResolvedTypeDefinition::Exported(exported) => {
                    self.lower_exported_type(*exported, short, args, bindings, position)
                }
            };
        }

        // Fallback 1: a single segment naming an in-scope generic param
        // (inner frames shadow outer: search in reverse).
        // Fallback 1b: bare `Self` under a concrete impl owner (r-a's
        // resolver-provided self type). Interface frames never reach
        // here - their `Self` is a param, caught by fallback 1.
        if segments.len() == 1
            && segments[0].as_str() == "Self"
            && let Some(self_ty) = &self.self_ty
        {
            return self_ty.clone();
        }

        // Fallback 2: `Enum.Variant` read as a literal type.
        if segments.len() >= 2
            && args.is_empty()
            && bindings.is_empty()
            && let Some(enum_def) = self.resolve_type(&segments[..segments.len() - 1])
        {
            let enum_qtn = match enum_def {
                ResolvedTypeDefinition::Source(Definition::Enum(enum_loc)) => {
                    let enum_data = baml_compiler2_ppir::item_data::enum_data(self.db, enum_loc);
                    enum_data
                        .variants
                        .iter()
                        .any(|variant| &variant.name == short)
                        .then(|| {
                            self.qualify(Definition::Enum(enum_loc), &segments[segments.len() - 2])
                        })
                }
                ResolvedTypeDefinition::Exported(exported) => match *exported {
                    crate::package_interface::ExportedType::Enum { qtn, variants } => variants
                        .iter()
                        .any(|variant| variant == short)
                        .then_some(qtn),
                    _ => None,
                },
                ResolvedTypeDefinition::Source(_) => None,
            };
            if let Some(qtn) = enum_qtn {
                return LoweringTy::EnumVariant(qtn, short.clone(), attr());
            }
        }

        // Fallback 3: an associated projection written as a dotted path
        // (`T.Item`, `Self.Item`, chained `T.Item.Sub`): the head names
        // an in-scope generic param (`Self` is an interface frame's slot
        // 0), and each further segment projects through the interface its
        // base determines (the unique declaring bound; a chained step
        // resolves through the previous member's declared bound).
        let projection_head = || {
            if let Some(param) = self
                .generic_params
                .iter()
                .rev()
                .find(|param| param.name() == &segments[0])
            {
                return Some(LoweringTy::TypeVar(param.clone(), attr()));
            }
            // `Self.Member` under a concrete impl owner: the head is the
            // subject itself; `projection_interface_for`'s concrete-Self
            // arm supplies the impl target as qualifier.
            if segments[0].as_str() == "Self" {
                return self.self_ty.clone();
            }
            None
        };
        if segments.len() > 1 && args.is_empty() && bindings.is_empty() {
            if let Some(head) = projection_head() {
                let mut ty = head;
                for member in &segments[1..] {
                    // The impl-target qualifier road first (`Self.Member`
                    // inside an implements block), then the full
                    // determination (bound conjunctions, requires
                    // closures, chained declared bounds).
                    if let Some(interface) = self.projection_interface_for(&ty, member) {
                        ty = LoweringTy::AssociatedTypeProjection {
                            base: Box::new(ty),
                            interface: Box::new(interface),
                            member: member.clone(),
                            attr: attr(),
                        };
                        continue;
                    }
                    let Ok(ty_plain) = baml_type::Ty::try_from(&ty) else {
                        return LoweringTy::error();
                    };
                    let lowered = crate::interfaces::lower_projection(
                        self.db,
                        &self.plain_bounds_env(),
                        ty_plain,
                        None,
                        member.clone(),
                    );
                    self.record_type_errors(lowered.diagnostics);
                    ty = lowered.ty.as_lowering_ty().clone();
                    if matches!(ty, LoweringTy::Error { .. }) {
                        return ty;
                    }
                }
                return ty;
            }
            // Type-headed prefix (`ArrayIterator.Element`, and the
            // rejected interface-as-base `Iterator.Element` - Rust's
            // E0223): the prefix resolves as an ordinary type path after
            // enum variants had first refusal. Probed with the sink
            // muted so an unresolvable prefix falls through to the
            // ordinary unresolved-path report below.
            if let Some(head) = self.probe_projection_prefix(&segments[..segments.len() - 1]) {
                let member = segments.last().expect("checked len > 1");
                // A hole-carrying prefix cannot host a member projection;
                // degrade to `Error` inside the prefix (the ordinary
                // determination failure diagnostics still apply).
                let head_plain = reject_holes(&head);
                if let Some(iface_qtn) = crate::interfaces::interface_base_without_member_pin(
                    self.db,
                    &head_plain,
                    member,
                ) {
                    self.record_type_errors(vec![
                        crate::diagnostics::TirTypeError::InterfaceProjectionBase {
                            interface: iface_qtn,
                            member: member.clone(),
                        },
                    ]);
                    return LoweringTy::error();
                }
                let lowered = crate::interfaces::lower_projection(
                    self.db,
                    &self.plain_bounds_env(),
                    head_plain,
                    None,
                    member.clone(),
                );
                self.record_type_errors(lowered.diagnostics);
                return lowered.ty.as_lowering_ty().clone();
            }
        }

        if let (Some(diags), Some(type_ref)) = (&self.diags, self.current_ref.get()) {
            diags.borrow_mut().push(LoweringDiag {
                type_ref,
                kind: LoweringDiagKind::Unresolved {
                    name: Name::new(
                        segments
                            .iter()
                            .map(Name::as_str)
                            .collect::<Vec<_>>()
                            .join("."),
                    ),
                    suggestions: self.type_suggestions(segments),
                },
            });
        }
        LoweringTy::error()
    }

    /// "Did you mean" candidates for an unresolved SINGLE-SEGMENT type
    /// name: every namespace that declares the bare name, spelled as its
    /// fully qualified `root...` path (TIR's `type_suggestions`).
    pub(crate) fn type_suggestions(&self, segments: &[Name]) -> Box<[Name]> {
        if segments.len() != 1 {
            return Box::new([]);
        }
        let item = &segments[0];
        let mut suggestions: Vec<String> = Vec::new();
        for (ns_path, ns_items) in &self.package_items.namespaces {
            if ns_items.types.contains_key(item) {
                if ns_path.is_empty() {
                    suggestions.push(format!("root.{item}"));
                } else {
                    let ns_str = ns_path
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    suggestions.push(format!("root.{ns_str}.{item}"));
                }
            }
        }
        suggestions.sort();
        suggestions
            .iter()
            .map(String::as_str)
            .map(Name::new)
            .collect()
    }

    fn lower_definition(
        &self,
        def: Definition<'db>,
        short: &Name,
        mut args: Vec<LoweringTy>,
        bindings: Vec<(Name, LoweringTy)>,
        position: TypePosition,
    ) -> LoweringTy {
        let attr = TyAttr::default;
        match def {
            Definition::Class(class_loc) => {
                let data = baml_compiler2_ppir::item_data::class_data(self.db, class_loc);
                self.record_arity(short, args.len(), data.generic_params.len());
                enforce_arity(&mut args, data.generic_params.len());
                let qtn = self.qualify(def, short);
                let ty = class_lowering_ty(qtn, args);
                // The `baml.Map<K, V>` spelling bridges to the structural
                // map (B-1080), so it gets the same key validation as the
                // `map<k, v>` syntax.
                if let LoweringTy::Map { key, value, attr } = ty {
                    return LoweringTy::Map {
                        key: Box::new(self.checked_map_key(*key)),
                        value,
                        attr,
                    };
                }
                ty
            }
            Definition::Interface(interface_loc) => {
                let data = baml_compiler2_ppir::item_data::interface_data(self.db, interface_loc);
                self.record_arity(short, args.len(), data.generic_params.len());
                enforce_arity(&mut args, data.generic_params.len());
                let qtn = self.qualify(def, short);
                // A binding naming an undeclared member, or re-binding an
                // already-bound one, is diagnosed and dropped - the lowered
                // instantiation carries only the interface's declared shape.
                let mut checked: Vec<(Name, LoweringTy)> = Vec::with_capacity(bindings.len());
                for (name, value) in bindings {
                    if !data.associated_types.iter().any(|assoc| assoc.name == name) {
                        self.record_type_errors(vec![
                            crate::diagnostics::TirTypeError::UnresolvedType {
                                name: name.clone(),
                                suggestions: data
                                    .associated_types
                                    .iter()
                                    .map(|assoc| assoc.name.clone())
                                    .collect(),
                            },
                        ]);
                        continue;
                    }
                    if checked.iter().any(|(seen, _)| *seen == name) {
                        self.record_type_errors(vec![
                            crate::diagnostics::TirTypeError::DuplicateAssociatedTypeBinding {
                                interface: qtn.clone(),
                                name,
                            },
                        ]);
                        continue;
                    }
                    checked.push((name, value));
                }
                let mut bindings = checked;
                // An existential denotes one complete instantiation
                // (BEP-048 1.7(a)): each omitted defaulted member fills
                // eagerly - `Self` is the existential itself, so a
                // Self-referencing default reduces against the pins so
                // far - and a member with neither a written pin nor a
                // default is an error (E0191-analog), its slot recovering
                // as `Ty::Error` so the instantiation keeps the declared
                // shape. A constraint head pins only what it writes.
                // Written pins stay VERBATIM: their values live in the
                // REFERENCING scope's vocabulary, and re-deriving them
                // through the interface's own frame (whose `Self`/slot
                // `ParamTy`s collide with any other frame's) would capture
                // foreign variables. Only the omitted defaults - lowered
                // once in the interface's frame - realize here.
                if position == TypePosition::Existential && !data.associated_types.is_empty() {
                    let iface_params = interface_declared_params(self.db, interface_loc);
                    let self_param = interface_frame(self.db, interface_loc)
                        .first()
                        .cloned()
                        .expect("interface frame starts with Self");
                    // Default realization speaks the finalized vocabulary; a
                    // hole inside an argument degrades to `Error` within the
                    // realized default only (the argument itself stays a hole
                    // in the built node, so inference still fills it).
                    let plain_args: Vec<baml_type::Ty> = args.iter().map(reject_holes).collect();
                    for assoc in &data.associated_types {
                        if bindings.iter().any(|(name, _)| name == &assoc.name) {
                            continue;
                        }
                        let Some((default, _diags)) =
                            crate::interfaces::interface_associated_type_default(
                                self.db,
                                interface_loc,
                                assoc.name.clone(),
                            )
                        else {
                            continue;
                        };
                        let self_ty = baml_type::Ty::Interface(
                            qtn.clone(),
                            plain_args.clone().into(),
                            bindings
                                .iter()
                                .map(|(name, ty)| (name.clone(), reject_holes(ty)))
                                .collect(),
                            attr(),
                        );
                        let realized = crate::interfaces::realize_associated_default(
                            &default,
                            &iface_params,
                            &plain_args,
                            &self_param,
                            &self_ty,
                        );
                        bindings.push((assoc.name.clone(), realized.into()));
                    }
                    let missing: Vec<Name> = data
                        .associated_types
                        .iter()
                        .map(|assoc| assoc.name.clone())
                        .filter(|name| !bindings.iter().any(|(bound, _)| bound == name))
                        .collect();
                    if !missing.is_empty() {
                        for name in &missing {
                            bindings.push((name.clone(), LoweringTy::error()));
                        }
                        self.record_type_errors(vec![
                            crate::diagnostics::TirTypeError::MissingAssociatedTypeBindings {
                                interface: qtn.clone(),
                                missing,
                            },
                        ]);
                    }
                }
                // Sorted for order-insensitive identity.
                bindings.sort_by(|(a, _), (b, _)| a.cmp(b));
                LoweringTy::Interface(qtn, args.into(), bindings.into(), attr())
            }
            Definition::Enum(_) => LoweringTy::Enum(self.qualify(def, short), attr()),
            // Aliases stay nominal at lowering; expansion is lazy and
            // cycle-guarded, through the fact oracle.
            Definition::TypeAlias(_) => LoweringTy::TypeAlias(self.qualify(def, short), attr()),
            // Value-namespace definitions are not types.
            Definition::Function(_)
            | Definition::TemplateString(_)
            | Definition::Client(_)
            | Definition::Test(_)
            | Definition::RetryPolicy(_)
            | Definition::Let(_) => LoweringTy::error(),
        }
    }

    fn lower_exported_type(
        &self,
        exported: crate::package_interface::ExportedType,
        short: &Name,
        mut args: Vec<LoweringTy>,
        bindings: Vec<(Name, LoweringTy)>,
        position: TypePosition,
    ) -> LoweringTy {
        use crate::package_interface::ExportedType;
        let attr = TyAttr::default;
        match exported {
            ExportedType::Class {
                qtn,
                generic_params,
                ..
            } => {
                self.record_arity(short, args.len(), generic_params.len());
                enforce_arity(&mut args, generic_params.len());
                LoweringTy::Class(qtn, args.into_boxed_slice(), attr())
            }
            ExportedType::Enum { qtn, .. } => {
                self.record_arity(short, args.len(), 0);
                self.reject_non_interface_bindings(bindings);
                LoweringTy::Enum(qtn, attr())
            }
            ExportedType::TypeAlias { qtn, .. } => {
                self.record_arity(short, args.len(), 0);
                self.reject_non_interface_bindings(bindings);
                LoweringTy::TypeAlias(qtn, attr())
            }
            ExportedType::Interface {
                qtn,
                self_param,
                generic_params,
                associated_types,
                ..
            } => {
                self.record_arity(short, args.len(), generic_params.len());
                enforce_arity(&mut args, generic_params.len());

                let mut checked = Vec::with_capacity(bindings.len());
                for (name, value) in bindings {
                    if !associated_types.iter().any(|assoc| assoc.name == name) {
                        self.record_type_errors(vec![
                            crate::diagnostics::TirTypeError::UnresolvedType {
                                name,
                                suggestions: associated_types
                                    .iter()
                                    .map(|assoc| assoc.name.clone())
                                    .collect(),
                            },
                        ]);
                        continue;
                    }
                    if checked.iter().any(|(seen, _)| seen == &name) {
                        self.record_type_errors(vec![
                            crate::diagnostics::TirTypeError::DuplicateAssociatedTypeBinding {
                                interface: qtn.clone(),
                                name,
                            },
                        ]);
                        continue;
                    }
                    checked.push((name, value));
                }

                if position == TypePosition::Existential {
                    let plain_args: Vec<_> = args.iter().map(reject_holes).collect();
                    for assoc in &associated_types {
                        if checked.iter().any(|(name, _)| name == &assoc.name) {
                            continue;
                        }
                        let Some(default) = &assoc.default else {
                            continue;
                        };
                        let self_ty = baml_type::Ty::Interface(
                            qtn.clone(),
                            plain_args.clone().into(),
                            checked
                                .iter()
                                .map(|(name, ty)| (name.clone(), reject_holes(ty)))
                                .collect(),
                            attr(),
                        );
                        let realized = crate::interfaces::realize_associated_default(
                            default,
                            &generic_params,
                            &plain_args,
                            &self_param,
                            &self_ty,
                        );
                        checked.push((assoc.name.clone(), realized.into()));
                    }
                    let missing: Vec<Name> = associated_types
                        .iter()
                        .map(|assoc| assoc.name.clone())
                        .filter(|name| !checked.iter().any(|(bound, _)| bound == name))
                        .collect();
                    if !missing.is_empty() {
                        checked.extend(
                            missing
                                .iter()
                                .cloned()
                                .map(|name| (name, LoweringTy::error())),
                        );
                        self.record_type_errors(vec![
                            crate::diagnostics::TirTypeError::MissingAssociatedTypeBindings {
                                interface: qtn.clone(),
                                missing,
                            },
                        ]);
                    }
                }
                checked.sort_by(|(a, _), (b, _)| a.cmp(b));
                LoweringTy::Interface(
                    qtn,
                    args.into_boxed_slice(),
                    checked.into_boxed_slice(),
                    attr(),
                )
            }
        }
    }

    fn reject_non_interface_bindings(&self, bindings: Vec<(Name, LoweringTy)>) {
        for (name, _) in bindings {
            self.record_type_errors(vec![crate::diagnostics::TirTypeError::UnresolvedType {
                name,
                suggestions: Box::new([]),
            }]);
        }
    }

    fn can_access_package(&self, package: &Name) -> bool {
        if &self.package_items.package == package {
            return true;
        }
        baml_compiler2_hir::package::package_dependencies(
            self.db,
            PackageId::new(self.db, self.package_items.package.clone()),
        )
        .iter()
        .any(|dep| dep.name(self.db) == *package)
    }

    /// TIR's `resolve_type_in`, mirrored: (1) namespace-relative in the
    /// current package (no outward walk); (2) `root.`-absolute or
    /// package-prefixed; (3) the `$stream` companion fallback.
    fn resolve_type(&self, segments: &[Name]) -> Option<ResolvedTypeDefinition<'db>> {
        let (item, seg_ns) = segments.split_last().expect("type paths are never empty");

        let relative_ns: Vec<Name> = if self.ns_context.is_empty() {
            seg_ns.to_vec()
        } else {
            self.ns_context.iter().chain(seg_ns).cloned().collect()
        };
        if let Some(def) = self.package_items.lookup_type(&relative_ns, item) {
            return Some(ResolvedTypeDefinition::Source(def));
        }

        if segments.len() >= 2 {
            let prefix_ns = &segments[1..segments.len() - 1];
            if segments[0].as_str() == "root" {
                if let Some(def) = self.package_items.lookup_type(prefix_ns, item) {
                    return Some(ResolvedTypeDefinition::Source(def));
                }
            } else {
                if baml_compiler2_hir::package::is_external_package(self.db, &segments[0]) {
                    if !self.can_access_package(&segments[0]) {
                        return None;
                    }
                    let interface =
                        crate::package_interface::mounted_interface(self.db, &segments[0])?;
                    if let Some(exported) = interface.lookup_type(prefix_ns, item) {
                        return Some(ResolvedTypeDefinition::Exported(Box::new(exported.clone())));
                    }
                }
                let dep_items = baml_compiler2_ppir::package_items(
                    self.db,
                    PackageId::new(self.db, segments[0].clone()),
                );
                if let Some(def) = dep_items.lookup_type(prefix_ns, item) {
                    return Some(ResolvedTypeDefinition::Source(def));
                }
            }
        }

        // `json` is the sole builtin namespace shorthand. After ordinary
        // local/package lookup fails, reinterpret `json.*` under `baml`.
        if segments.first().is_some_and(|root| root.as_str() == "json")
            && self.can_access_package(&Name::new("baml"))
        {
            let baml_package = Name::new("baml");
            let baml_items = baml_compiler2_ppir::package_items(
                self.db,
                PackageId::new(self.db, baml_package.clone()),
            );
            let namespace = &segments[..segments.len() - 1];
            let visible = self.package_items.package == baml_package
                || crate::package_interface::package_interface(
                    self.db,
                    PackageId::new(self.db, baml_package),
                )
                .lookup_type(namespace, item)
                .is_some();
            if visible && let Some(def) = baml_items.lookup_type(namespace, item) {
                return Some(ResolvedTypeDefinition::Source(def));
            }
        }

        // `$stream` companions of classes/aliases resolve through their base
        // name; the caller re-qualifies under the `$stream` name.
        if let Some(base) = item.as_str().strip_suffix("$stream") {
            let mut base_segments = segments.to_vec();
            *base_segments.last_mut().expect("non-empty") = Name::new(base);
            return self.resolve_type(&base_segments).filter(|def| match def {
                ResolvedTypeDefinition::Source(Definition::Class(_) | Definition::TypeAlias(_)) => {
                    true
                }
                ResolvedTypeDefinition::Exported(exported) => matches!(
                    exported.as_ref(),
                    crate::package_interface::ExportedType::Class { .. }
                        | crate::package_interface::ExportedType::TypeAlias { .. }
                ),
                ResolvedTypeDefinition::Source(_) => false,
            });
        }

        None
    }

    /// Value-namespace resolution, mirroring `LowerCtx::resolve_type`'s
    /// algorithm over `lookup_value` (functions, clients, lets). No
    /// `$stream` fallback: companions are functions with their own names.
    pub fn resolve_value(&self, segments: &[Name]) -> Option<Definition<'db>> {
        let (item, seg_ns) = segments.split_last()?;
        let relative_ns: Vec<Name> = if self.ns_context.is_empty() {
            seg_ns.to_vec()
        } else {
            self.ns_context.iter().chain(seg_ns).cloned().collect()
        };
        if let Some(def) = self.package_items.lookup_value(&relative_ns, item) {
            return Some(def);
        }
        if segments.len() >= 2 {
            let prefix_ns = &segments[1..segments.len() - 1];
            if segments[0].as_str() == "root" {
                if let Some(def) = self.package_items.lookup_value(prefix_ns, item) {
                    return Some(def);
                }
            } else {
                let dep_items = baml_compiler2_ppir::package_items(
                    self.db,
                    PackageId::new(self.db, segments[0].clone()),
                );
                if let Some(def) = dep_items.lookup_value(prefix_ns, item) {
                    return Some(def);
                }
            }
        }
        if segments.first().is_some_and(|root| root.as_str() == "json")
            && self.can_access_package(&Name::new("baml"))
        {
            let baml_package = Name::new("baml");
            let baml_items = baml_compiler2_ppir::package_items(
                self.db,
                PackageId::new(self.db, baml_package.clone()),
            );
            let namespace = &segments[..segments.len() - 1];
            let visible = self.package_items.package == baml_package
                || crate::package_interface::package_interface(
                    self.db,
                    PackageId::new(self.db, baml_package),
                )
                .lookup_function(namespace, item)
                .is_some();
            if visible && let Some(def) = baml_items.lookup_value(namespace, item) {
                return Some(def);
            }
        }
        None
    }

    /// Source-less free-function lookup. Kept separate from
    /// [`Self::resolve_value`] so source consumers that require a real loc can
    /// remain explicit; inference tries the source road first, then this one.
    pub fn resolve_exported_value(
        &self,
        segments: &[Name],
    ) -> Option<crate::package_interface::ResolvedFunction> {
        if segments.len() < 2 {
            return None;
        }
        let (package, visible_segments) =
            if segments.first().is_some_and(|root| root.as_str() == "json") {
                // Mirror `resolve_value`'s sole builtin namespace shorthand after
                // ordinary local lookup.
                (Name::new("baml"), segments)
            } else {
                (segments[0].clone(), &segments[1..])
            };
        if !baml_compiler2_hir::package::is_external_package(self.db, &package)
            || !self.can_access_package(&package)
        {
            return None;
        }
        let (item, namespace) = visible_segments.split_last()?;
        let interface = crate::package_interface::mounted_interface(self.db, &package)?;
        let function = interface.lookup_function(namespace, item)?;
        Some(crate::package_interface::resolved_exported_function(
            function,
            Vec::new(),
            Vec::new(),
        ))
    }

    /// Type-namespace resolution, exposed for constructor and member typing.
    pub fn resolve_type_definition(&self, segments: &[Name]) -> Option<Definition<'db>> {
        match self.resolve_type(segments) {
            Some(ResolvedTypeDefinition::Source(def)) => Some(def),
            _ => None,
        }
    }

    pub fn resolve_exported_type_definition(
        &self,
        segments: &[Name],
    ) -> Option<Box<crate::package_interface::ExportedType>> {
        match self.resolve_type(segments) {
            Some(ResolvedTypeDefinition::Exported(exported)) => Some(exported),
            _ => None,
        }
    }

    /// A dotted TYPE path in value position (the `Type` prefix of
    /// `Type.from_json`), resolved exactly as a written annotation path -
    /// classes, enums, aliases, and in-scope generic params - with no
    /// written args or bindings.
    pub fn lower_type_path(&self, segments: &[Name]) -> LoweringTy {
        self.lower_path(segments, Vec::new(), Vec::new(), TypePosition::Existential)
    }

    /// `LowerCtx::qualify`, exposed for constructor typing.
    pub fn qualify_definition(&self, def: Definition<'db>, short: &Name) -> TypeName {
        self.qualify(def, short)
    }

    /// TIR's `qualify_def`: the qualified name comes from the DEFINITION's
    /// file, while the short name is what the user wrote (which is what
    /// keeps `$stream` companions distinct from their base).
    fn qualify(&self, def: Definition<'db>, short: &Name) -> TypeName {
        let info = baml_compiler2_hir::file_package::file_package(self.db, def.file(self.db));
        TypeName::new(info.package, info.namespace_path, short.clone())
    }
}

/// Substitutes generic parameters by FRAME INDEX: `TypeVar(p)` becomes
/// `args[p.index()]` (absent slots stay symbolic). The instantiation fold
/// for call sites and constructors; flag-short-circuited.
pub fn substitute_params(ty: &baml_type::interned::Ty, args: &[baml_type::interned::Ty]) -> Ty {
    use baml_type::interned::TypeFlags;
    if !ty.flags().contains(TypeFlags::HAS_TYPEVAR) {
        return ty.clone();
    }
    if let InferTy::TypeVar(param, _) = ty.kind()
        && let Some(replacement) = args.get(param.index() as usize)
    {
        return replacement.clone();
    }
    Ty::intern(
        ty.kind()
            .map_children(|child| substitute_params(child, args)),
    )
}

/// Generic-arity recovery without diagnostics (S17): extras truncated,
/// missing padded with `Error`.
fn enforce_arity(args: &mut Vec<LoweringTy>, expected: usize) {
    args.truncate(expected);
    while args.len() < expected {
        args.push(LoweringTy::error());
    }
}

// -- Generic frames -----------------------------------------------------------

/// The flattened generic frame for a function: owner generics first (class
/// generics; interfaces prepend `Self` and append associated-type names,
/// mirroring TIR's layout), then the function's own generics, then its
/// synthetic effect params. Indices are absolute frame positions.
pub fn function_generic_frame<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> Vec<ParamTy> {
    let mut frame = Vec::new();
    match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class_loc)) => {
            let data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            extend_frame(&mut frame, data.generic_params.iter().map(|g| &g.name));
        }
        Some(MethodOwner::Interface(interface_loc)) => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, interface_loc);
            extend_frame(&mut frame, &[Name::new("Self")]);
            extend_frame(&mut frame, data.generic_params.iter().map(|g| &g.name));
            let associated: Vec<Name> = data
                .associated_types
                .iter()
                .map(|assoc| assoc.name.clone())
                .collect();
            extend_frame(&mut frame, &associated);
        }
        // Free impls (`implements<T extends I> J for T[]`): the impl's own
        // generics are the owner prefix, mirroring the class arm.
        Some(MethodOwner::FreeImpl(impl_loc)) => {
            let data = baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc);
            if let baml_compiler2_ppir::item_data::ImplSubjectData::Free { generics, .. } =
                &data.subject
            {
                let names: Vec<Name> = generics.iter().map(|param| param.name.clone()).collect();
                extend_frame(&mut frame, &names);
            }
        }
        None => {}
    }
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    extend_frame(&mut frame, &data.user_generic_params);
    extend_frame(&mut frame, &data.synthetic_effect_params);
    frame
}

/// The builtin class spellings that ARE structural types, so a class
/// reference at them denotes the structural kind rather than a nominal
/// `Class`: `baml.future.Future<V, E>` is the dedicated Future kind, and
/// (B-1080) the builtin `baml.Array<T>` / `baml.Map<K, V>` class spellings
/// lower to `List`/`Map`, making every algebra arm relate them for free
/// instead of TIR's one-directional argument-path patch. Keyed on the builtin
/// package specifically: a user-defined `class Array<T>` stays nominal.
///
/// The ONE bridge decision, shared by the two vocabulary-specific
/// constructors below ([`class_lowering_ty`], [`class_ty`]) so the name/arity
/// predicate cannot drift between the lowering chain and the interned
/// inference world.
enum BuiltinStructural {
    Future,
    List,
    Map,
}

fn builtin_structural(qtn: &TypeName, arity: usize) -> Option<BuiltinStructural> {
    if qtn.is_local() || qtn.package().as_str() != "baml" {
        return None;
    }
    if qtn.namespace().len() == 1
        && qtn.namespace()[0].as_str() == "future"
        && qtn.name().as_str() == "Future"
        && arity == 2
    {
        return Some(BuiltinStructural::Future);
    }
    if qtn.namespace().is_empty() {
        if qtn.name().as_str() == "Array" && arity == 1 {
            return Some(BuiltinStructural::List);
        }
        if qtn.name().as_str() == "Map" && arity == 2 {
            return Some(BuiltinStructural::Map);
        }
    }
    None
}

/// The type a class reference denotes in the lowering vocabulary, builtin
/// bridges ([`builtin_structural`]) applied. The single constructor for class
/// types in the lowering chain, shared by annotation lowering and
/// `class_self_ty`.
pub fn class_lowering_ty(qtn: TypeName, mut args: Vec<LoweringTy>) -> LoweringTy {
    let attr = TyAttr::default;
    match builtin_structural(&qtn, args.len()) {
        Some(BuiltinStructural::Future) => {
            let error_ty = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            let value_ty = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            LoweringTy::Future(Box::new(value_ty), Box::new(error_ty), attr())
        }
        Some(BuiltinStructural::List) => {
            let element = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            LoweringTy::List(Box::new(element), attr())
        }
        Some(BuiltinStructural::Map) => {
            let value = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            let key = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            LoweringTy::Map {
                key: Box::new(key),
                value: Box::new(value),
                attr: attr(),
            }
        }
        None => LoweringTy::Class(qtn, args.into(), attr()),
    }
}

/// [`class_lowering_ty`]'s interned-vocabulary twin, for types minted inside
/// inference (instantiated class heads, pattern heads): same bridge decision,
/// handle children.
pub fn class_ty(qtn: TypeName, mut args: Vec<Ty>) -> Ty {
    let attr = TyAttr::default;
    match builtin_structural(&qtn, args.len()) {
        Some(BuiltinStructural::Future) => {
            let error_ty = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            let value_ty = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            Ty::intern(InferTy::Future(value_ty, error_ty, attr()))
        }
        Some(BuiltinStructural::List) => {
            let element = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            Ty::intern(InferTy::List(element, attr()))
        }
        Some(BuiltinStructural::Map) => {
            let value = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            let key = args.pop().unwrap_or_else(|| unreachable!("checked arity"));
            Ty::intern(InferTy::Map {
                key,
                value,
                attr: attr(),
            })
        }
        None => Ty::intern(InferTy::Class(qtn, args.into(), attr())),
    }
}

/// The qualified name a class definition contributes, from its file's
/// package - the definition-side counterpart of `LowerCtx::qualify`.
pub fn class_qualified_name<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> TypeName {
    let package = baml_compiler2_hir::file_package::file_package(db, class.file(db));
    TypeName::new(
        package.package.clone(),
        package.namespace_path,
        baml_compiler2_ppir::item_data::class_data(db, class)
            .name
            .clone(),
    )
}

/// The type of `self` inside `class`: the class applied to its own generic
/// params as `TypeVar`s, through the same builtin bridging as written
/// annotations - so `self` in `baml.Array<T>` is `T[]`, and substituting
/// the receiver's args yields e.g. `int[]` with no per-class special case.
pub fn class_self_ty<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> baml_type::Ty {
    let args: Vec<LoweringTy> = class_generic_frame(db, class)
        .into_iter()
        .map(|param| LoweringTy::TypeVar(param, TyAttr::default()))
        .collect();
    // Hole-free by construction, so the reject fold is a pure narrowing.
    reject_holes(&class_lowering_ty(class_qualified_name(db, class), args))
}

/// A generic frame from bare interface param names (no `Self` slot -
/// the registry's `requires` lowering; the full interface frame with
/// `Self` at index 0 stays `function_generic_frame`'s).
pub fn interface_generic_frame_params(names: &[Name]) -> Vec<ParamTy> {
    let mut frame = Vec::new();
    extend_frame(&mut frame, names);
    frame
}

/// The full interface frame: `[Self, params.., assoc..]` - the positional
/// discipline every interface-scoped type (member signatures, fields,
/// associated-type bounds and defaults) lowers in, instantiated by
/// `interface_instantiation`'s vector of the same shape.
pub fn interface_frame<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: InterfaceLoc<'db>,
) -> Vec<ParamTy> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let mut names = vec![Name::new("Self")];
    names.extend(data.generic_params.iter().map(|g| g.name.clone()));
    names.extend(data.associated_types.iter().map(|assoc| assoc.name.clone()));
    interface_generic_frame_params(&names)
}

/// The interface's OWN declared params - `interface_frame`'s middle
/// section, without `Self` and the associated slots.
pub fn interface_declared_params<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: InterfaceLoc<'db>,
) -> Vec<ParamTy> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    interface_frame(db, interface)[1..=data.generic_params.len()].to_vec()
}

/// The root generic frame for an impl block: the enclosing class's frame
/// for an in-class `implements`, the block's own declared generics for a
/// free `implements ... for T`.
pub fn impl_frame<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Vec<ParamTy> {
    let data = baml_compiler2_ppir::item_data::impl_block_data(db, block);
    match &data.subject {
        baml_compiler2_ppir::item_data::ImplSubjectData::InClass { class, .. } => {
            class_generic_frame(db, *class)
        }
        baml_compiler2_ppir::item_data::ImplSubjectData::Free { generics, .. } => {
            let mut frame = Vec::new();
            ParamTy::extend_frame(
                &mut frame,
                &generics
                    .iter()
                    .map(|param| param.name.clone())
                    .collect::<Vec<_>>(),
            );
            frame
        }
    }
}

/// A free impl block's declared generic bounds (`implements<T extends I>
/// ... for T`), conjunctive per param - the impl-scope param env. In-class
/// impls carry the CLASS's bounds instead (`class_generic_bounds`).
pub fn impl_generic_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> FxHashMap<ParamTy, Vec<baml_type::Interface>> {
    let data = baml_compiler2_ppir::item_data::impl_block_data(db, block);
    match &data.subject {
        baml_compiler2_ppir::item_data::ImplSubjectData::InClass { class, .. } => {
            class_generic_bounds(db, *class)
        }
        baml_compiler2_ppir::item_data::ImplSubjectData::Free { generics, .. } => {
            let frame = impl_frame(db, block);
            let ctx = lower_ctx_for_file(db, block.file(db)).with_frame(frame.clone());
            let mut out = FxHashMap::default();
            for (param, param_data) in frame.iter().zip(generics.iter()) {
                let bounds: Vec<_> = param_data
                    .bounds
                    .iter()
                    .filter_map(|&type_ref| {
                        reject_holes(&ctx.lower_type_ref_at(
                            &data.type_refs,
                            type_ref,
                            TypePosition::ConstraintHead,
                        ))
                        .as_interface()
                    })
                    .collect();
                if !bounds.is_empty() {
                    out.insert(param.clone(), bounds);
                }
            }
            out
        }
    }
}

/// The qualified name of a definition as its own file's package and
/// namespace spell it - the one shared constructor for turning an item
/// resolution back into the type vocabulary's name.
pub fn qualify_def<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    def: baml_compiler2_hir::contributions::Definition<'db>,
    name: &Name,
) -> baml_type::QualifiedTypeName {
    let info = baml_compiler2_hir::file_package::file_package(db, def.file(db));
    baml_type::QualifiedTypeName::new(info.package, info.namespace_path, name.clone())
}

/// The root generic frame for a class.
pub fn class_generic_frame<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<ParamTy> {
    let mut frame = Vec::new();
    extend_frame(
        &mut frame,
        baml_compiler2_ppir::item_data::class_data(db, class)
            .generic_params
            .iter()
            .map(|g| &g.name),
    );
    frame
}

fn extend_frame<'a>(frame: &mut Vec<ParamTy>, names: impl IntoIterator<Item = &'a Name>) {
    for name in names {
        let index = u32::try_from(frame.len()).expect("generic frame index overflow");
        frame.push(ParamTy::new(index, name.clone()));
    }
}

// -- Item queries -------------------------------------------------------------

/// A function's elaborated signature, lowered.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSignature {
    pub generic_params: Vec<ParamTy>,
    pub params: Vec<SignatureParam>,
    pub ret: baml_type::Ty,
    /// The declared clause when written, else the INFERRED effect via
    /// `callable_throws` (S12) - body-derived, fixpoint over mutual
    /// recursion, `never` when nothing throws.
    pub throws: baml_type::Ty,
    /// Whether `throws` was written. The owner's own inference checks its
    /// throw sites against a DECLARED clause (the contract) and ignores an
    /// inferred one (which is derived FROM those sites).
    pub throws_declared: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignatureParam {
    pub name: Name,
    pub ty: baml_type::Ty,
    pub has_default: bool,
}

/// Ruling 4: `_` never infers in declaration signatures. Lowering emits the
/// hole node uniformly; this is the signature-side policy fold that rejects
/// survivors, producing the FINALIZED type (the inference side instead
/// instantiates holes as fresh vars). Every hole becomes the `Error`
/// sentinel — its rejection diagnostic is the position's own (S17 / E0147),
/// not this fold's — with the closed spine around it preserved.
pub fn reject_holes(ty: &LoweringTy) -> baml_type::Ty {
    baml_type::Ty::try_from(ty).unwrap_or_else(|_| fill_holes_as_errors(ty))
}

/// The slow path of [`reject_holes`], reached only for genuinely
/// hole-carrying trees (the fast path is the family's generated narrowing):
/// rebuild with every `Infer` replaced by `Error`. A hand-written walk until
/// the `ty_family!` macro grows per-member child rebuilds.
fn fill_holes_as_errors(ty: &LoweringTy) -> baml_type::Ty {
    let fill_all = |tys: &[LoweringTy]| -> Box<[baml_type::Ty]> {
        tys.iter().map(fill_holes_as_errors).collect()
    };
    let attr = |attr: &TyAttr| attr.clone();
    match ty {
        LoweringTy::Infer { attr: a } => baml_type::Ty::Error { attr: attr(a) },
        LoweringTy::List(inner, a) => {
            baml_type::Ty::List(Box::new(fill_holes_as_errors(inner)), attr(a))
        }
        LoweringTy::Map {
            key,
            value,
            attr: a,
        } => baml_type::Ty::Map {
            key: Box::new(fill_holes_as_errors(key)),
            value: Box::new(fill_holes_as_errors(value)),
            attr: attr(a),
        },
        LoweringTy::Union(members, a) => baml_type::Ty::Union(fill_all(members), attr(a)),
        LoweringTy::Class(name, args, a) => {
            baml_type::Ty::Class(name.clone(), fill_all(args), attr(a))
        }
        LoweringTy::Interface(name, args, assoc, a) => baml_type::Ty::Interface(
            name.clone(),
            fill_all(args),
            assoc
                .iter()
                .map(|(name, ty)| (name.clone(), fill_holes_as_errors(ty)))
                .collect(),
            attr(a),
        ),
        LoweringTy::Function {
            params,
            ret,
            throws,
            attr: a,
        } => baml_type::Ty::Function {
            params: params
                .iter()
                .map(|param| baml_type::FunctionParamTy {
                    name: param.name.clone(),
                    ty: fill_holes_as_errors(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(fill_holes_as_errors(ret)),
            throws: Box::new(fill_holes_as_errors(throws)),
            attr: attr(a),
        },
        LoweringTy::Future(value, error, a) => baml_type::Ty::Future(
            Box::new(fill_holes_as_errors(value)),
            Box::new(fill_holes_as_errors(error)),
            attr(a),
        ),
        LoweringTy::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr: a,
        } => baml_type::Ty::AssociatedTypeProjection {
            base: Box::new(fill_holes_as_errors(base)),
            interface: Box::new(baml_type::Interface::new(
                interface.name.clone(),
                fill_all(&interface.generics),
                interface
                    .associated_types
                    .iter()
                    .map(|(name, ty)| (name.clone(), fill_holes_as_errors(ty)))
                    .collect(),
            )),
            member: member.clone(),
            attr: attr(a),
        },
        // Hole-free leaves: the generated narrowing is total on them.
        other => baml_type::Ty::try_from(other)
            .unwrap_or_else(|_| unreachable!("every hole-carrying shape is matched above")),
    }
}

/// The declared interface bounds for a CLASS's own generic frame - the
/// class arm of [`function_generic_bounds`], for declaration sites that
/// lower class-scoped types with no method in hand.
pub fn class_generic_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> FxHashMap<ParamTy, Vec<baml_type::Interface>> {
    let frame = class_generic_frame(db, class);
    let ctx = lower_ctx_for_file(db, class.file(db)).with_frame(frame.clone());
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    let mut out = FxHashMap::default();
    for (param, declared) in frame.iter().zip(&data.generic_params) {
        let refs: Vec<_> = declared
            .bounds
            .iter()
            .filter_map(|&type_ref| {
                reject_holes(&ctx.lower_type_ref_at(
                    &data.type_refs,
                    type_ref,
                    TypePosition::ConstraintHead,
                ))
                .as_interface()
            })
            .collect();
        if !refs.is_empty() {
            out.insert(param.clone(), refs);
        }
    }
    out
}

/// The declared interface bounds for a function's full generic frame
/// (class prefix + own params; effect params unbounded), keyed by the
/// same `ParamTy` identities `function_generic_frame` assigns.
pub fn function_generic_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> FxHashMap<ParamTy, Vec<baml_type::Interface>> {
    let frame = function_generic_frame(db, function);
    let ctx = lower_ctx_for_file(db, function.file(db)).with_frame(frame.clone());
    let mut out = FxHashMap::default();
    // A lowered constraint head enters the bounds vocabulary through the
    // reject fold: a hole inside a written bound degrades to `Error` (its
    // diagnostic is the WF walk's), never panics.
    let as_ref = |ty: &LoweringTy| reject_holes(ty).as_interface();
    let mut frame_iter = frame.iter();
    match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class)) => {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, class);
            for declared in &class_data.generic_params {
                let Some(param) = frame_iter.next() else {
                    break;
                };
                let refs: Vec<_> = declared
                    .bounds
                    .iter()
                    .filter_map(|&type_ref| {
                        as_ref(&ctx.lower_type_ref_at(
                            &class_data.type_refs,
                            type_ref,
                            TypePosition::ConstraintHead,
                        ))
                    })
                    .collect();
                if !refs.is_empty() {
                    out.insert(param.clone(), refs);
                }
            }
        }
        Some(MethodOwner::Interface(interface)) => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
            // The shared interface param env (`Self` bound, param bounds,
            // associated-slot bounds), keyed by the same frame-prefix
            // identities this function's frame starts with.
            out.extend(interface_scope_bounds(db, interface));
            for _ in 0..(1 + data.generic_params.len() + data.associated_types.len()) {
                frame_iter.next();
            }
        }
        Some(MethodOwner::FreeImpl(impl_loc)) => {
            // The impl's declared bounds (`implements<T extends I> ...`),
            // conjunctive per param, keyed by the frame-prefix identities
            // this function's frame starts with.
            let impl_data = baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc);
            if let baml_compiler2_ppir::item_data::ImplSubjectData::Free { generics, .. } =
                &impl_data.subject
            {
                for param_data in generics {
                    let Some(param) = frame_iter.next() else {
                        break;
                    };
                    let bounds: Vec<_> = param_data
                        .bounds
                        .iter()
                        .filter_map(|&type_ref| {
                            as_ref(&ctx.lower_type_ref_at(
                                &impl_data.type_refs,
                                type_ref,
                                TypePosition::ConstraintHead,
                            ))
                        })
                        .collect();
                    if !bounds.is_empty() {
                        out.insert(param.clone(), bounds);
                    }
                }
            }
        }
        None => {}
    }
    let data = baml_compiler2_ppir::item_data::function_data(db, function);
    for declared in &data.generic_params {
        let Some(param) = frame_iter.next() else {
            break;
        };
        let refs: Vec<_> = declared
            .bounds
            .iter()
            .filter_map(|&type_ref| {
                as_ref(&ctx.lower_type_ref_at(
                    &data.type_refs,
                    type_ref,
                    TypePosition::ConstraintHead,
                ))
            })
            .collect();
        if !refs.is_empty() {
            out.insert(param.clone(), refs);
        }
    }
    out
}

/// The interface scope's param env, keyed by `interface_frame`
/// identities: `Self` (slot 0) bounded by the interface itself at its own
/// params, each generic param's declared bound, and each associated
/// slot's declared bound (I5 consumes those for projections). The single
/// env every interface-scoped lowering shares - member signatures via
/// `function_generic_bounds`, required-method and field lowering, and
/// associated-type bounds/defaults - so `Self.Member` projections resolve
/// their qualifying interface identically everywhere.
pub fn interface_scope_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> FxHashMap<ParamTy, Vec<baml_type::Interface>> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let frame = interface_frame(db, interface);
    let ctx = lower_ctx_for_file(db, interface.file(db)).with_frame(frame.clone());
    // A lowered constraint head enters the bounds vocabulary through the
    // reject fold: a hole inside a written bound degrades to `Error` (its
    // diagnostic is the WF walk's), never panics.
    let as_ref = |ty: &LoweringTy| reject_holes(ty).as_interface();
    let mut out = FxHashMap::default();
    let mut frame_iter = frame.iter();
    if let Some(self_param) = frame_iter.next() {
        let args: Vec<baml_type::Ty> = frame
            .iter()
            .skip(1)
            .take(data.generic_params.len())
            .map(|param| baml_type::Ty::TypeVar(param.clone(), TyAttr::default()))
            .collect();
        // Each associated slot pins to the frame's OWN var, so inside the
        // interface `Self.Member` reduces to that slot (TIR's layout: the
        // member is a frame position, bound per-receiver at impl
        // selection - a default method's projection stays symbolic, never
        // the declared default).
        let pins: Box<[(Name, baml_type::Ty)]> = frame
            .iter()
            .skip(1 + data.generic_params.len())
            .zip(&data.associated_types)
            .map(|(param, assoc)| {
                (
                    assoc.name.clone(),
                    baml_type::Ty::TypeVar(param.clone(), TyAttr::default()),
                )
            })
            .collect();
        out.insert(
            self_param.clone(),
            vec![baml_type::Interface::new(
                interface_qualified_name(db, interface),
                args.into_boxed_slice(),
                pins,
            )],
        );
    }
    for declared in &data.generic_params {
        let Some(param) = frame_iter.next() else {
            break;
        };
        let refs: Vec<_> = declared
            .bounds
            .iter()
            .filter_map(|&type_ref| {
                as_ref(&ctx.lower_type_ref_at(
                    &data.type_refs,
                    type_ref,
                    TypePosition::ConstraintHead,
                ))
            })
            .collect();
        if !refs.is_empty() {
            out.insert(param.clone(), refs);
        }
    }
    for assoc in &data.associated_types {
        let param = frame_iter.next();
        if let (Some(param), Some(type_ref)) = (param, assoc.bound)
            && let Some(bound) = as_ref(&ctx.lower_type_ref_at(
                &data.type_refs,
                type_ref,
                TypePosition::ConstraintHead,
            ))
        {
            out.insert(param.clone(), vec![bound]);
        }
    }
    out
}

/// The qualified name an interface definition contributes.
pub fn interface_qualified_name<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> TypeName {
    let package = baml_compiler2_hir::file_package::file_package(db, interface.file(db));
    TypeName::new(
        package.package,
        package.namespace_path,
        baml_compiler2_ppir::item_data::interface_data(db, interface)
            .name
            .clone(),
    )
}

// SAFETY: PartialEq-driven overwrite, the CallableThrows precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FunctionSignature {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

fn function_signature_cycle_initial<'db>(
    _db: &'db dyn baml_compiler2_ppir::Db,
    _id: salsa::Id,
    _function: FunctionLoc<'db>,
) -> FunctionSignature {
    // The fixpoint seed for the signature/throws/inference cycle (an
    // omitted or partial throws clause reads `callable_throws`, which
    // runs `infer_body`, which reads the signature): a degenerate empty
    // signature; iteration converges to the real one.
    FunctionSignature {
        generic_params: Vec::new(),
        params: Vec::new(),
        ret: baml_type::Ty::error(),
        throws: baml_type::Ty::never(),
        throws_declared: false,
    }
}

/// The concrete `Self` a method's OWNER provides - the class's self
/// type, or a free impl's for-target (lowered in `frame`, whose prefix
/// is the impl's params). `None` for interface owners and free
/// functions: an interface method's `Self` is its frame's universal
/// slot, never a concrete type.
pub fn owner_self_ty<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
    frame: &[baml_type::ParamTy],
) -> Option<baml_type::Ty> {
    match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class)) => Some(class_self_ty(db, class)),
        Some(MethodOwner::FreeImpl(impl_loc)) => {
            let data = baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc);
            match &data.subject {
                baml_compiler2_ppir::item_data::ImplSubjectData::Free { for_target, .. } => {
                    let ctx = lower_ctx_for_file(db, impl_loc.file(db)).with_frame(frame.to_vec());
                    Some(reject_holes(
                        &ctx.lower_type_ref(&data.type_refs, *for_target),
                    ))
                }
                baml_compiler2_ppir::item_data::ImplSubjectData::InClass { class, .. } => {
                    Some(class_self_ty(db, *class))
                }
            }
        }
        _ => None,
    }
}

/// The realized interface target of an implements-block (or free-impl)
/// method: the written reference plus its written `type Member = ...`
/// bindings as pins - the qualifier `Self.Member` projects through in
/// this scope. `None` for plain methods and interface-owned bodies
/// (there `Self` is the frame's slot 0 and bounds qualify).
pub fn owner_impl_target<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
    frame: &[baml_type::ParamTy],
) -> Option<baml_type::Interface> {
    let target = baml_compiler2_ppir::item_data::method_interface_target(db, function).as_ref()?;
    // The impl's bounds ride along: a written `type Member = T.Item`
    // binding must find `Item`'s declaring interface through `T`'s bound.
    let ctx = lower_ctx_for_file(db, function.file(db))
        .with_frame(frame.to_vec())
        .with_bounds(function_generic_bounds(db, function));
    let interface = reject_holes(&ctx.lower_type_ref_at(
        &target.type_refs,
        target.target,
        TypePosition::ConstraintHead,
    ))
    .as_interface()?;
    let mut pins = target
        .associated_type_bindings
        .iter()
        .filter_map(|binding| {
            binding.type_ref.map(|type_ref| {
                (
                    binding.name.clone(),
                    reject_holes(&ctx.lower_type_ref(&target.type_refs, type_ref)),
                )
            })
        })
        .collect::<Vec<_>>();
    for (name, ty) in &interface.associated_types {
        if !pins.iter().any(|(have, _)| have == name) {
            pins.push((name.clone(), ty.clone()));
        }
    }
    Some(baml_type::Interface::new(
        interface.name.clone(),
        interface.generics,
        pins.into_boxed_slice(),
    ))
}

/// TRACKED (S2/S3): the signature firewall - a body edit that leaves the
/// signature unchanged cuts off every caller's re-inference.
/// The check layer's SIGNATURE diagnostic walk: re-lower every written
/// signature type reference with the sink enabled and hand back each
/// unresolved path with its resolved span (the item source map's
/// type-ref spans). Untracked and pure - spans never enter salsa
/// results (r-a's ide-layer discipline).
/// A recorded lowering diagnostic as the shared error vocabulary.
pub fn lowering_diag_error(kind: &LoweringDiagKind) -> crate::diagnostics::TirTypeError {
    use crate::diagnostics::TirTypeError;
    match kind {
        LoweringDiagKind::Unresolved { name, suggestions } => {
            crate::diagnostics::removed_reflect_spelling(name).unwrap_or(
                TirTypeError::UnresolvedType {
                    name: name.clone(),
                    suggestions: suggestions.clone(),
                },
            )
        }
        LoweringDiagKind::WrongArgCount {
            name,
            expected,
            got,
        } => TirTypeError::WrongNumberOfTypeArgs {
            type_name: name.clone(),
            expected: *expected,
            got: *got,
        },
        LoweringDiagKind::InvalidMapKey { key } => {
            TirTypeError::InvalidMapKeyType { key: key.clone() }
        }
        LoweringDiagKind::Projection(error) => (**error).clone(),
        LoweringDiagKind::FnTypeMissingThrows => TirTypeError::FunctionTypeMissingThrows,
        LoweringDiagKind::RuntimeTypeHasNoScope => TirTypeError::RuntimeTypeHasNoScope,
    }
}

fn extend_lowering_diagnostics(
    out: &mut Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)>,
    source_map: &baml_compiler2_hir::type_ref::TypeRefSourceMap,
    diagnostics: Vec<LoweringDiag>,
) {
    out.extend(diagnostics.into_iter().map(|diag| {
        (
            source_map.span(diag.type_ref),
            lowering_diag_error(&diag.kind),
        )
    }));
}

/// Whether a declared throws clause is an open contract: it names `unknown`
/// directly, through a union member, or through a type alias. An open
/// contract deliberately admits any thrown value, so throws-coverage
/// analysis (E0097 extraneous-declaration warnings) does not apply to it.
pub(crate) fn is_open_throws_contract(db: &dyn baml_compiler2_ppir::Db, ty: &Ty) -> bool {
    fn visit(
        facts: &crate::facts::Facts<'_>,
        ty: &Ty,
        seen_aliases: &mut FxHashSet<TypeName>,
    ) -> bool {
        match ty.kind() {
            InferTy::Unknown { .. } => true,
            InferTy::Union(members, _) => members
                .iter()
                .any(|member| visit(facts, member, seen_aliases)),
            InferTy::TypeAlias(name, _) if seen_aliases.insert(name.clone()) => {
                baml_type::normalize::TypeContext::alias_def(facts, name)
                    .map(|target| visit(facts, &Ty::from_plain(&target), seen_aliases))
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    visit(&crate::facts::Facts::new(db), ty, &mut FxHashSet::default())
}

pub fn signature_lowering_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)> {
    use crate::diagnostics::TirTypeError;
    // Required interface methods are signature-only items checked by the
    // interface-scope driver with `Self` and the associated slots in
    // scope; this pass would misreport their `Self.*` references (the
    // same exclusion the pre-S17 signature pass carried).
    if baml_compiler2_ppir::item_data::is_required_interface_method(db, function) {
        return Vec::new();
    }
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let frame = function_generic_frame(db, function);
    let bounds = function_generic_bounds(db, function);
    let concrete_self = owner_self_ty(db, function, &frame);
    let impl_target = owner_impl_target(db, function, &frame);
    let ctx = lower_ctx_for_file(db, function.file(db))
        .with_frame(frame)
        .with_bounds(bounds)
        .with_self_ty(concrete_self)
        .with_impl_target(impl_target);
    // Params/ret/throws lowered from the ELABORATED store: their ids index
    // the elaborated source map, not the written one (the two stores number
    // independently - a raw-map lookup here is an out-of-bounds panic on any
    // function whose elaboration allocates extra refs).
    let elaborated_map =
        baml_compiler2_ppir::item_data::elaborated_function_source_map(db, function);
    // The signature's written positions, each lowered for the sink AND
    // judged for generic-argument well-formedness (rustc's wfcheck:
    // `Box<int>` under `class Box<T extends Named>` reports here).
    let scope_env = function_generic_bounds(db, function);
    let mut out: Vec<(text_size::TextRange, TirTypeError)> = Vec::new();
    let mut lower_and_judge = |type_ref: baml_compiler2_hir::type_ref::TypeRefId| {
        let (lowered, diagnostics) = ctx.lower_type_ref_with_diagnostics(&data.type_refs, type_ref);
        extend_lowering_diagnostics(&mut out, &elaborated_map.type_refs, diagnostics);
        for error in crate::interfaces::type_generic_bound_errors(db, &scope_env, &lowered) {
            out.push((elaborated_map.type_refs.span(type_ref), error));
        }
    };
    for param in &data.params {
        // The unannotated-`self` slot lowers as `Missing` by elaboration;
        // it is not a written reference.
        if param.name.as_str() == "self"
            && matches!(data.type_refs[param.type_ref].kind, TypeRefKind::Missing)
        {
            continue;
        }
        lower_and_judge(param.type_ref);
    }
    if let Some(ret) = data.return_type {
        lower_and_judge(ret);
    }
    if let Some(throws) = data.throws {
        lower_and_judge(throws);
    }
    // A bound must name an interface DIRECTLY (E0145): an alias denotes
    // a type, never the interface itself. `function_generic_bounds`
    // silently skips such bounds; the diagnostic walk names them. Bounds
    // lower from the WRITTEN store, so this walk's spans (and its sink
    // records, taken separately below) use the written source map.
    let source_map = baml_compiler2_ppir::item_data::function_source_map(db, function);
    let func_data = baml_compiler2_ppir::item_data::function_data(db, function);
    for bound in func_data
        .generic_params
        .iter()
        .flat_map(|g| g.bounds.iter())
    {
        let (lowered, diagnostics) = ctx.lower_type_ref_at_with_diagnostics(
            &func_data.type_refs,
            *bound,
            TypePosition::ConstraintHead,
        );
        extend_lowering_diagnostics(&mut out, &source_map.type_refs, diagnostics);
        match &lowered {
            // The compiler-derived builtin interface is a VALUE type,
            // never a bound (E0154).
            LoweringTy::Interface(qtn, ..) if qtn.is_reflect_root_type("AnyFunction") => {
                out.push((
                    source_map.type_refs.span(*bound),
                    TirTypeError::BuiltinInterfaceNotABound {
                        interface: qtn.clone(),
                    },
                ));
            }
            LoweringTy::Interface(..) => {
                for error in bound_binding_violations(
                    db,
                    &func_data.type_refs,
                    *bound,
                    &reject_holes(&lowered),
                ) {
                    out.push((source_map.type_refs.span(*bound), error));
                }
            }
            _ if lowered.contains_error() => {}
            _ => {
                out.push((
                    source_map.type_refs.span(*bound),
                    TirTypeError::GenericBoundNotInterface {
                        bound: reject_holes(&lowered),
                    },
                ));
            }
        }
    }
    out
}

/// An explicit associated binding written on a generic bound (`P extends
/// Parser<Output = V>`) must implement that member's own declared bound
/// (`type Output extends Named`) - the same implements relation the
/// impl-side binding check enforces. Only WRITTEN bindings: a default is
/// the interface's own obligation, checked at its declaration; a value
/// still carrying a type variable resolves at instantiation and fails
/// open. TIR's bound-side check from `lower_generic_param_interface_bounds`.
fn bound_binding_violations(
    db: &dyn baml_compiler2_ppir::Db,
    store: &TypeRefStore,
    bound: TypeRefId,
    lowered: &baml_type::Ty,
) -> Vec<crate::diagnostics::TirTypeError> {
    let TypeRefKind::Path {
        associated_type_bindings,
        ..
    } = &store[bound].kind
    else {
        return Vec::new();
    };
    if associated_type_bindings.is_empty() {
        return Vec::new();
    }
    let baml_type::Ty::Interface(qtn, generics, pins, _) = lowered else {
        return Vec::new();
    };
    let head = baml_type::Interface::new(qtn.clone(), generics.clone(), pins.clone());
    let facts = crate::facts::Facts::new(db);
    let mut out = Vec::new();
    for written in associated_type_bindings {
        // An unknown binding name was diagnosed and dropped by lowering.
        let Some((_, value)) = pins.iter().find(|(name, _)| *name == written.name) else {
            continue;
        };
        if baml_type_runtime::contains_typevar(value) {
            continue;
        }
        let normalized = baml_type::normalize::normalize(value, &facts);
        for declared in baml_type::normalize::TypeContext::associated_type_bound(
            &facts,
            &head,
            written.name.clone(),
        ) {
            if !crate::interfaces::normalized_arg_implements_bound(&facts, &normalized, &declared) {
                out.push(
                    crate::diagnostics::TirTypeError::AssociatedTypeBindingViolatesBound {
                        interface: qtn.clone(),
                        name: written.name.clone(),
                        binding: value.clone(),
                        bound: declared,
                    },
                );
            }
        }
    }
    out
}

/// The check layer's CLASS-declaration diagnostic walk: generic-param
/// bounds re-lowered with the sink under the class frame (unresolved,
/// arity, non-interface and builtin-not-a-bound rules).
pub fn class_lowering_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)> {
    use crate::diagnostics::TirTypeError;
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    let frame = class_generic_frame(db, class);
    let ctx = lower_ctx_for_file(db, class.file(db))
        .with_frame(frame)
        .with_bounds(class_generic_bounds(db, class));
    let source_map = baml_compiler2_ppir::item_data::class_source_map(db, class);
    let mut out = Vec::new();
    // Field annotations: every written field type re-lowers with the sink
    // (unresolved names, wrong arg counts - the pre-S17 structural walk)
    // and is judged for generic-argument well-formedness.
    let scope_env = class_generic_bounds(db, class);
    for field in &data.fields {
        let (lowered, diagnostics) =
            ctx.lower_type_ref_with_diagnostics(&data.type_refs, field.type_ref);
        extend_lowering_diagnostics(&mut out, &source_map.type_refs, diagnostics);
        for error in crate::interfaces::type_generic_bound_errors(db, &scope_env, &lowered) {
            out.push((source_map.type_refs.span(field.type_ref), error));
        }
    }
    for bound in data.generic_params.iter().flat_map(|g| g.bounds.iter()) {
        let (lowered, diagnostics) = ctx.lower_type_ref_at_with_diagnostics(
            &data.type_refs,
            *bound,
            TypePosition::ConstraintHead,
        );
        extend_lowering_diagnostics(&mut out, &source_map.type_refs, diagnostics);
        match &lowered {
            LoweringTy::Interface(qtn, ..) if qtn.is_reflect_root_type("AnyFunction") => {
                out.push((
                    source_map.type_refs.span(*bound),
                    TirTypeError::BuiltinInterfaceNotABound {
                        interface: qtn.clone(),
                    },
                ));
            }
            LoweringTy::Interface(..) => {
                for error in
                    bound_binding_violations(db, &data.type_refs, *bound, &reject_holes(&lowered))
                {
                    out.push((source_map.type_refs.span(*bound), error));
                }
            }
            _ if lowered.contains_error() => {}
            _ => {
                out.push((
                    source_map.type_refs.span(*bound),
                    TirTypeError::GenericBoundNotInterface {
                        bound: reject_holes(&lowered),
                    },
                ));
            }
        }
    }
    out
}

/// The check layer's INTERFACE-declaration diagnostic walk: requires
/// targets re-lowered with the sink under the interface's own frame
/// (unresolved paths, wrong type-arg counts, non-interface targets).
pub fn interface_lowering_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: InterfaceLoc<'db>,
) -> Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)> {
    use crate::diagnostics::TirTypeError;
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let frame = interface_frame(db, interface);
    let ctx = lower_ctx_for_file(db, interface.file(db))
        .with_frame(frame)
        .with_bounds(interface_scope_bounds(db, interface));
    let source_map = baml_compiler2_ppir::item_data::interface_source_map(db, interface);
    let mut out = Vec::new();
    // Field and required-method annotations judge for generic-argument
    // well-formedness in the interface's own scope.
    let scope_env = interface_scope_bounds(db, interface);
    let judge = |type_ref: baml_compiler2_hir::type_ref::TypeRefId,
                 out: &mut Vec<(text_size::TextRange, TirTypeError)>| {
        let (lowered, diagnostics) = ctx.lower_type_ref_with_diagnostics(&data.type_refs, type_ref);
        extend_lowering_diagnostics(out, &source_map.type_refs, diagnostics);
        for error in crate::interfaces::type_generic_bound_errors(db, &scope_env, &lowered) {
            out.push((source_map.type_refs.span(type_ref), error));
        }
    };
    for field in &data.fields {
        judge(field.type_ref, &mut out);
    }
    // Required-method signatures judge through their RESOLVED forms (the
    // one declaration-site lowering, method-own generics in scope) - a
    // re-lowering here would miss the method's own frame and false-fire
    // E0002 on its type parameters.
    {
        let resolved = crate::interfaces::resolve_interface_required_methods(db, interface);
        let scope_env = interface_scope_bounds(db, interface);
        for (index, method) in resolved.iter().enumerate() {
            let mut env = scope_env.clone();
            for (param, bounds) in &method.generic_params {
                env.insert(param.clone(), bounds.clone());
            }
            let span = source_map
                .required_method_spans
                .get(index)
                .map(|sig_map| sig_map.name_span)
                .unwrap_or(source_map.name_span);
            for error in crate::interfaces::type_generic_bound_errors(
                db,
                &env,
                method.function_ty.as_lowering_ty(),
            ) {
                out.push((span, error));
            }
        }
    }
    // Associated-type bounds (`type X extends B`): the same bound rules
    // generic params carry.
    for assoc in &data.associated_types {
        let Some(bound) = assoc.bound else { continue };
        let lowered = ctx.lower_type_ref_at(&data.type_refs, bound, TypePosition::ConstraintHead);
        match &lowered {
            LoweringTy::Interface(qtn, ..) if qtn.is_reflect_root_type("AnyFunction") => {
                out.push((
                    source_map.type_refs.span(bound),
                    TirTypeError::BuiltinInterfaceNotABound {
                        interface: qtn.clone(),
                    },
                ));
            }
            LoweringTy::Interface(..) => {}
            _ if lowered.contains_error() => {}
            _ => {
                out.push((
                    source_map.type_refs.span(bound),
                    TirTypeError::GenericBoundNotInterface {
                        bound: reject_holes(&lowered),
                    },
                ));
            }
        }
    }
    // An associated type's default must implement its declared bound
    // (`type Item extends J = V` requires `V` to implement `J`) - the
    // decl-side analogue of the impl-side binding check, via the same
    // shared bound-satisfaction helper, judged in the interface's own
    // param env (a default naming a bounded declared param satisfies the
    // bound through that param's carried conjunction). A self-referential
    // default keeps `Self` symbolic and fails open through the
    // projection's carried bound.
    {
        let facts = crate::facts::Facts::with_bounds(db, interface_scope_bounds(db, interface));
        let own_params: Vec<baml_type::Ty> = interface_declared_params(db, interface)
            .iter()
            .map(|param| baml_type::Ty::TypeVar(param.clone(), baml_type::TyAttr::default()))
            .collect();
        let head = baml_type::Interface::new(
            interface_qualified_name(db, interface),
            own_params.into(),
            Box::new([]),
        );
        for assoc in &data.associated_types {
            let (Some(default_ref), Some(_)) = (assoc.default, assoc.bound) else {
                continue;
            };
            let Some((default_ty, _diags)) = crate::interfaces::interface_associated_type_default(
                db,
                interface,
                assoc.name.clone(),
            ) else {
                continue;
            };
            let normalized = baml_type::normalize::normalize(&default_ty, &facts);
            for declared in baml_type::normalize::TypeContext::associated_type_bound(
                &facts,
                &head,
                assoc.name.clone(),
            ) {
                if !crate::interfaces::normalized_arg_implements_bound(
                    &facts,
                    &normalized,
                    &declared,
                ) {
                    out.push((
                        source_map.type_refs.span(default_ref),
                        TirTypeError::AssociatedTypeDefaultViolatesBound {
                            interface: interface_qualified_name(db, interface),
                            name: assoc.name.clone(),
                            default: default_ty.clone(),
                            bound: declared,
                        },
                    ));
                }
            }
        }
    }
    // Every interface method — required or default — must declare its
    // `throws` clause explicitly: the signature is a dispatch contract, so
    // it is never inferred (`TYPE_SYSTEM.md` rule 1). E0170.
    for &method in &data.methods {
        let function = baml_compiler2_ppir::item_data::function_data(db, method);
        if function.metadata.is_language_internal {
            continue;
        }
        if function.throws.is_none() {
            out.push((
                baml_compiler2_ppir::item_data::function_source_map(db, method).name_span,
                TirTypeError::InterfaceMethodMissingThrows {
                    interface: interface_qualified_name(db, interface),
                    method: function.name.clone(),
                },
            ));
        }
    }
    // Associated-type BOUNDS re-lower with the sink so unresolved names and
    // arity mistakes in `type A extends …` surface here (the constraint-head
    // position keeps written pins only — a bound never demands the target's
    // associated types be specified).
    for assoc in &data.associated_types {
        if let Some(bound) = assoc.bound {
            let _ = ctx.lower_type_ref_at(&data.type_refs, bound, TypePosition::ConstraintHead);
        }
    }
    // A transitive `requires` graph cycling back to this interface (E0118),
    // reported with the full witnessing name chain.
    if let Some(chain) = crate::interfaces::interface_requires_cycle(db, interface) {
        out.push((
            source_map.name_span,
            TirTypeError::InterfaceRequiresCycle { chain },
        ));
    }
    for &target in &data.requires {
        let (lowered, diagnostics) = ctx.lower_type_ref_at_with_diagnostics(
            &data.type_refs,
            target,
            TypePosition::ConstraintHead,
        );
        extend_lowering_diagnostics(&mut out, &source_map.type_refs, diagnostics);
        if !lowered.contains_error() && !matches!(lowered, LoweringTy::Interface(..)) {
            out.push((
                source_map.type_refs.span(target),
                TirTypeError::InterfaceRequiresNonInterface {
                    interface: interface_qualified_name(db, interface),
                    target: reject_holes(&lowered),
                },
            ));
        }
    }
    out
}

/// A class's fields resolved to plain types (the IDE surface's shape:
/// hover, completions, schema derivation). Field-annotation DIAGNOSTICS
/// ride `class_lowering_diagnostics`; this query is the typed view.
#[salsa::tracked(returns(ref))]
pub fn resolve_class_fields<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<(
    Name,
    baml_type::Ty,
    Vec<baml_compiler2_hir::item_tree::Attribute>,
)> {
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    let ctx = lower_ctx_for_file(db, class.file(db))
        .with_frame(class_generic_frame(db, class))
        .with_bounds(class_generic_bounds(db, class));
    data.fields
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                reject_holes(&ctx.lower_type_ref(&data.type_refs, field.type_ref)),
                field.attributes.clone(),
            )
        })
        .collect()
}

/// The check layer's TYPE-ALIAS diagnostic walk: the alias body re-lowered
/// with the sink (unresolved names, wrong arg counts).
pub fn type_alias_lowering_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    alias: baml_compiler2_hir::loc::TypeAliasLoc<'db>,
) -> Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)> {
    let data = baml_compiler2_ppir::item_data::type_alias_data(db, alias);
    let Some(value) = data.value else {
        return Vec::new();
    };
    let ctx = lower_ctx_for_file(db, alias.file(db));
    let (lowered, diagnostics) = ctx.lower_type_ref_with_diagnostics(&data.type_refs, value);
    let source_map = baml_compiler2_ppir::item_data::type_alias_source_map(db, alias);
    let mut out = Vec::new();
    extend_lowering_diagnostics(&mut out, &source_map.type_refs, diagnostics);
    // The RHS judges for generic-argument well-formedness (aliases are
    // non-generic, so the scope env is empty). The walk expands nested
    // aliases itself; judging the WRITTEN body here keeps one report at
    // the declaration instead of one per use.
    for error in crate::interfaces::type_generic_bound_errors(db, &FxHashMap::default(), &lowered) {
        out.push((source_map.type_refs.span(value), error));
    }
    out
}

#[salsa::tracked(returns(ref), cycle_initial = function_signature_cycle_initial)]
pub fn function_signature<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> FunctionSignature {
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let frame = function_generic_frame(db, function);
    let bounds = function_generic_bounds(db, function);
    // The owner's concrete `Self` (r-a's resolver-provided self type)
    // serves BOTH jobs at once: written `Self` in any signature
    // position resolves through the ctx, and an unannotated `self`
    // receiver (elaboration leaves its slot `Missing`) takes it
    // directly. Interface owners provide none - their `Self` is the
    // frame's universal slot 0, resolved as a param.
    let concrete_self = owner_self_ty(db, function, &frame);
    let impl_target = owner_impl_target(db, function, &frame);
    let ctx = lower_ctx_for_file(db, function.file(db))
        .with_frame(frame.clone())
        .with_bounds(bounds)
        .with_self_ty(concrete_self.clone())
        .with_impl_target(impl_target);
    let owner = baml_compiler2_ppir::item_data::method_owner(db, function);
    let self_ty = |param: &baml_compiler2_ppir::item_data::ElaboratedParamData| {
        if param.name.as_str() != "self"
            || !matches!(data.type_refs[param.type_ref].kind, TypeRefKind::Missing)
        {
            return None;
        }
        match owner {
            Some(MethodOwner::Interface(_)) => Some(baml_type::Ty::TypeVar(
                frame
                    .first()
                    .cloned()
                    .expect("interface frame starts with Self"),
                TyAttr::default(),
            )),
            _ => concrete_self.clone(),
        }
    };
    let params = data
        .params
        .iter()
        .map(|param| SignatureParam {
            name: param.name.clone(),
            ty: self_ty(param).unwrap_or_else(|| {
                reject_holes(&ctx.lower_type_ref(&data.type_refs, param.type_ref))
            }),
            has_default: param.has_default,
        })
        .collect();
    let ret = data
        .return_type
        .map(|ret| reject_holes(&ctx.lower_type_ref(&data.type_refs, ret)))
        .unwrap_or_else(baml_type::Ty::error);
    let throws_declared = data.throws.is_some();
    let throws = data
        .throws
        .map(|throws| {
            let lowered = ctx.lower_type_ref(&data.type_refs, throws);
            if throws_clause_parts(&lowered).1 {
                // A PARTIAL clause (`throws T | _`, spec rule 3): callers
                // see the merged surface (declared + inferred), which is
                // what `callable_throws` computes through the body run.
                crate::callable::callable_throws(db, function).0
            } else {
                reject_holes(&lowered)
            }
        })
        .unwrap_or_else(|| {
            // Omitted: the body-inferred effect, fixpoint over mutual
            // recursion (S12's callable_throws).
            crate::callable::callable_throws(db, function).0
        });
    FunctionSignature {
        generic_params: frame,
        params,
        ret,
        throws,
        throws_declared,
    }
}

/// A class's field types, lowered in the class's own generic frame.
pub fn class_field_types<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ClassLoc<'db>,
) -> Vec<(Name, baml_type::Ty)> {
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    let ctx = lower_ctx_for_file(db, class.file(db)).with_frame(class_generic_frame(db, class));
    data.fields
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                reject_holes(&ctx.lower_type_ref(&data.type_refs, field.type_ref)),
            )
        })
        .collect()
}

/// A type alias's right-hand side, lowered (aliases are non-generic).
pub fn type_alias_value<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    alias: TypeAliasLoc<'db>,
) -> baml_type::Ty {
    let data = baml_compiler2_ppir::item_data::type_alias_data(db, alias);
    let ctx = lower_ctx_for_file(db, alias.file(db));
    data.value
        .map(|value| reject_holes(&ctx.lower_type_ref(&data.type_refs, value)))
        .unwrap_or_else(baml_type::Ty::error)
}

/// One associated type's bound or default, lowered once in the interface
/// frame. Wrapped for the manual `salsa::Update` impl (the
/// `CallableThrows` precedent).
#[derive(Debug, Clone, PartialEq)]
pub struct AssocTypeLowering(pub Option<baml_type::Ty>);

// SAFETY: PartialEq-driven overwrite, the CallableThrows precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for AssocTypeLowering {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

/// The DEFAULT of associated type `member` on `interface` (`type member =
/// ...`), lowered ONCE in the interface frame with `Self` symbolic (frame
/// slot 0) - rustc's discipline for default associated types (the trait
/// definition lowers them with `Self` as an ordinary param); realization
/// at a site is positional substitution via `interface_instantiation`.
/// `None` when `member` is undeclared or has no default.
#[salsa::tracked(returns(ref))]
// A salsa query key must be owned; the by-value `Name` is the contract.
#[allow(clippy::needless_pass_by_value)]
pub fn interface_assoc_default<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: InterfaceLoc<'db>,
    member: Name,
) -> AssocTypeLowering {
    AssocTypeLowering(lower_assoc_type_ref(
        db,
        interface,
        &member,
        TypePosition::Existential,
        |assoc| assoc.default,
    ))
}

/// The declared BOUND of associated type `member` on `interface` (`type
/// member extends J`), lowered once in the interface frame - rustc's
/// `explicit_item_bounds`: what a still-rigid projection is provable
/// against. `None` when `member` is undeclared or unbounded.
#[salsa::tracked(returns(ref))]
// A salsa query key must be owned; the by-value `Name` is the contract.
#[allow(clippy::needless_pass_by_value)]
pub fn interface_assoc_bound<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: InterfaceLoc<'db>,
    member: Name,
) -> AssocTypeLowering {
    AssocTypeLowering(lower_assoc_type_ref(
        db,
        interface,
        &member,
        TypePosition::ConstraintHead,
        |assoc| assoc.bound,
    ))
}

fn lower_assoc_type_ref<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: InterfaceLoc<'db>,
    member: &Name,
    position: TypePosition,
    select: impl Fn(&baml_compiler2_ppir::item_data::AssociatedTypeData) -> Option<TypeRefId>,
) -> Option<baml_type::Ty> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let assoc = data
        .associated_types
        .iter()
        .find(|assoc| assoc.name == *member)?;
    let type_ref = select(assoc)?;
    let ctx = lower_ctx_for_file(db, interface.file(db))
        .with_frame(interface_frame(db, interface))
        .with_bounds(interface_scope_bounds(db, interface));
    Some(reject_holes(&ctx.lower_type_ref_at(
        &data.type_refs,
        type_ref,
        position,
    )))
}

/// Splits a lowered `throws` clause into its named members and whether it
/// carries an open slot (`_`): spec Functions rule 3, `throws T | _`
/// declares the named types AND opens the remainder to inference. The
/// hole is only meaningful as a top-level member; nested holes stay
/// ruling-4 rejections through `reject_holes` on the named part.
pub fn throws_clause_parts(ty: &LoweringTy) -> (baml_type::Ty, bool) {
    let members: Vec<&LoweringTy> = match ty {
        LoweringTy::Union(members, _) => members.iter().collect(),
        _ => vec![ty],
    };
    let mut named = Vec::new();
    let mut open = false;
    for member in members {
        if matches!(member, LoweringTy::Infer { .. }) {
            open = true;
        } else {
            named.push(reject_holes(member));
        }
    }
    let named = match named.len() {
        0 => baml_type::Ty::Never {
            attr: TyAttr::default(),
        },
        1 => named.pop().expect("checked len"),
        _ => baml_type::Ty::Union(named.into(), TyAttr::default()),
    };
    (named, open)
}
