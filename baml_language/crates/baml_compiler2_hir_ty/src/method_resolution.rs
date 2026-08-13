//! Method resolution: receiver type -> method candidate, the
//! rust-analyzer `method_resolution.rs` analog (S11). BAML's version has
//! no autoderef/autoref chains: a receiver resolves to exactly one owning
//! class - its own for nominal receivers, the language's builtin class for
//! structural ones (`int[]` methods live on `class baml.Array<T>`,
//! `string`'s on `class baml.String`, and so on) - and the receiver's
//! structure supplies the class generic arguments.
//!
//! The full ladder (the callers' order): class-inherent methods, then
//! interface members - existential and rigid-bounded receivers through
//! their bounds (I3), concrete receivers through the impls they match
//! (I6, the rust-analyzer trait-impl candidate tier) - then fields.
//! Not yet resolved here (later slices): union receivers, `$stream`
//! companions, and free-impl method bodies as inference roots (their
//! member TYPES already resolve through the interface signature).

use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc, ImplLoc, InterfaceLoc},
};
use baml_type::{
    Literal, MediaKind, Name, ParamTy, TyAttr, TypeName,
    interned::{InterfaceRef, Ty, TyKind},
    normalize::TypeContext as _,
};

use crate::facts::Facts;

/// One resolved method: the function plus the receiver-driven
/// instantiation of its owning class's generic params (the frame prefix
/// that `function_generic_frame` prepends for methods).
pub struct MethodCandidate<'db> {
    pub method: FunctionLoc<'db>,
    /// The declaring class - the resolution's identity for the recorded
    /// tables (r-a's `write_method_resolution` records the target the
    /// probe chose; the class was already computed here either way).
    pub class: ClassLoc<'db>,
    pub class_args: Vec<Ty>,
}

/// Finds `name` among the methods of `receiver`'s owning class. The
/// receiver must already be structurally resolved (no top-level inference
/// var); aliases expand through the fact oracle.
pub fn lookup_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
    name: &Name,
) -> Option<MethodCandidate<'db>> {
    let (class, class_args) = receiver_class(facts, receiver, 8)?;
    let method = baml_compiler2_ppir::item_data::class_data(db, class)
        .methods
        .iter()
        .copied()
        .find(|&method| baml_compiler2_ppir::item_data::function_data(db, method).name == *name)?;
    Some(MethodCandidate {
        method,
        class,
        class_args,
    })
}

/// The class whose declaration owns `receiver`'s methods, with the generic
/// arguments the receiver pins. This table IS the language's builtin-class
/// correspondence (TIR: `resolve_builtin_member` call sites), one row per
/// structural kind; literals defer to their base primitive's class.
pub(crate) fn receiver_class<'db>(
    facts: &Facts<'db>,
    receiver: &Ty,
    fuel: u32,
) -> Option<(ClassLoc<'db>, Vec<Ty>)> {
    let builtin = |namespace: &[&str], name: &str, args: Vec<Ty>| {
        let qtn = TypeName::new(
            Name::new("baml"),
            namespace.iter().map(Name::new).collect(),
            Name::new(name),
        );
        match facts.definition_of(&qtn) {
            Some(Definition::Class(class)) => Some((class, args)),
            _ => None,
        }
    };
    match receiver.kind() {
        TyKind::Class(qtn, args, _) => match facts.definition_of(qtn) {
            Some(Definition::Class(class)) => Some((class, args.to_vec())),
            _ => None,
        },
        TyKind::List(element, _) => builtin(&[], "Array", vec![element.clone()]),
        TyKind::Map { key, value, .. } => builtin(&[], "Map", vec![key.clone(), value.clone()]),
        TyKind::Future(value, error, _) => {
            builtin(&["future"], "Future", vec![value.clone(), error.clone()])
        }
        TyKind::String { .. } | TyKind::Literal(Literal::String(_), _, _) => {
            builtin(&[], "String", Vec::new())
        }
        TyKind::Int { .. } | TyKind::Literal(Literal::Int(_), _, _) => {
            builtin(&[], "Int", Vec::new())
        }
        TyKind::Bigint { .. } | TyKind::Literal(Literal::Bigint(_), _, _) => {
            builtin(&[], "Bigint", Vec::new())
        }
        TyKind::Float { .. } | TyKind::Literal(Literal::Float(_), _, _) => {
            builtin(&[], "Float", Vec::new())
        }
        TyKind::Bool { .. } | TyKind::Literal(Literal::Bool(_), _, _) => {
            builtin(&[], "Bool", Vec::new())
        }
        TyKind::Uint8Array { .. } => builtin(&[], "Uint8Array", Vec::new()),
        // The `type` primitive's members (reflection, BEP-039) live on
        // `class baml.TypeValue` - `reflect.type_of<T>().to_string()`.
        TyKind::Type { .. } => builtin(&[], "TypeValue", Vec::new()),
        TyKind::Media(kind, _) => {
            let class = match kind {
                MediaKind::Image => "Image",
                MediaKind::Audio => "Audio",
                MediaKind::Video => "Video",
                MediaKind::Pdf => "Pdf",
                // Generic media (`media`, any subtype) has no single class.
                MediaKind::Generic => return None,
            };
            builtin(&["media"], class, Vec::new())
        }
        // Aliases are transparent: expand through the oracle (fuel-bounded
        // like every alias walk) and resolve on the expansion.
        TyKind::TypeAlias(qtn, _) => {
            let expanded = facts.alias_def(qtn)?;
            let fuel = fuel.checked_sub(1)?;
            receiver_class(facts, &Ty::from_plain(&expanded), fuel)
        }
        _ => None,
    }
}

/// A resolved INTERFACE member (I3): the member's type, fully
/// instantiated for the receiver.
pub struct InterfaceMember<'db> {
    pub ty: Ty,
    /// Methods bind their receiver; fields do not take one.
    pub is_method: bool,
    /// Set when the member is a default method with OWN generic params:
    /// `ty` has the interface frame substituted but the own suffix still
    /// rigid, and the CALL SITE finishes the instantiation (turbofish or
    /// fresh vars) - rust-analyzer's `subst_for_def(parent_subst)` +
    /// `fill_with_inference_vars` split, the same discipline the
    /// class-method path applies through `MethodCandidate`.
    pub pending_own: Option<PendingOwnGenerics<'db>>,
    /// Where the member was found - the resolution's identity for the
    /// recorded tables. The resolver knows the declarer at the match; it
    /// returns what it computed (r-a's resolution fns hand back the
    /// target for `write_method_resolution` the same way).
    pub declarer: MemberDeclarer<'db>,
}

/// The declaration a resolved interface member came from, by dispatch
/// mode: symbolic receivers (existential, bounded var, rigid projection)
/// dispatch VIRTUALLY through the interface slot; concrete receivers
/// resolve through a specific impl block.
pub enum MemberDeclarer<'db> {
    /// A virtual FIELD read: the realized declaring-interface view (the
    /// runtime resolver's key) and the field's index in that interface's
    /// own declared field list.
    VirtualField {
        interface: InterfaceLoc<'db>,
        realized: InterfaceRef,
        field_index: u32,
    },
    /// A virtual METHOD slot: only the interface and the member are
    /// statically known (a required method has no body; the default is
    /// one possible target).
    VirtualMethod {
        interface: InterfaceLoc<'db>,
        method: FunctionLoc<'db>,
    },
    /// A concrete receiver's method through a matched impl: the impl's
    /// override when it provides one, else the interface's default body.
    ImplMethod {
        block: ImplLoc<'db>,
        func: FunctionLoc<'db>,
    },
    /// A concrete receiver's interface FIELD through a matched impl.
    /// The backing class-field link is not resolved here yet, so
    /// consumers record nothing for this case (S16 follow-up).
    ImplField { block: ImplLoc<'db> },
}

/// The pieces the call site needs to finish a default method's
/// instantiation: the method and the interface-frame prefix
/// (`[Self, args.., assoc..]`) already pinned by the receiver.
pub struct PendingOwnGenerics<'db> {
    pub method: baml_compiler2_hir::loc::FunctionLoc<'db>,
    pub prefix: Vec<Ty>,
}

/// The outcome of an interface-member lookup. Ambiguity is a DISTINCT
/// verdict from not-found (rustc's `MethodError::Ambiguity` next to
/// `NoMatch`): a member two realized-distinct interfaces declare must
/// report E0121/E0122 with its sources, never silently pick one or fall
/// through to an unrelated "no such member" report.
pub enum InterfaceMemberLookup<'db> {
    Found(InterfaceMember<'db>),
    /// Two or more realized-distinct interfaces declare the member.
    /// `sources` are their qualified display names, for the `as<I>`
    /// disambiguation hint.
    Ambiguous {
        sources: Vec<String>,
        is_field: bool,
    },
    NotFound,
}

/// Resolves `name` as an interface member of `receiver` - an interface
/// existential (`Self` = the existential, one-`Self` gated) or a rigid
/// bounded var (`Self` = the variable; `Self`-typed params are sound).
/// A bounded var's bound list is a CONJUNCTION: every conjunct
/// contributes candidates. Within one conjunct the root shadows its
/// `requires` closure (root-wins); ACROSS the pool, declarers dedupe by
/// realized identity - a declarer two conjuncts both reach through
/// `requires` is one declarer, not an ambiguity (the associated-type
/// dedup discipline) - and two or more surviving declarers are
/// ambiguous. Concrete receivers' impl-provided members join with the
/// impls-for-receiver step (pinned pending).
pub fn lookup_interface_member<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
    name: &Name,
) -> InterfaceMemberLookup<'db> {
    let (roots, existential): (Vec<InterfaceRef>, bool) = match receiver.kind() {
        TyKind::Interface(qtn, args, pins, _) => (
            vec![InterfaceRef::new(
                qtn.clone(),
                (args.to_vec()).into(),
                pins.to_vec(),
            )],
            true,
        ),
        TyKind::TypeVar(param, _) => (
            baml_type::normalize::TypeContext::type_var_bound(facts, param)
                .iter()
                .map(InterfaceRef::from_constraint)
                .collect(),
            false,
        ),
        // A rigid, irreducible projection: rustc's alias-bound
        // (`item_bounds`) candidates - the associated type's DECLARED
        // bound (`type Item extends Labeled`) is what its members
        // resolve through, realized for the projection's subject.
        TyKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => (assoc_bound_roots(db, facts, base, interface, member), false),
        // Concrete receivers resolve through the impls they match - the
        // trait-impl candidate tier (I6).
        _ => {
            return match lookup_impl_member(db, facts, receiver, name) {
                Some(member) => InterfaceMemberLookup::Found(member),
                None => InterfaceMemberLookup::NotFound,
            };
        }
    };
    let mut declarers: Vec<(InterfaceRef, InterfaceMember<'db>)> = Vec::new();
    let push = |declarers: &mut Vec<(InterfaceRef, InterfaceMember<'db>)>,
                realized: &InterfaceRef,
                member: InterfaceMember<'db>| {
        if !declarers.iter().any(|(seen, _)| seen == realized) {
            declarers.push((realized.clone(), member));
        }
    };
    for root in &roots {
        // Root-wins: the directly-named interface shadows its own closure.
        if let Some(member) = member_on_interface(db, facts, root, receiver, name, existential) {
            push(&mut declarers, root, member);
            continue;
        }
        for required in crate::impls::direct_requires_closure(db, root, receiver, 8) {
            if let Some(member) =
                member_on_interface(db, facts, &required, receiver, name, existential)
            {
                push(&mut declarers, &required, member);
            }
        }
    }
    match declarers.len() {
        0 => InterfaceMemberLookup::NotFound,
        1 => {
            let (_, member) = declarers.pop().expect("len checked");
            InterfaceMemberLookup::Found(member)
        }
        _ => InterfaceMemberLookup::Ambiguous {
            is_field: !declarers[0].1.is_method,
            sources: declarers
                .iter()
                .map(|(realized, _)| realized.name.render_user_facing())
                .collect(),
        },
    }
}

/// Concrete receivers: the rust-analyzer trait-impl candidate tier of
/// method resolution. Class-inherent methods were tried first (the
/// caller's ladder); here every impl the receiver matches contributes
/// its interface's members - fields, required signatures, and DEFAULT
/// methods realized at `Self` = the receiver, unpinned associated slots
/// as symbolic projections the oracle reduces through the impl's
/// bindings (I5). Root-wins across providers (a provider another
/// provider `requires` is shadowed - the most-derived interface wins,
/// mirroring the symbolic receivers' tiering); distinct survivors are
/// ambiguous (None; S17 renders). Free-impl method BODIES as inference
/// roots are separate future work - the member TYPE here comes from the
/// interface signature either way.
fn lookup_impl_member<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
    name: &Name,
) -> Option<InterfaceMember<'db>> {
    let mut providers: Vec<(InterfaceRef, InterfaceMember<'db>)> = Vec::new();
    for resolved in crate::impls::impls_for_type(db, receiver) {
        if !env_discharges_rigid_bounds(db, facts, &resolved) {
            continue;
        }
        let implemented = resolved.implemented();
        if let Some(mut member) =
            member_on_interface(db, facts, &implemented, receiver, name, false)
            && !providers.iter().any(|(seen, _)| *seen == implemented)
        {
            // Concrete dispatch: the impl block is the declarer. A
            // method resolves to the impl's override when it provides
            // one, else the interface's default body (the recorded
            // callable IS the body the call runs).
            member.declarer = match member.declarer {
                MemberDeclarer::VirtualMethod { method, .. } => {
                    let func = resolved
                        .facts
                        .methods
                        .iter()
                        .copied()
                        .find(|&candidate| {
                            baml_compiler2_ppir::item_data::function_data(db, candidate).name
                                == *name
                        })
                        .unwrap_or(method);
                    MemberDeclarer::ImplMethod {
                        block: resolved.block,
                        func,
                    }
                }
                MemberDeclarer::VirtualField { .. } => MemberDeclarer::ImplField {
                    block: resolved.block,
                },
                concrete => concrete,
            };
            providers.push((implemented, member));
        }
    }
    if providers.len() > 1 {
        let heads: Vec<InterfaceRef> = providers.iter().map(|(target, _)| target.clone()).collect();
        providers.retain(|(target, _)| {
            !heads.iter().any(|other| {
                other.name != target.name
                    && crate::impls::interface_requires(db, other, target, receiver, 8)
            })
        });
    }
    match providers.len() {
        1 => providers.pop().map(|(_, member)| member),
        _ => None,
    }
}

/// The alias-bound roots of a projection receiver: the associated
/// type's declared bound, lowered in the interface frame and realized
/// with `Self` = the projection's base - the assoc's own slot realizes
/// to the projection itself, so a `Self.Item`-mentioning bound lands
/// back on the receiver.
fn assoc_bound_roots<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    base: &Ty,
    interface_ref: &baml_type::interned::InterfaceRef,
    member: &Name,
) -> Vec<InterfaceRef> {
    let Some(Definition::Interface(interface)) = facts.definition_of(&interface_ref.name) else {
        return Vec::new();
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let Some(assoc) = data
        .associated_types
        .iter()
        .find(|assoc| assoc.name == *member)
    else {
        return Vec::new();
    };
    let Some(bound) = assoc.bound else {
        return Vec::new();
    };
    let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db))
        .with_frame(interface_frame(interface, db))
        .with_bounds(crate::lower::interface_scope_bounds(db, interface));
    let bound_ty = ctx.lower_type_ref(&data.type_refs, bound);
    let target = InterfaceRef::new(
        interface_ref.name.clone(),
        (interface_ref.generics.to_vec()).into(),
        interface_ref.associated_types.to_vec(),
    );
    let instantiation = interface_instantiation(base, &target, data);
    match crate::lower::substitute_params(&bound_ty, &instantiation).kind() {
        TyKind::Interface(name, args, pins, _) => vec![InterfaceRef::new(
            name.clone(),
            (args.to_vec()).into(),
            pins.to_vec(),
        )],
        _ => Vec::new(),
    }
}

/// Discharges the bounds `bounds_hold` skipped as vacuous - impl params
/// bound to RIGID vars - against the caller's param env: rustc's
/// caller-bound (`ParamCandidate`) tier, checked here because the env
/// exists at this layer and not inside the db-only impl search.
fn env_discharges_rigid_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    resolved: &crate::impls::ResolvedImpl<'db>,
) -> bool {
    for (param, bounds) in &resolved.facts.generic_params {
        let Some(actual) = resolved.bindings.get(param) else {
            continue;
        };
        if !actual.has_typevar() {
            continue;
        }
        for bound in bounds {
            // Pins are outputs, not part of the relation.
            let goal = InterfaceRef::new(
                bound.name.clone(),
                bound
                    .generics
                    .iter()
                    .map(|arg| crate::impls::substitute_bindings(arg, &resolved.bindings))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                Vec::new(),
            );
            if !env_proves(db, facts, actual, &goal) {
                return false;
            }
        }
    }
    true
}

/// Whether the param env proves rigid `actual` implements `goal`: the
/// declared env clause for the var, or anything in its elaborated
/// `requires` closure, whose head matches the goal. Structured
/// rigid-carrying actuals fall back to the impl search (which admits
/// placeholders).
fn env_proves<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    actual: &Ty,
    goal: &InterfaceRef,
) -> bool {
    let TyKind::TypeVar(param, _) = actual.kind() else {
        return crate::impls::resolve_impl(db, actual, goal).is_some();
    };
    let eq = crate::impls::AliasOnlyFacts::new(db);
    for bound in baml_type::normalize::TypeContext::type_var_bound(facts, param) {
        let root = InterfaceRef::from_constraint(&bound);
        let heads = crate::impls::requires_heads(db, &root, actual, 8);
        if heads
            .iter()
            .any(|head| crate::impls::head_matches(head, goal, &eq))
        {
            return true;
        }
    }
    false
}

/// A member declared DIRECTLY on `target`, instantiated for `receiver`.
pub(crate) fn member_on_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    target: &InterfaceRef,
    receiver: &Ty,
    name: &Name,
    existential: bool,
) -> Option<InterfaceMember<'db>> {
    let Some(Definition::Interface(interface)) = facts.definition_of(&target.name) else {
        return None;
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let instantiation = interface_instantiation(receiver, target, data);

    // Fields first (mirroring the class path's field-before-method).
    if let Some(index) = data.fields.iter().position(|field| field.name == *name) {
        let field = &data.fields[index];
        let frame = interface_frame(interface, db);
        let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db))
            .with_frame(frame)
            .with_bounds(crate::lower::interface_scope_bounds(db, interface));
        let field_ty = ctx.lower_type_ref(&data.type_refs, field.type_ref);
        return Some(InterfaceMember {
            ty: crate::lower::substitute_params(&field_ty, &instantiation),
            is_method: false,
            pending_own: None,
            declarer: MemberDeclarer::VirtualField {
                interface,
                realized: target.clone(),
                // The index space every implementation is baked against
                // is the interface's OWN declared field list, not the
                // requires-closure flattening (which dedups by name and
                // numbers differently).
                field_index: index as u32,
            },
        });
    }

    // Methods - default and required alike: ONE item kind, one
    // signature road (r-a's shape; `body: None` is body lowering's
    // business, never resolution's).
    if let Some(&method) = data
        .methods
        .iter()
        .find(|&&method| baml_compiler2_ppir::item_data::function_data(db, method).name == *name)
    {
        let signature = crate::lower::function_signature(db, method);
        if existential && signature_breaks_one_self(signature) {
            return None;
        }
        // The interface frame is the receiver's business; the method's
        // OWN generics (frame suffix, `map<R, E2>`, `seen<U>`) are the
        // call site's - hand them back for turbofish/fresh-var filling.
        let pending_own =
            (signature.generic_params.len() > instantiation.len()).then(|| PendingOwnGenerics {
                method,
                prefix: instantiation.clone(),
            });
        return Some(InterfaceMember {
            ty: instantiate_signature(signature, &instantiation),
            is_method: true,
            pending_own,
            declarer: MemberDeclarer::VirtualMethod { interface, method },
        });
    }

    None
}

/// The interface frame's instantiation vector for a receiver:
/// `[Self, args.., assoc..]` - `Self` is the receiver, args come from
/// the reference, associated slots take the reference's pins or the
/// symbolic projection through the receiver (which I5's oracle reduces
/// when the facts determine it).
pub(crate) fn interface_instantiation(
    receiver: &Ty,
    target: &InterfaceRef,
    data: &baml_compiler2_ppir::item_data::InterfaceData<'_>,
) -> Vec<Ty> {
    let mut out = vec![receiver.clone()];
    for (index, _) in data.generic_params.iter().enumerate() {
        out.push(
            target
                .generics
                .get(index)
                .cloned()
                .unwrap_or_else(Ty::error),
        );
    }
    let interface_ref = target.clone();
    for assoc in &data.associated_types {
        let slot = target
            .associated_types
            .iter()
            .find(|(name, _)| *name == assoc.name)
            .map(|(_, ty)| ty.clone())
            .unwrap_or_else(|| {
                Ty::intern(TyKind::AssociatedTypeProjection {
                    base: receiver.clone(),
                    interface: interface_ref.clone(),
                    member: assoc.name.clone(),
                    attr: TyAttr::default(),
                })
            });
        out.push(slot);
    }
    out
}

fn interface_frame<'db>(
    interface: InterfaceLoc<'db>,
    db: &'db dyn baml_compiler2_ppir::Db,
) -> Vec<ParamTy> {
    crate::lower::interface_frame(db, interface)
}

fn instantiate_signature(signature: &crate::lower::FunctionSignature, instantiation: &[Ty]) -> Ty {
    let params: Box<[baml_type::interned::FunctionParam]> = signature
        .params
        .iter()
        .map(|param| baml_type::interned::FunctionParam {
            name: Some(param.name.clone()),
            ty: crate::lower::substitute_params(&param.ty, instantiation),
            mode: if param.has_default {
                baml_type::FunctionParamMode::Optional
            } else {
                baml_type::FunctionParamMode::Required
            },
        })
        .collect();
    Ty::intern(TyKind::Function {
        params,
        ret: crate::lower::substitute_params(&signature.ret, instantiation),
        throws: crate::lower::substitute_params(&signature.throws, instantiation),
        attr: TyAttr::default(),
    })
}

/// The one-`Self` rule (spec: object safety for existential receivers):
/// a NON-self parameter containing bare `Self`, or `Self` nested inside
/// an invariant constructor in the return/throws, makes the method
/// uncallable through an existential (a bare top-level `-> Self`
/// collapses covariantly and stays legal). `Self.Assoc` projections are
/// exempt - the existential's pins make them one concrete type.
fn signature_breaks_one_self(signature: &crate::lower::FunctionSignature) -> bool {
    let self_in = |ty: &Ty, top_ok: bool| -> bool { self_occurs(ty, top_ok) };
    signature
        .params
        .iter()
        .skip(1)
        .any(|param| self_in(&param.ty, false))
        || self_in(&signature.ret, true)
        || self_in(&signature.throws, true)
}

/// Whether frame slot 0 (`Self`) occurs illegally: any occurrence in a
/// non-top position, or a top occurrence when `top_ok` is false.
/// Projection bases are exempt.
fn self_occurs(ty: &Ty, top_ok: bool) -> bool {
    match ty.kind() {
        TyKind::TypeVar(param, _) if param.index() == 0 && param.as_str() == "Self" => !top_ok,
        TyKind::AssociatedTypeProjection { .. } => false,
        // Unions and optionals are covariant-transparent.
        TyKind::Union(members, _) => members.iter().any(|member| self_occurs(member, top_ok)),
        _ => {
            let mut found = false;
            let mut children = Vec::new();
            baml_type::interned::for_each_child(ty.kind(), |child| children.push(child.clone()));
            for child in children {
                if self_occurs(&child, false) {
                    found = true;
                }
            }
            found
        }
    }
}
