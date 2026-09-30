//! Item builders shared by every emitter: the pooled object of a class, an
//! enum, an interface, or a recursive alias from its declaration; an
//! `implements` block's rule with its references kept as identities; and a
//! package's synthesized functions — a `let` initializer's helper, the
//! `$init` that runs them, the `$init_test` chainer.
//!
//! None of them knows how the emitter lays a pool out. A builder answers with
//! the object; the caller pools it and resolves the identities a rule
//! carries through the package's local-or-import ordinals.

use std::collections::HashSet;

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::{
    body::{FunctionBody, function_body},
    contributions::Definition,
    file_package::file_package,
    item_data::{
        AssociatedTypeBindingData, ClassData, FieldData, InterfaceFieldLinkData, class_data,
        enum_data, function_data, impl_block_data, impl_enclosing_class, interface_data,
        is_required_interface_method, type_alias_data,
    },
    loc::{ClassLoc, DeclRef, EnumLoc, FunctionLoc, ImplLoc, LetLoc, TypeAliasLoc},
    package::located_head,
    type_ref::{TypeRefId, TypeRefStore},
};
use baml_compiler2_hir_ty::{extern_loc::InterfaceRef, layout, lower::qualify_def};
use baml_compiler2_mir::{RuntimeLowering, definition_link_name, lower_let_body, tir2_to_template};
use baml_type::{DeclName, ParamTy, Ty, TypeName, typetag::TypeTag};
use bex_vm_types::{
    Bytecode, Class, ClassField, ConstValue, Enum, EnumVariant, Function, FunctionKind,
    FunctionOrigin, GlobalIndex, ImplBodyCoherence, Instruction, InterfaceBound, ObjectPool,
    SpelledBound, TyTemplate,
    bytecode::{InstructionMeta, OperandMeta},
    types::TypeAliasDef,
};

use crate::{
    ClassFieldSnapshot, LoweringError, MirCodegenContext, OptLevel, emit::compile_mir_function,
    refs::PackageRefs,
};

// ── The slot law ─────────────────────────────────────────────────────────────

/// Whether `func` owns neither a global slot nor a pooled object. A required
/// interface method is signature-only; a `$compiler_intrinsic` /
/// `$await_any` body is never called (its call sites lower to an intrinsic
/// statement / an await-any terminator). Every pass that enumerates
/// functions skips exactly this set — slot assignment, the stdlib splice
/// replay, the reuse gate, rule baking — so their orders agree by
/// construction instead of by four copies of one test.
///
/// The body kind is read from the span-free `function_body` firewall query,
/// not from `lower_function(..).kind`: fully lowering every function just to
/// inspect that one field would double the lowering work.
pub(crate) fn owns_no_slot<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    func: FunctionLoc<'db>,
) -> bool {
    is_required_interface_method(db, func)
        || matches!(
            function_body(db, func).as_ref(),
            FunctionBody::Builtin(kind) if kind.lowers_at_call_site()
        )
}

// ── Declarations → pooled objects ────────────────────────────────────────────

/// The class's runtime fields in slot order: its own declared fields, one
/// slot per name. Interface fields are typed views over class storage
/// (BEP-044) and never add slots. This is the order
/// [`layout::class_fields`] answers; [`build_class_object`] pins the two.
fn runtime_fields<'a>(class: &'a ClassData<'_>) -> Vec<&'a FieldData> {
    let mut seen = HashSet::new();
    class
        .fields
        .iter()
        .filter(|field| seen.insert(field.name.as_str()))
        .collect()
}

/// BEP-042: does the class define a magic `cleanup(self) -> void` finalizer?
///
/// This MUST stay in lockstep with the canonical
/// `cleanup_guard::has_cleanup_shape` (which validates the AST and emits
/// E0144): same shape — one `self` param with no default, no generics,
/// `-> void` return, and no propagating `throws` — on the lowered HIR
/// `Function`. `throws` is effectively-none when absent or `never`, and the
/// return must be `void`, both read off the function's own `TypeRefStore`.
///
/// Only DIRECT class methods count — which is exactly what `class.methods`
/// holds (an `implements`-block method is Impl-owned and never lands
/// there): the AST guard injector and the `{class_fqn}.cleanup` GC
/// resolution only cover direct methods.
fn has_cleanup(db: &dyn baml_compiler2_mir::Db, class: &ClassData<'_>) -> bool {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    class.methods.iter().any(|&method| {
        let func = function_data(db, method);
        let throws_effectively_none = func
            .throws
            .is_none_or(|id| matches!(func.type_refs.get(id).kind, TypeRefKind::Never));
        let returns_void = func
            .return_type
            .is_some_and(|id| matches!(func.type_refs.get(id).kind, TypeRefKind::Void));
        func.name.as_str() == baml_compiler2_ast::cleanup_guard::CLEANUP_METHOD
            && func.generic_params.is_empty()
            && func.params.len() == 1
            && func.params[0].name.as_str() == "self"
            && !func.params[0].has_default
            && throws_effectively_none
            && returns_void
    })
}

/// One class's pooled `Object::Class` from its declaration: the runtime
/// field layout (typed both erased, for codegen, and as templates, for the
/// typed runtime walk), the BEP-042 `cleanup` shape, and under `type_tag`
/// the declaration's own operand, which the linker or grafter replaces with
/// the tag it assigns. The inherent method table is the caller's to fill
/// once the bodies are pooled.
pub(crate) fn build_class_object<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class_loc: ClassLoc<'db>,
    cache: &RuntimeLowering<'_>,
    anchor: &mut PackageRefs<'_, '_>,
    type_tag: TypeTag,
) -> Class {
    let class = class_data(db, class_loc);
    let store = &class.type_refs;
    let wire = cache.wire(&qualify_def(db, Definition::Class(class_loc), &class.name));
    // Class-level generic params, the binding context that resolves
    // `T`-references in field type expressions to `TyTemplate::TypeArgRef(N)`
    // (and to `TypeVar` rather than `Error` on the erased side). When empty,
    // `tir2_to_template` produces a `Concrete`-equivalent leaf for every leaf
    // and `field_template == Concrete(field_type)`.
    let class_generic_params = baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);
    let runtime_fields = runtime_fields(class);
    // The slot order pooled here is the order MIR bakes field indices
    // against, read through `hir_ty::layout` — one declaration, one layout;
    // pinned where both are in hand.
    debug_assert!(
        runtime_fields
            .iter()
            .map(|field| field.name.as_str())
            .eq(layout::class_fields(db, DeclRef::Source(class_loc))
                .iter()
                .map(|(name, _)| name.as_str())),
        "emit's pooled field order for `{wire}` differs from the declaration's layout"
    );
    let fields = runtime_fields
        .iter()
        .map(|field| {
            let tir_ty = baml_compiler2_hir_ty::lower::reject_holes(
                &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, class_loc.file(db))
                    .with_frame(class_generic_params.clone())
                    .lower_type_ref(store, field.type_ref),
            );
            let field_type = cache.convert(&tir_ty);
            let field_template =
                anchor.anchor_template(&tir2_to_template(&tir_ty, cache, &class_generic_params));
            ClassField {
                name: field.name.to_string(),
                field_type: anchor.anchor_runtime_ty(&field_type),
                field_template,
                description: field.attrs.schema.description.clone(),
                alias: field.attrs.schema.alias.clone(),
                docstring: field.docstring.clone(),
                // Source attributes are a fixed vocabulary, all of it typed;
                // `other` is the runtime class builder's.
                other: indexmap::IndexMap::new(),
                skip: field.attrs.skip,
                stream_done: field.attrs.stream_done,
                must_exist: field.attrs.must_exist,
                runtime_type: None,
            }
        })
        .collect();
    Class {
        name: bex_vm_types::DeclarationName::Declared(wire),
        fields,
        description: class.attrs.schema.description.clone(),
        alias: class.attrs.schema.alias.clone(),
        docstring: class.docstring.clone(),
        other: indexmap::IndexMap::new(),
        stream_done: class.attrs.stream_done,
        type_tag,
        has_cleanup: has_cleanup(db, class),
        methods: indexmap::IndexMap::new(),
        generic_param_count: class.generic_params.len(),
        owner: bex_vm_types::types::Owner::anonymous(),
    }
}

/// One enum's pooled `Object::Enum` from its declaration, its variants in
/// declaration (= discriminant) order, under `type_tag`.
pub(crate) fn build_enum_object<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    enum_loc: EnumLoc<'db>,
    cache: &RuntimeLowering<'_>,
    type_tag: TypeTag,
) -> Enum {
    let enm = enum_data(db, enum_loc);
    let variants = enm
        .variants
        .iter()
        .map(|variant| EnumVariant {
            name: variant.name.to_string(),
            description: variant.attrs.schema.description.clone(),
            alias: variant.attrs.schema.alias.clone(),
            docstring: variant.docstring.clone(),
            other: indexmap::IndexMap::new(),
            skip: variant.attrs.skip,
        })
        .collect();
    Enum {
        name: bex_vm_types::DeclarationName::Declared(cache.wire(&qualify_def(
            db,
            Definition::Enum(enum_loc),
            &enm.name,
        ))),
        type_tag,
        variants,
        description: enm.attrs.schema.description.clone(),
        alias: enm.attrs.schema.alias.clone(),
        docstring: enm.docstring.clone(),
        other: indexmap::IndexMap::new(),
        owner: bex_vm_types::types::Owner::anonymous(),
    }
}

/// One RECURSIVE alias's pooled `Object::TypeAlias`: its unfolded value as
/// the runtime carries it, under `type_tag`. Non-recursive aliases are
/// expanded at lowering and never pooled; the caller has already decided
/// this one survives.
///
/// # Errors
///
/// An alias body that lowered to a non-realized type: aliases have no
/// type-parameter list, so nothing is in scope for the right-hand side to
/// reference, and a type variable there is a compiler bug, not a program to
/// carry.
pub(crate) fn build_alias_object<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    alias_loc: TypeAliasLoc<'db>,
    cache: &RuntimeLowering<'_>,
    anchor: &mut PackageRefs<'_, '_>,
    type_tag: TypeTag,
) -> Result<TypeAliasDef, LoweringError> {
    let alias_data = type_alias_data(db, alias_loc);
    let head = qualify_def(db, Definition::TypeAlias(alias_loc), &alias_data.name);
    debug_assert!(
        cache.aliases.recursive.contains(&head),
        "only recursive aliases are pooled"
    );
    let wire = cache.wire(&head);
    let value = cache.convert(&cache.aliases.aliases[&head]);
    let definition = baml_compiler2_mir::RealizedTy::try_from(&value).map_err(|e| {
        LoweringError::Internal(format!(
            "type alias `{}` lowered to a non-realized type (`{}`); aliases cannot be \
             generic, so this is a compiler bug",
            wire.render_dotted(false),
            e.variant,
        ))
    })?;
    Ok(TypeAliasDef {
        name: wire,
        type_tag,
        definition: anchor.anchor_realized(&definition),
        owner: bex_vm_types::HeapPtr::null(),
    })
}

/// Build the runtime [`InterfaceDef`](bex_vm_types::types::InterfaceDef) signature
/// — generic-param bounds, `requires`, associated-type bounds, fields, and method
/// signatures — for one interface, from its item-tree declaration.
///
/// Type expressions are lowered in the interface's own scope (so its generic
/// params resolve to `TypeVar`s) and narrowed to runtime types via
/// [`baml_type::lower_to_runtime`] — the faithful converter that preserves type
/// vars for reflection, NOT the value-erasing `convert_tir_ty_for_runtime`. Bound
/// and `requires` targets become [`baml_type::RuntimeInterface`] (via
/// `RuntimeInterface::new`, which canonicalizes associated-binding order).
pub(crate) fn build_interface_def(
    db: &dyn baml_compiler2_mir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
    iface_tn: &DeclName,
    // The declaration's own tag as the caller's lane states it.
    type_tag: TypeTag,
    resolved: &RuntimeLowering<'_>,
    anchor: &mut PackageRefs<'_, '_>,
) -> bex_vm_types::types::InterfaceDef {
    use baml_compiler2_hir::item_data::FunctionParamData;
    use baml_compiler2_mir::RuntimeTy;
    type RuntimeInterface = baml_type::RuntimeInterface<DeclName>;
    use bex_vm_types::types::{InterfaceDef, InterfaceFieldDef, InterfaceMethodDef};

    /// A method signature at the compiler's head, before anchoring.
    struct NamedMethod {
        name: Name,
        args: Vec<RuntimeTy>,
        kwargs: Vec<(Name, RuntimeTy)>,
        returns: RuntimeTy,
        errors: RuntimeTy,
    }

    let file = iface_loc.file(db);
    let iface_wire = resolved.wire(iface_tn);
    let interface = interface_data(db, iface_loc);
    let generics = &interface.generic_params;
    let interface_frame_params = baml_compiler2_hir_ty::lower::interface_frame(db, iface_loc);
    // The interface scope's own param env: `Self` (slot 0) bounded by the
    // interface itself plus declared param bounds - the single env every
    // interface-scoped lowering shares (associated types are not slots), so
    // `Self.Item` projections resolve here exactly as in signature lowering.
    let decl_ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
        .with_frame(interface_frame_params.clone())
        .with_bounds(baml_compiler2_hir_ty::lower::interface_scope_bounds(
            db, iface_loc,
        ));

    // Lower a type ref in `ctx` and narrow to a runtime type. Diagnostics are
    // not collected: emit only runs on a checked program, so the declaration
    // is already validated. `lower_to_runtime` rejects only the
    // error-recovery sentinels, which a checked program cannot contain, so a
    // failure means a compiler bug and must not be papered over by dropping
    // the entry (that would renumber positional arguments).
    let lower_rt = |ctx: &baml_compiler2_hir_ty::lower::LowerCtx<'_>,
                    store: &TypeRefStore,
                    id: TypeRefId|
     -> RuntimeTy {
        let ty = baml_compiler2_hir_ty::lower::reject_holes(&ctx.lower_type_ref(store, id));
        baml_type::lower_to_runtime(&ty, resolved.aliases).unwrap_or_else(|e| {
            unreachable!("interface `{iface_wire}` declares a non-runtime type: {e:?}")
        })
    };
    // Lower an interface bound / `requires` target / associated-type bound.
    // These are constraint heads: hir keeps written pins only (no eager
    // default realization). A target that is not an interface was rejected
    // upstream (E0145 / E0133) and yields `None`.
    let lower_iface = |ctx: &baml_compiler2_hir_ty::lower::LowerCtx<'_>,
                       store: &TypeRefStore,
                       id: TypeRefId|
     -> Option<RuntimeInterface> {
        // ConstraintHead, per the contract above: the default (existential)
        // position eagerly realizes omitted associated defaults and fills
        // the rest with Error sentinels — a bound like `type A extends
        // Iface` with unpinned associated types would panic `to_runtime`.
        let lowered = baml_compiler2_hir_ty::lower::reject_holes(&ctx.lower_type_ref_at(
            store,
            id,
            baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
        ));
        let baml_type::Ty::Interface(qtn, args, assoc) = lowered else {
            return None;
        };
        let to_runtime = |t: &baml_type::Ty| {
            baml_type::lower_to_runtime(t, resolved.aliases).unwrap_or_else(|e| {
                unreachable!("interface `{iface_wire}` declares a non-runtime constraint: {e:?}")
            })
        };
        let generics = args.iter().map(to_runtime).collect();
        let associated_types = assoc
            .iter()
            .map(|(n, t)| (n.clone(), to_runtime(t)))
            .collect();
        Some(RuntimeInterface::new(qtn, generics, associated_types))
    };
    // A method's runtime signature: Required params -> positional `args`,
    // optional (defaulted) params -> `kwargs`; the `self` receiver is
    // dropped; absent return/throws lower to `Void`.
    let build_method = |ctx: &baml_compiler2_hir_ty::lower::LowerCtx<'_>,
                        store: &TypeRefStore,
                        name: &Name,
                        params: &[FunctionParamData],
                        return_type: Option<TypeRefId>,
                        throws: Option<TypeRefId>|
     -> NamedMethod {
        // An untyped parameter is a syntax-level error, so it cannot reach
        // emit; the top type keeps the positional layout intact if one did.
        let unannotated = || RuntimeTy::Unknown;
        let void = || RuntimeTy::Void;
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        for p in params {
            if p.name.as_str() == "self" {
                continue;
            }
            let ty = p
                .type_ref
                .map_or_else(unannotated, |id| lower_rt(ctx, store, id));
            if p.has_default {
                kwargs.push((p.name.clone(), ty));
            } else {
                args.push(ty);
            }
        }
        NamedMethod {
            name: name.clone(),
            args,
            kwargs,
            returns: return_type.map_or_else(void, |id| lower_rt(ctx, store, id)),
            errors: throws.map_or_else(void, |id| lower_rt(ctx, store, id)),
        }
    };

    // `T extends A & B` is a conjunction; every conjunct that resolves to an
    // interface is emitted so the runtime enforces all of them.
    let store = &interface.type_refs;
    let args: Vec<(Name, Vec<RuntimeInterface>)> = generics
        .iter()
        .map(|declared| {
            let bounds = declared
                .bounds
                .iter()
                .filter_map(|&id| lower_iface(&decl_ctx, store, id))
                .collect();
            (declared.name.clone(), bounds)
        })
        .collect();
    let requires: Vec<RuntimeInterface> = interface
        .requires
        .iter()
        .filter_map(|&id| lower_iface(&decl_ctx, store, id))
        .collect();
    let assoc: Vec<(Name, RuntimeInterface)> = interface
        .associated_types
        .iter()
        .filter_map(|at| {
            at.bound
                .and_then(|id| lower_iface(&decl_ctx, store, id))
                .map(|ri| (at.name.clone(), ri))
        })
        .collect();
    // This list is the interface's field *index space*: `RuntimeImplRule::field_links`
    // is baked parallel to it, so every declared field keeps its slot. A field always
    // carries a type — an untyped one is a syntax-level error that cannot reach emit.
    let fields: Vec<(Name, RuntimeTy)> = interface
        .fields
        .iter()
        .map(|f| (f.name.clone(), lower_rt(&decl_ctx, store, f.type_ref)))
        .collect();
    let mut methods: Vec<NamedMethod> = interface
        .required_methods
        .iter()
        .map(|m| {
            let mut scope = interface_frame_params.clone();
            let own_names: Vec<Name> = m.generic_params.iter().map(|p| p.name.clone()).collect();
            ParamTy::extend_frame(&mut scope, &own_names);
            let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
                .with_frame(scope)
                .with_bounds(baml_compiler2_hir_ty::lower::interface_scope_bounds(
                    db, iface_loc,
                ));
            build_method(&ctx, store, &m.name, &m.params, m.return_type, m.throws)
        })
        .collect();
    methods.extend(interface.default_methods.iter().map(|&loc| {
        let f = function_data(db, loc);
        // The method's full frame and bounds (its own params plus the
        // interface scope's, `function_generic_bounds`' interface arm).
        let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, loc.file(db))
            .with_frame(baml_compiler2_hir_ty::lower::function_generic_frame(
                db, loc,
            ))
            .with_bounds(baml_compiler2_hir_ty::lower::function_generic_bounds(
                db, loc,
            ));
        build_method(
            &ctx,
            &f.type_refs,
            &f.name,
            &f.params,
            f.return_type,
            f.throws,
        )
    }));

    // Everything lowered at the compiler's head; anchored once, here, through
    // the caller's lane.
    InterfaceDef {
        type_tag,
        name: iface_wire,
        args: args
            .into_iter()
            .map(|(name, bounds)| {
                let bounds = bounds
                    .iter()
                    .map(|bound| anchor.anchor_interface(bound))
                    .collect();
                (name, bounds)
            })
            .collect(),
        requires: requires
            .iter()
            .map(|bound| anchor.anchor_interface(bound))
            .collect(),
        assoc: assoc
            .into_iter()
            .map(|(name, bound)| (name, anchor.anchor_interface(&bound)))
            .collect(),
        fields: fields
            .into_iter()
            .map(|(name, ty)| InterfaceFieldDef {
                name,
                ty: anchor.anchor_runtime_ty(&ty),
            })
            .collect(),
        methods: methods
            .into_iter()
            .map(|method| InterfaceMethodDef {
                name: method.name,
                args: method
                    .args
                    .iter()
                    .map(|ty| anchor.anchor_runtime_ty(ty))
                    .collect(),
                kwargs: method
                    .kwargs
                    .into_iter()
                    .map(|(name, ty)| (name, anchor.anchor_runtime_ty(&ty)))
                    .collect(),
                returns: anchor.anchor_runtime_ty(&method.returns),
                errors: anchor.anchor_runtime_ty(&method.errors),
                // Functions are pooled after interfaces, so the default's
                // object index is not known yet; the caller back-fills it once
                // the pool is complete.
                default: None,
                default_fn: bex_vm_types::HeapPtr::null(),
            })
            .collect(),
        // The loader assigns a static declaration's package.
        owner: bex_vm_types::HeapPtr::null(),
    }
}

// ── `implements` blocks → rules ──────────────────────────────────────────────

/// Generic-parameter bound sets, keyed by the declared parameter.
type ImplBoundsMap = rustc_hash::FxHashMap<ParamTy, Vec<baml_type::Interface>>;

/// A lowered interface type split into its head plus its args / associated
/// bindings as `TyTemplate`s (generic params → `TypeArgRef`), at the
/// compiler's head.
struct IfaceParts {
    head: DeclName,
    wire: TypeName,
    args: Vec<baml_compiler2_mir::TyTemplate>,
    assoc: Vec<(Name, baml_compiler2_mir::TyTemplate)>,
}

/// `None` when `iface_ty` is not an interface type.
fn split_interface(
    iface_ty: &Ty,
    resolved: &RuntimeLowering<'_>,
    generics: &[ParamTy],
) -> Option<IfaceParts> {
    let Ty::Interface(head, args, assoc) = iface_ty else {
        return None;
    };
    let template = |t: &Ty| tir2_to_template(t, resolved, generics);
    Some(IfaceParts {
        head: head.clone(),
        wire: resolved.wire(head),
        args: args.iter().map(template).collect(),
        assoc: assoc
            .iter()
            .map(|(name, t)| (name.clone(), template(t)))
            .collect(),
    })
}

/// The lowered head of one `implements` block's baked rule — the pieces that
/// identify the rule. `(interface, for_ty_pattern, interface_args)` is the
/// rule's COHERENCE KEY (unique per interface for accepted programs).
///
/// Shared by [`bake_impl_rule`] (which bakes the rule from it) and the
/// decomposition's rule-attribution replay (which pairs each baked rule back
/// to its declaring file through the same key), so the two can never drift.
pub(crate) struct ImplRuleTarget<'db> {
    /// The implemented interface, wherever it is declared.
    pub(crate) interface: InterfaceRef<'db>,
    /// The interface head as the wire spells it.
    pub(crate) iface_tn: TypeName,
    /// The full lowered target (`Ty::Interface`), for the bake's
    /// argument/associated-binding work.
    iface_ty: Ty,
    interface_args: Vec<baml_compiler2_mir::TyTemplate>,
    /// The target's WRITTEN associated pins only (block-level pins are folded
    /// in by the bake).
    interface_assoc: Vec<(Name, baml_compiler2_mir::TyTemplate)>,
    for_ty: Ty,
    for_ty_pattern: baml_compiler2_mir::TyTemplate,
    impl_params: Vec<ParamTy>,
    impl_bounds: ImplBoundsMap,
    /// Each declared param's bound conjunction, in frame order, each
    /// conjunction canonically sorted — the constraint-set third of the
    /// rule's coherence identity. The bake anchors exactly this, in this
    /// order, so a rule and its declaring block cannot disagree on it.
    generic_param_bounds: Vec<Vec<TargetBound>>,
}

/// One interface bound of an impl's declared parameter, at the compiler's
/// head.
#[derive(Clone)]
pub(crate) struct TargetBound {
    interface: DeclName,
    args: Vec<baml_compiler2_mir::TyTemplate>,
    assoc: Vec<(Name, baml_compiler2_mir::TyTemplate)>,
}

impl TargetBound {
    /// The bound located from `body_root`: its wire-key form.
    fn located(&self, db: &dyn baml_compiler2_hir::Db, body_root: SourceRoot) -> SpelledBound {
        let mut locate = |decl: &DeclName| located_head(db, body_root, decl);
        SpelledBound {
            interface: locate(&self.interface),
            args: self
                .args
                .iter()
                .map(|arg| arg.map_heads(&mut locate))
                .collect(),
            assoc: self
                .assoc
                .iter()
                .map(|(name, ty)| (name.clone(), ty.map_heads(&mut locate)))
                .collect(),
        }
    }
}

impl ImplRuleTarget<'_> {
    /// The block's identity as a wire key (see [`ImplCoherenceKey`]'s
    /// invariant, which this carries): every head located from the block's
    /// own package by edge path, so every artifact of the package states the
    /// same key and no name a program gives a package takes part.
    pub(crate) fn coherence_key(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        body_root: SourceRoot,
    ) -> ImplBodyCoherence {
        let mut locate = |decl: &DeclName| located_head(db, body_root, decl);
        ImplBodyCoherence {
            for_ty_pattern: self.for_ty_pattern.map_heads(&mut locate),
            interface_args: self
                .interface_args
                .iter()
                .map(|arg| arg.map_heads(&mut locate))
                .collect(),
            generic_param_bounds: self
                .generic_param_bounds
                .iter()
                .map(|bounds| {
                    bounds
                        .iter()
                        .map(|bound| bound.located(db, body_root))
                        .collect()
                })
                .collect(),
        }
    }
}

/// One declared bound at a lane's head.
pub(crate) fn anchor_bound(
    anchor: &mut PackageRefs<'_, '_>,
    bound: &TargetBound,
) -> InterfaceBound {
    InterfaceBound {
        interface: anchor.head(&bound.interface),
        args: bound
            .args
            .iter()
            .map(|arg| anchor.anchor_template(arg))
            .collect(),
        assoc: bound
            .assoc
            .iter()
            .map(|(name, ty)| (name.clone(), anchor.anchor_template(ty)))
            .collect(),
    }
}

/// Lower one `implements` block's target and for-type. `None` when the target
/// does not lower to an interface (already diagnosed upstream), or when a
/// declared bound failed to lower: the uniform bound surface
/// (`impl_generic_bounds`) keeps only bounds that lower to interfaces —
/// E0145 / unresolved-name diagnostics own the rest — so a declared/lowered
/// count mismatch means the declared rule is NARROWER than anything bakeable.
/// Baking without the bound WIDENS the rule; declining here drops the whole
/// rule from BOTH callers (the bake and the coherence key), which loses a
/// dispatch and can never over-match or mis-attribute. Fires only on
/// programs that already carry diagnostics and never reach a runnable
/// artifact.
pub(crate) fn impl_rule_target<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    impl_loc: ImplLoc<'db>,
    resolved: &RuntimeLowering<'_>,
) -> Option<ImplRuleTarget<'db>> {
    let block = impl_block_data(db, impl_loc);
    let store = &block.type_refs;
    let impl_params = baml_compiler2_hir_ty::lower::impl_frame(db, impl_loc);
    let impl_bounds = baml_compiler2_hir_ty::lower::impl_generic_bounds(db, impl_loc);
    // The target is a constraint, not an existential: it carries only its
    // written inline pins (unwritten members bake their declared defaults).
    let iface_ty = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, impl_loc.file(db))
        .with_frame(impl_params.clone())
        .with_bounds(impl_bounds.clone())
        .lower_type_ref_at(
            store,
            block.interface_target,
            baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
        );
    let iface_ty = baml_compiler2_hir_ty::lower::reject_holes(&iface_ty);
    let target = split_interface(&iface_ty, resolved, &impl_params)?;
    // The checker resolved the written target to a declaration before it
    // lowered to an interface type, so the head names one on some lane.
    let interface = layout::interface_ref_of(db, &target.head).unwrap_or_else(|| {
        unreachable!(
            "the lowered target `{}` names an interface no lane declares",
            target.wire
        )
    });
    // The implementor: `Self` in `Ty` space, off the UNIFORM impl surface —
    // `impl_self_ty` (with `impl_frame`/`impl_generic_bounds` above) owns the
    // in-body-vs-free distinction; emit never matches the subject.
    let for_ty = baml_compiler2_hir_ty::lower::impl_self_ty(db, impl_loc);
    let for_ty_pattern = tir2_to_template(&for_ty, resolved, &impl_params);
    // Fail closed on a bound the LOWERING dropped (doc above).
    let (declared_generics, _) =
        baml_compiler2_hir::item_data::impl_declared_generics(db, impl_loc);
    let declared_bound_count: usize = declared_generics.iter().map(|g| g.bounds.len()).sum();
    let lowered_bound_count: usize = impl_params
        .iter()
        .map(|param| impl_bounds.get(param).map_or(0, Vec::len))
        .sum();
    if lowered_bound_count != declared_bound_count {
        return None;
    }
    // Each declared param's bound conjunction, converted through the same
    // `split_interface` road as the target. The Option-collect is a second
    // belt on the same law (a bound that does not split drops the rule);
    // with the arity gate above it should never fire. Each conjunction is
    // sorted canonically so a written reorder cannot fork the identity key.
    let body_root = file_package(db, impl_loc.file(db)).root;
    let generic_param_bounds: Option<Vec<Vec<TargetBound>>> = impl_params
        .iter()
        .map(|param| {
            let mut bounds: Vec<TargetBound> = impl_bounds
                .get(param)
                .into_iter()
                .flatten()
                .map(|bound| {
                    split_interface(&bound.to_ty(), resolved, &impl_params).map(|parts| {
                        TargetBound {
                            interface: parts.head,
                            args: parts.args,
                            assoc: parts.assoc,
                        }
                    })
                })
                .collect::<Option<_>>()?;
            // Ordered by the located form, which is the same in every
            // database the package compiles in.
            bounds.sort_by_cached_key(|bound| format!("{:?}", bound.located(db, body_root)));
            Some(bounds)
        })
        .collect();
    let generic_param_bounds = generic_param_bounds?;
    Some(ImplRuleTarget {
        interface,
        iface_tn: target.wire,
        iface_ty,
        interface_args: target.args,
        interface_assoc: target.assoc,
        for_ty,
        for_ty_pattern,
        impl_params,
        impl_bounds,
        generic_param_bounds,
    })
}

/// One `implements` block's baked rule with every reference kept as an
/// identity: the runtime rule minus the two indices only a placement lane
/// can answer — the interface's pooled object (the caller has the interface
/// from the [`ImplRuleTarget`] it baked) and each provided method's body.
pub(crate) struct ImplRuleParts<'db> {
    pub(crate) for_ty_pattern: TyTemplate,
    pub(crate) generic_param_bounds: Vec<Vec<InterfaceBound>>,
    pub(crate) interface_args: Vec<TyTemplate>,
    /// Every declared associated member: the target's written pins, the
    /// block's `type X = ..` bindings, and the interface's defaults
    /// completed at this implementor.
    pub(crate) interface_assoc: Vec<(Name, TyTemplate)>,
    /// The owner frame every provided body is compiled against: the impl's
    /// declared generics, which for an in-class block ARE the class's.
    pub(crate) frame: Vec<TyTemplate>,
    /// The block's provided methods that have a compiled body, by name
    /// (a `$compiler_intrinsic` / `$await_any` body is never pooled and
    /// contributes no dispatch — see [`owns_no_slot`]).
    pub(crate) methods: Vec<(Name, FunctionLoc<'db>)>,
    /// Per declared interface field, in the interface's field order, the
    /// class slot it reads.
    pub(crate) field_links: Box<[u32]>,
}

/// Bake one `implements` block's rule from its [`ImplRuleTarget`].
///
/// ONE rule per block, whatever its spelling (`TYPE_SYSTEM.md`: in-class and
/// out-of-body impls are one form). The subject decides only where the
/// owner frame, bounds, and field table come from: an in-class block borrows
/// the enclosing CLASS's generics and links its fields; a free block
/// declares its own generics and never has fields to link (E0126). A
/// field-only (method-less) impl still bakes — membership matters for
/// reflection and bound checks even when there is nothing to dispatch.
///
/// `None` drops the rule whole, never a partial one: a field table whose
/// positions no longer line up with the interface would silently read the
/// wrong field, while a lost dispatch is an absence — and every such case
/// is already a diagnosed program that never reaches a runnable artifact.
pub(crate) fn bake_impl_rule<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    impl_loc: ImplLoc<'db>,
    target: ImplRuleTarget<'db>,
    resolved: &RuntimeLowering<'_>,
    anchor: &mut PackageRefs<'_, '_>,
) -> Option<ImplRuleParts<'db>> {
    let block = impl_block_data(db, impl_loc);
    let ImplRuleTarget {
        interface,
        iface_tn,
        iface_ty,
        interface_args,
        mut interface_assoc,
        for_ty,
        for_ty_pattern,
        impl_params,
        impl_bounds,
        generic_param_bounds,
    } = target;
    interface_assoc.extend(block_associated_bindings(
        db,
        impl_loc,
        &block.type_refs,
        &block.associated_type_bindings,
        &impl_params,
        &impl_bounds,
        resolved,
    ));
    let Ty::Interface(_, iface_arg_tys, _) = &iface_ty else {
        unreachable!("split_interface matched an interface")
    };
    complete_interface_assoc(
        db,
        interface,
        &mut interface_assoc,
        iface_arg_tys,
        &for_ty,
        &impl_params,
        resolved,
    );
    // The constraint set was lowered (and fail-closed gated) inside
    // `impl_rule_target`, so the bake and the rule's `ImplCoherenceKey`
    // carry the identical canonicalized bounds.
    let frame = (0..u32::try_from(impl_params.len()).expect("generic arity fits u32"))
        .map(TyTemplate::TypeArgRef)
        .collect();
    let methods = block
        .methods
        .iter()
        .copied()
        .filter(|&method| !owns_no_slot(db, method))
        .map(|method| (function_data(db, method).name.clone(), method))
        .collect();
    let field_links = field_links(db, impl_loc, interface, &iface_tn, &block.field_links)?;
    Some(ImplRuleParts {
        for_ty_pattern: anchor.anchor_template(&for_ty_pattern),
        generic_param_bounds: generic_param_bounds
            .iter()
            .map(|bounds| {
                bounds
                    .iter()
                    .map(|bound| anchor_bound(anchor, bound))
                    .collect()
            })
            .collect(),
        interface_args: interface_args
            .iter()
            .map(|arg| anchor.anchor_template(arg))
            .collect(),
        interface_assoc: interface_assoc
            .iter()
            .map(|(name, ty)| (name.clone(), anchor.anchor_template(ty)))
            .collect(),
        frame,
        methods,
        field_links,
    })
}

/// Associated-type bindings written in an `implements` block body
/// (`type Item = int`) live beside the target, not in it (`split_interface`
/// only sees the target), so fold them into the implemented interface's
/// bindings. Prefer `impl_data`'s canonical checked value: it lowers
/// bindings in declaration order with `Self` carrying the pins resolved so
/// far, so `type Item = int; type Items = Self.Item[]` becomes `int[]`.
/// Mounted interfaces have no source `InterfaceLoc`; their equivalent
/// canonical values live in the loc-free `impl_facts` surface. Re-lowering
/// a raw binding in the plain impl scope loses the earlier witness and
/// leaves an error-recovery projection that `RuntimeTy` cannot represent.
fn block_associated_bindings<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    impl_loc: ImplLoc<'db>,
    store: &TypeRefStore,
    bindings: &[AssociatedTypeBindingData],
    generics: &[ParamTy],
    bounds: &ImplBoundsMap,
    resolved: &RuntimeLowering<'_>,
) -> Vec<(Name, baml_compiler2_mir::TyTemplate)> {
    let canonical = baml_compiler2_hir_ty::interfaces::impl_data(db, impl_loc)
        .as_ref()
        .ok();
    let loc_free = baml_compiler2_hir_ty::impls::impl_facts(db, impl_loc).for_display();
    // Lower a type ref (in the block's `TypeRefStore`) in the block's own
    // scope, discarding diagnostics (these targets were already validated
    // upstream). `bounds` carries the impl's generic-param bounds so a
    // bound-typevar projection in a binding value — the `implements<T
    // extends Iface> Other for T[] { type E = T.Assoc }` shape — determines
    // its interface instead of erasing.
    let lower = |id: TypeRefId| -> Ty {
        baml_compiler2_hir_ty::lower::reject_holes(
            &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, impl_loc.file(db))
                .with_frame(generics.to_vec())
                .with_bounds(bounds.clone())
                .lower_type_ref(store, id),
        )
    };
    bindings
        .iter()
        .filter_map(|binding| {
            let ty = canonical
                .and_then(|data| {
                    data.associated_types
                        .iter()
                        .find(|(name, _)| *name == binding.name)
                        .map(|(_, ty)| ty.clone())
                })
                .or_else(|| {
                    loc_free.and_then(|facts| {
                        facts
                            .associated_types
                            .iter()
                            .find(|(name, _)| *name == binding.name)
                            .map(|(_, ty)| ty.to_plain())
                    })
                })
                .or_else(|| binding.type_ref.map(lower))?;
            Some((
                binding.name.clone(),
                tir2_to_template(&ty, resolved, generics),
            ))
        })
        .collect()
}

/// Complete a rule's associated bindings: every declared member the impl
/// leaves unpinned is baked from the interface's declared default,
/// substituted at this impl — `Self` := the for-type, the interface's params
/// := the target's arguments — so the registry answers every declared member
/// identically, pinned or defaulted. A Self-referencing default
/// (`type Items = Self.Item[]`) keeps its projections symbolic in the baked
/// template; the runtime reduces them back through this same rule at
/// realization time (fuel-bounded against cycles). A member with neither pin
/// nor default is a diagnosed incomplete impl and stays absent.
///
/// Associated types are NOT frame slots (an adopted default's frame is
/// `[Self ++ interface generic args]`; a body's `Self.Assoc` lowers as a
/// projection), so this table is the RULE's reduction source for those
/// projections, never a frame filler.
fn complete_interface_assoc<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    interface: InterfaceRef<'db>,
    interface_assoc: &mut Vec<(Name, baml_compiler2_mir::TyTemplate)>,
    iface_arg_tys: &[Ty],
    for_ty: &Ty,
    generics: &[ParamTy],
    resolved: &RuntimeLowering<'_>,
) {
    let decls = layout::interface_assoc_decls(db, interface);
    for (name, default) in &decls.members {
        if interface_assoc.iter().any(|(bound, _)| bound == name) {
            continue;
        }
        let Some(default) = default else {
            continue;
        };
        let mut bindings: rustc_hash::FxHashMap<ParamTy, Ty> = rustc_hash::FxHashMap::default();
        bindings.insert(decls.self_param.clone(), for_ty.clone());
        for (param, arg) in decls.params.iter().zip(iface_arg_tys) {
            bindings.insert(param.clone(), arg.clone());
        }
        let completed = baml_type::unify::substitute_ty(default, &bindings);
        interface_assoc.push((
            name.clone(),
            tir2_to_template(&completed, resolved, generics),
        ));
    }
}

/// The field table for one block, positional over the interface's own
/// declared fields. Each entry is the class slot the interface field reads:
/// the block's explicit `field as class_field` link, else the same-named
/// class field (the default that `concrete_interface_field_sources` applies
/// in TIR).
///
/// `None` when the table would be partial: an out-of-body impl of a
/// field-bearing interface (E0126), or a link to a field the class does not
/// declare (E0124) — both already diagnosed, so the program never reaches a
/// runnable artifact; the rule is dropped whole rather than baked with a
/// table whose positions no longer line up with the interface.
fn field_links<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    impl_loc: ImplLoc<'db>,
    interface: InterfaceRef<'db>,
    iface_tn: &TypeName,
    links: &[InterfaceFieldLinkData],
) -> Option<Box<[u32]>> {
    let declared = layout::interface_field_names(db, interface);
    if declared.is_empty() {
        return Some(Box::default());
    }
    let Some(class) = impl_enclosing_class(db, impl_loc) else {
        debug_assert!(
            false,
            "out-of-body impl of field-bearing interface `{iface_tn}` should be rejected by E0126",
        );
        return None;
    };
    declared
        .iter()
        .map(|iface_field| {
            let class_field = links
                .iter()
                .find(|link| link.interface_field == *iface_field)
                .map_or(iface_field, |link| &link.class_field);
            let slot = layout::class_field_index(db, DeclRef::Source(class), class_field);
            debug_assert!(
                slot.is_some(),
                "interface `{iface_tn}` field `{iface_field}` links to `{}.{class_field}`, which \
                 has no runtime slot",
                class_data(db, class).name,
            );
            slot
        })
        .collect()
}

// ── Synthesized functions ────────────────────────────────────────────────────

/// A synthesized zero-arity function: no source, no signature, `Internal`
/// origin, `null` return.
fn synthesized_function(name: String, bytecode: Bytecode) -> Function {
    Function {
        name,
        source_file: String::new(),
        docstring: None,
        declared_name: None,
        arity: 0,
        real_local_count: 0,
        bytecode,
        kind: FunctionKind::Bytecode,
        telemetry_function_id: None,
        telemetry_registration: bex_vm_types::FunctionRegistration::default(),
        telemetry_policy_id: bex_vm_types::TelemetryPolicyId::none(),
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: baml_base::Span::fake(),
        return_type: TyTemplate::Null,
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: "null".to_string(),
        throws_type: TyTemplate::Never,
        origin: FunctionOrigin::Internal,
        is_interface_body: false,
        native_key: None,
        body_meta: None,
        runtime_package: bex_vm_types::HeapPtr::null(),
    }
}

/// The zero-arg helper `$init_let_{ordinal}` that evaluates one top-level
/// `let`'s initializer — the body `$init` calls before storing the result
/// into the binding's slot. The initializer is lowered through MIR and
/// compiled like any body; its lambda children are pooled into `objects`
/// first. A binding without an initializer gets a helper that pushes `null`.
/// The helper itself is the caller's to pool and slot.
///
/// # Errors
///
/// The initializer failed to lower (an internal error, attributed to the
/// `let`'s file).
#[expect(clippy::too_many_arguments)]
pub(crate) fn compile_let_helper<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    binding: LetLoc<'db>,
    ordinal: usize,
    refs: &mut PackageRefs<'_, 'db>,
    class_fields: &ClassFieldSnapshot<'db>,
    objects: &mut ObjectPool,
    objects_base: usize,
    opt: OptLevel,
) -> Result<Function, LoweringError> {
    let file = binding.file(db);
    let name = format!("$init_let_{ordinal}");
    let lowered = lower_let_body(db, binding, opt).map_err(|error| {
        crate::internal_lowering_error(
            db,
            file,
            &definition_link_name(db, Definition::Let(binding)),
            &error,
        )
    })?;
    let Some((mir_body, lambdas)) = lowered else {
        let mut bytecode = Bytecode::default();
        bytecode.constants.push(ConstValue::Null);
        bytecode.instructions.push(Instruction::LoadConst(0));
        bytecode.instructions.push(Instruction::Return);
        return Ok(synthesized_function(name, bytecode));
    };
    let line_starts = crate::build_line_starts(file.text(db));
    let source_file = crate::relative_source_path(db, file);
    let empty_capture_types = Vec::new();
    let empty_spawn_capture_indices = HashSet::new();
    let lambda_info = crate::compile_lambdas(
        db,
        &lambdas,
        Some(&mir_body),
        &empty_capture_types,
        &empty_spawn_capture_indices,
        &line_starts,
        &source_file,
        &mut *refs,
        class_fields,
        objects,
        objects_base,
        opt,
    );
    let lambda_object_indices: Vec<usize> = lambda_info.iter().map(|(idx, _)| *idx).collect();
    let lambda_names: Vec<String> = lambda_info.iter().map(|(_, name)| name.clone()).collect();
    let ctx = MirCodegenContext {
        db,
        refs,
        class_fields,
        objects,
        objects_base,
        lambda_object_indices: &lambda_object_indices,
        lambda_names: &lambda_names,
        capture_types: &empty_capture_types,
        spawn_capture_indices: &empty_spawn_capture_indices,
    };
    let mut helper = compile_mir_function(&mir_body, 0, None, &line_starts, ctx, opt);
    helper.name = name;
    helper.source_file = source_file;
    helper.arity = 0;
    Ok(helper)
}

/// One `let` of a package's `$init`: the helper that evaluates its
/// initializer and the binding's slot the result is stored into.
pub(crate) struct InitStep {
    /// The global slot [`compile_let_helper`]'s function was placed at.
    pub(crate) helper: GlobalIndex,
    /// The binding's own global slot.
    pub(crate) target: GlobalIndex,
    /// The binding's rendered name — operand metadata only.
    pub(crate) target_name: String,
}

/// A package's `$init`: calls every `let`'s helper in dependency order and
/// stores each result into the binding's slot.
pub(crate) fn build_init_function(steps: &[InitStep]) -> Function {
    let mut instructions: Vec<Instruction> = Vec::new();
    let mut meta: Vec<InstructionMeta> = Vec::new();
    let mut constants: Vec<ConstValue> = Vec::new();
    for (ordinal, step) in steps.iter().enumerate() {
        instructions.push(Instruction::Call {
            callee: step.helper,
            ntypeargs: 0,
        });
        meta.push(InstructionMeta {
            operand: Some(OperandMeta::Callable(format!("$init_let_{ordinal}"))),
        });
        instructions.push(Instruction::StoreGlobal(step.target));
        meta.push(InstructionMeta {
            operand: Some(OperandMeta::Global(step.target_name.clone())),
        });
    }
    // Push Null and Return (Return pops the top of the eval stack).
    let null_const_idx = constants.len();
    constants.push(ConstValue::Null);
    instructions.push(Instruction::LoadConst(null_const_idx));
    meta.push(InstructionMeta {
        operand: Some(OperandMeta::Const("null".to_string())),
    });
    instructions.push(Instruction::Return);
    meta.push(InstructionMeta::default());
    synthesized_function(
        "$init".to_string(),
        Bytecode {
            instructions,
            constants,
            meta,
            ..Bytecode::default()
        },
    )
}

/// A package's `$init_test` chainer: calls each per-file test initializer
/// (`parts`, in layout order) with the registry it was itself called with.
///
/// Per part: `LoadVar 1` (the registry param — slot 1 is the first arg,
/// slot 0 the reserved fn ref), `Call`, `Pop 1` (discard the null return).
pub(crate) fn build_init_test_chainer(parts: &[GlobalIndex]) -> Function {
    let mut instructions = Vec::new();
    let mut constants: Vec<ConstValue> = Vec::new();
    for &callee in parts {
        instructions.push(Instruction::LoadVar(1));
        instructions.push(Instruction::Call {
            callee,
            ntypeargs: 0,
        });
        instructions.push(Instruction::Pop(1));
    }
    let null_const_idx = constants.len();
    constants.push(ConstValue::Null);
    instructions.push(Instruction::LoadConst(null_const_idx));
    instructions.push(Instruction::Return);
    Function {
        arity: 1,
        real_local_count: 1, // the registry param
        // local_names is indexed by slot number: slot 0 = fn ref (reserved,
        // empty placeholder), slot 1 = first param "registry".
        local_names: vec![String::new(), "registry".to_string()],
        param_names: vec!["registry".to_string()],
        param_types: vec![TyTemplate::Unknown], // type not needed for chainer dispatch
        param_has_default: vec![false],
        display_param_types: vec!["unknown".to_string()],
        ..synthesized_function(
            "$init_test".to_string(),
            Bytecode {
                instructions,
                constants,
                ..Bytecode::default()
            },
        )
    }
}
