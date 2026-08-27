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
    loc::{ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc},
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
    pub source: MethodCandidateSource<'db>,
    pub class_args: Vec<Ty>,
}

pub enum MethodCandidateSource<'db> {
    Source {
        method: FunctionLoc<'db>,
        class: ClassLoc<'db>,
    },
    External(Box<crate::package_interface::ResolvedFunction>),
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
    if let Some((class, class_args)) = receiver_class(facts, receiver, 8) {
        // Implements-block methods resolve here too (static dispatch to the
        // override, builtin-backed receivers included); the AMBIGUITY rule -
        // several implemented interfaces declaring the name need `as<I>`
        // qualification even when a candidate exists - is the callers'
        // `concrete_member_ambiguity` pre-check, not an exclusion here.
        let method = baml_compiler2_ppir::item_data::class_data(db, class)
            .methods
            .iter()
            .copied()
            .find(|&method| {
                baml_compiler2_ppir::item_data::function_data(db, method).name == *name
            })?;
        return Some(MethodCandidate {
            source: MethodCandidateSource::Source { method, class },
            class_args,
        });
    }
    let (qtn, class_args) = external_class_for_type(facts, receiver, 8)?;
    let crate::package_interface::ExportedType::Class {
        methods,
        generic_params,
        generic_param_bounds,
        ..
    } = crate::package_interface::mounted_type_row(db, &qtn)?
    else {
        return None;
    };
    let method = methods.iter().find(|method| method.name == *name)?;
    Some(MethodCandidate {
        source: MethodCandidateSource::External(Box::new(
            crate::package_interface::resolved_exported_function(
                method,
                generic_params.clone(),
                generic_param_bounds.clone(),
            ),
        )),
        class_args,
    })
}

/// Source-less counterpart of [`receiver_class`]. It maps structural builtin
/// types to the same canonical class names, but leaves the declaration lookup
/// to the precompiled `PackageInterface` row.
pub(crate) fn external_class_for_type(
    facts: &Facts<'_>,
    receiver: &Ty,
    fuel: u32,
) -> Option<(TypeName, Vec<Ty>)> {
    let builtin = |namespace: &[&str], name: &str, args: Vec<Ty>| {
        (
            TypeName::new(
                Name::new("baml"),
                namespace.iter().map(Name::new).collect(),
                Name::new(name),
            ),
            args,
        )
    };
    Some(match receiver.kind() {
        TyKind::Class(qtn, args, _) => (qtn.clone(), args.to_vec()),
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
        TyKind::Type { .. } => (
            TypeName::new(Name::new("reflect"), Vec::new(), Name::new("Type")),
            Vec::new(),
        ),
        TyKind::Media(kind, _) => {
            let class = match kind {
                MediaKind::Image => "Image",
                MediaKind::Audio => "Audio",
                MediaKind::Video => "Video",
                MediaKind::Pdf => "Pdf",
                MediaKind::Generic => return None,
            };
            builtin(&["media"], class, Vec::new())
        }
        TyKind::TypeAlias(qtn, _) => {
            let expanded = facts.alias_def(qtn)?;
            return external_class_for_type(
                facts,
                &Ty::from_plain(&expanded),
                fuel.checked_sub(1)?,
            );
        }
        _ => return None,
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
        TyKind::Type { .. } => {
            let qtn = TypeName::new(Name::new("reflect"), Vec::new(), Name::new("Type"));
            match facts.definition_of(&qtn) {
                Some(Definition::Class(class)) => Some((class, Vec::new())),
                _ => None,
            }
        }
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
    /// A mounted method target. The descriptor is fully owned and remains
    /// valid without a dependency source location.
    ExternalMethod(std::sync::Arc<crate::callable::ExternalCallable>),
    /// A virtual field declared by a mounted interface.
    ExternalVirtualField {
        interface: baml_type::QualifiedTypeName,
        realized: InterfaceRef,
        field_index: u32,
    },
}

/// The pieces the call site needs to finish a default method's
/// instantiation: the method and the interface-frame prefix
/// (`[Self, args.., assoc..]`) already pinned by the receiver.
pub enum PendingOwnGenerics<'db> {
    Source {
        method: baml_compiler2_hir::loc::FunctionLoc<'db>,
        prefix: Vec<Ty>,
    },
    External {
        function: Box<crate::package_interface::ResolvedFunction>,
        prefix: Vec<Ty>,
    },
}

/// The outcome of an interface-member lookup. Ambiguity is a DISTINCT
/// verdict from not-found (rustc's `MethodError::Ambiguity` next to
/// `NoMatch`): a member two realized-distinct interfaces declare must
/// report E0121/E0122 with its sources, never silently pick one or fall
/// through to an unrelated "no such member" report.
pub enum InterfaceMemberLookup<'db> {
    Found(InterfaceMember<'db>),
    /// Two or more realized-distinct interfaces declare the member.
    /// `sources` are the realized declaring interfaces, rendered
    /// qualified at report time for the `as<I>` disambiguation hint.
    Ambiguous {
        sources: Vec<InterfaceRef>,
        is_field: bool,
    },
    /// A concrete receiver reached an interface FIELD: reachable only
    /// through an explicit `obj.as<I>.field` projection (TIR's rule -
    /// a class-owned field of the same name resolves earlier in the
    /// caller's ladder, so this is the interface-only case).
    FieldRequiresProjection {
        interface: InterfaceRef,
    },
    /// The member exists on the interface but uses `Self` outside the
    /// receiver position, so it is uncallable through an existential
    /// (or union) receiver - Rust's `dyn Trait` object-safety split.
    SelfRestricted {
        interface: InterfaceRef,
        position: crate::diagnostics::SelfCallPosition,
    },
    NotFound,
}

/// The interfaces a SYMBOLIC receiver resolves members through, and whether
/// the receiver is existential (which excludes members whose `Self` use makes
/// them uncallable through one).
///
/// `None` means the receiver is concrete: its members come from the impls it
/// matches, not from a bound list. This is the one derivation
/// [`lookup_interface_member`] and [`member_candidates`] share, so the
/// name-keyed lookup and the enumeration can never disagree about which
/// interfaces are in play.
pub(crate) fn member_roots<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
) -> Option<(Vec<InterfaceRef>, bool)> {
    match receiver.kind() {
        TyKind::Interface(qtn, args, pins, _) => Some((
            vec![InterfaceRef::new(
                qtn.clone(),
                (args.to_vec()).into(),
                pins.to_vec(),
            )],
            true,
        )),
        TyKind::TypeVar(param, _) => Some((
            baml_type::normalize::TypeContext::type_var_bound(facts, param)
                .iter()
                .map(InterfaceRef::from_constraint)
                .collect(),
            false,
        )),
        // A rigid, irreducible projection: rustc's alias-bound
        // (`item_bounds`) candidates - the associated type's DECLARED
        // bound (`type Item extends Labeled`) is what its members
        // resolve through, realized for the projection's subject.
        TyKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => Some((assoc_bound_roots(db, facts, base, interface, member), false)),
        _ => None,
    }
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
    let Some((roots, existential)) = member_roots(db, facts, receiver) else {
        // Concrete receivers resolve through the impls they match - the
        // trait-impl candidate tier (I6).
        return lookup_impl_member(db, facts, receiver, name);
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
        0 => {
            // No resolvable declarer - but an existential receiver may
            // have SKIPPED a declared method for using `Self` outside
            // the receiver position. Say that, not "no member".
            if existential {
                for root in &roots {
                    if let Some(position) = declared_method_self_restriction(db, facts, root, name)
                    {
                        return InterfaceMemberLookup::SelfRestricted {
                            interface: root.clone(),
                            position,
                        };
                    }
                }
            }
            InterfaceMemberLookup::NotFound
        }
        1 => {
            let (_, member) = declarers.pop().expect("len checked");
            InterfaceMemberLookup::Found(member)
        }
        _ => InterfaceMemberLookup::Ambiguous {
            is_field: !declarers[0].1.is_method,
            sources: declarers
                .iter()
                .map(|(realized, _)| realized.clone())
                .collect(),
        },
    }
}

/// Whether `target` declares method `name` with a `Self` use that makes it
/// uncallable through an existential receiver, and where that use sits.
pub(crate) fn declared_method_self_restriction<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    target: &InterfaceRef,
    name: &Name,
) -> Option<crate::diagnostics::SelfCallPosition> {
    if let Some(crate::package_interface::ExportedType::Interface {
        self_param,
        required_methods,
        default_methods,
        ..
    }) = crate::package_interface::mounted_type_row(db, &target.name)
    {
        let method = required_methods
            .iter()
            .chain(default_methods)
            .find(|method| method.name == *name)?;
        let function =
            crate::package_interface::resolved_exported_function(method, Vec::new(), Vec::new());
        if !exported_signature_breaks_one_self(&function, self_param) {
            return None;
        }
        return Some(
            if function
                .params
                .iter()
                .skip(1)
                .any(|param| self_occurs(&Ty::from_plain(&param.ty), false))
            {
                crate::diagnostics::SelfCallPosition::Parameter
            } else {
                crate::diagnostics::SelfCallPosition::NestedInReturn
            },
        );
    }
    let Some(Definition::Interface(interface)) = facts.definition_of(&target.name) else {
        return None;
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let method =
        data.methods.iter().copied().find(|&method| {
            baml_compiler2_ppir::item_data::function_data(db, method).name == *name
        })?;
    let signature = crate::lower::function_signature(db, method);
    signature_breaks_one_self(signature).then(|| {
        if signature
            .params
            .iter()
            .skip(1)
            .any(|param| self_occurs(&param.ty, false))
        {
            crate::diagnostics::SelfCallPosition::Parameter
        } else {
            crate::diagnostics::SelfCallPosition::NestedInReturn
        }
    })
}

/// The concrete-receiver impl tier's AMBIGUITY verdict alone - consulted
/// even when the class-inherent tier found a method, because an own
/// method does not arbitrate an interface clash: two implemented
/// interfaces declaring the name still need `as<I>` qualification
/// (E0121, TIR's rule).
pub fn concrete_member_ambiguity<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
    name: &Name,
) -> Option<(Vec<InterfaceRef>, bool)> {
    match lookup_impl_member(db, facts, receiver, name) {
        InterfaceMemberLookup::Ambiguous { sources, is_field } => Some((sources, is_field)),
        _ => None,
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
) -> InterfaceMemberLookup<'db> {
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
                MemberDeclarer::VirtualMethod { interface, method } => {
                    if let Some((block, func)) = resolved.source_dispatch(db, name) {
                        MemberDeclarer::ImplMethod { block, func }
                    } else if let Some(block) = resolved.source_block() {
                        MemberDeclarer::ImplMethod {
                            block,
                            func: method,
                        }
                    } else {
                        let callable = resolved
                            .mounted_method(name)
                            .and_then(|method| {
                                crate::package_interface::resolved_exported_function(
                                    method,
                                    Vec::new(),
                                    Vec::new(),
                                )
                                .external
                            })
                            .or_else(|| external_interface_callable(db, &implemented.name, name));
                        match callable {
                            Some(callable) => MemberDeclarer::ExternalMethod(callable),
                            None => MemberDeclarer::VirtualMethod { interface, method },
                        }
                    }
                }
                external @ MemberDeclarer::ExternalVirtualField { .. } => {
                    match resolved.source_block() {
                        Some(block) => MemberDeclarer::ImplField { block },
                        None => external,
                    }
                }
                virtual_field @ MemberDeclarer::VirtualField { .. } => {
                    match resolved.source_block() {
                        Some(block) => MemberDeclarer::ImplField { block },
                        None => virtual_field,
                    }
                }
                MemberDeclarer::ExternalMethod(callable) => resolved
                    .mounted_method(name)
                    .and_then(|method| {
                        crate::package_interface::resolved_exported_function(
                            method,
                            Vec::new(),
                            Vec::new(),
                        )
                        .external
                    })
                    .map(MemberDeclarer::ExternalMethod)
                    .unwrap_or(MemberDeclarer::ExternalMethod(callable)),
                concrete => concrete,
            };
            providers.push((implemented, member));
        }
    }
    // Interface FIELDS on a concrete receiver are projection-only (TIR's
    // rule): a class-owned field of the same name resolved earlier in the
    // caller's ladder, so a field reached here needs `obj.as<I>.field` -
    // and two distinct declaring interfaces make even that ambiguous.
    let field_sources: Vec<InterfaceRef> = providers
        .iter()
        .filter(|(_, member)| !member.is_method)
        .map(|(target, _)| target.clone())
        .collect();
    if field_sources.len() >= 2 {
        return InterfaceMemberLookup::Ambiguous {
            sources: field_sources,
            is_field: true,
        };
    }
    if let Some(interface) = field_sources.into_iter().next() {
        return InterfaceMemberLookup::FieldRequiresProjection { interface };
    }

    // Interface methods: the most-derived provider shadows the ones it
    // `requires` (root-wins); distinct survivors are ambiguous (E0121).
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
        0 => InterfaceMemberLookup::NotFound,
        1 => {
            let (_, member) = providers.pop().expect("len checked");
            InterfaceMemberLookup::Found(member)
        }
        _ => InterfaceMemberLookup::Ambiguous {
            sources: providers.into_iter().map(|(target, _)| target).collect(),
            is_field: false,
        },
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
    if crate::package_interface::mounted_type_row(db, &interface_ref.name).is_some() {
        return match crate::impls::realized_assoc_bound(db, interface_ref, base, member)
            .and_then(|ty| InterfaceRef::of_ty(&ty))
        {
            Some(bound) => vec![bound],
            None => Vec::new(),
        };
    }
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
    let bound_ty = ctx.lower_type_ref_at(
        &data.type_refs,
        bound,
        crate::lower::TypePosition::ConstraintHead,
    );
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
    for (param, bounds) in resolved.facts.generic_params() {
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
    if let Some(crate::package_interface::ExportedType::Interface {
        self_param,
        generic_params,
        associated_types,
        fields,
        required_methods,
        default_methods,
        ..
    }) = crate::package_interface::mounted_type_row(db, &target.name)
    {
        let instantiation = crate::impls::mounted_interface_instantiation(
            target,
            receiver,
            generic_params,
            associated_types,
        )?;
        if let Some(index) = fields.iter().position(|(field, ..)| field == name) {
            let (_, field_ty, _) = &fields[index];
            let field_ty =
                crate::lower::substitute_params(&Ty::from_plain(field_ty), &instantiation);
            return Some(InterfaceMember {
                ty: field_ty,
                is_method: false,
                pending_own: None,
                declarer: MemberDeclarer::ExternalVirtualField {
                    interface: target.name.clone(),
                    realized: target.clone(),
                    field_index: u32::try_from(index)
                        .expect("mounted interface field count fits u32"),
                },
            });
        }
        if let Some(method) = required_methods
            .iter()
            .chain(default_methods)
            .find(|method| method.name == *name)
        {
            let function = crate::package_interface::resolved_exported_function(
                method,
                Vec::new(),
                Vec::new(),
            );
            if existential && exported_signature_breaks_one_self(&function, self_param) {
                return None;
            }
            let pending_own =
                (!function.generic_params.is_empty()).then(|| PendingOwnGenerics::External {
                    function: Box::new(function.clone()),
                    prefix: instantiation.clone(),
                });
            let callable = function
                .external
                .clone()
                .expect("an exported interface method has an external target");
            return Some(InterfaceMember {
                ty: instantiate_external_signature(&function, &instantiation),
                is_method: true,
                pending_own,
                declarer: MemberDeclarer::ExternalMethod(callable),
            });
        }
        return None;
    }
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
                field_index: u32::try_from(index).expect("interface field count fits u32"),
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
        let pending_own = (signature.generic_params.len() > instantiation.len()).then(|| {
            PendingOwnGenerics::Source {
                method,
                prefix: instantiation.clone(),
            }
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

// ── Enumeration: the completion counterpart of the lookups above ────────────

/// One member a receiver exposes, as ENUMERATION sees it.
///
/// Deliberately name-shaped: the member's TYPE comes from running the name
/// back through [`lookup_method`] / [`lookup_interface_member`], so a caller
/// can never offer a member that resolution would not find, and the two can
/// never disagree about what it means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberCandidate<'db> {
    pub name: Name,
    /// A method (call it) rather than a field (read it).
    pub is_method: bool,
    /// Declared without a `self` receiver, so it is reached through the TYPE
    /// (`int.parse("3")`), never through a value. Fields are never static.
    pub is_static: bool,
    pub source: MemberSource,
    /// Where the member is declared, as the walk that found it already knew.
    /// Handing this back costs nothing and saves every consumer a second
    /// by-name search through the same data to render a signature.
    pub decl: MemberDecl<'db>,
}

/// The declaration an enumerated member came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberDecl<'db> {
    /// A method with a source declaration — class-inherent, or an
    /// interface's required/default method.
    Method(FunctionLoc<'db>),
    /// A field of a source class, by index into its declared field list.
    ClassField { class: ClassLoc<'db>, index: usize },
    /// A field of a source interface, by index into its declared field list.
    InterfaceField {
        interface: InterfaceLoc<'db>,
        index: usize,
    },
    /// An enum variant, by index into its declared variant list.
    EnumVariant {
        enum_loc: EnumLoc<'db>,
        index: usize,
    },
    /// Declared by a mounted package: there is no source declaration to
    /// point at, only the exported row.
    Mounted,
}

/// The tier an enumerated member came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemberSource {
    /// The receiver's own class declares it — the class-inherent tier.
    Inherent,
    /// An interface the receiver reaches declares it: through a bound, an
    /// existential, an assoc-type bound, or an impl the receiver matches.
    Interface(InterfaceRef),
}

/// Every member name `receiver` exposes, in the resolution ladder's own
/// order: class-inherent first, then the interfaces the receiver reaches.
///
/// This walks exactly the tiers [`lookup_method`] and
/// [`lookup_interface_member`] walk, sharing `member_roots` with them, and
/// the first spelling of a name wins — which is the ladder's precedence, so
/// what is enumerated is what would resolve. Members two interfaces declare
/// ambiguously (E0121) are still enumerated once: the fix for an ambiguous
/// member is to qualify it, and a completion that hid the name could not
/// lead anyone there.
///
/// Language-internal members are omitted. That is not presentation: they are
/// "outside BAML's user-facing language surface" (`FunctionMetadata`), so no
/// source position can name one.
pub fn member_candidates<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
) -> Vec<MemberCandidate<'db>> {
    let mut out: Vec<MemberCandidate<'db>> = Vec::new();

    // Tier 1: the receiver's own class (`lookup_method`'s two branches).
    if let Some((class, _)) = receiver_class(facts, receiver, 8) {
        let data = baml_compiler2_ppir::item_data::class_data(db, class);
        for (index, field) in data.fields.iter().enumerate() {
            push_candidate(
                &mut out,
                field.name.clone(),
                false,
                false,
                MemberSource::Inherent,
                MemberDecl::ClassField { class, index },
            );
        }
        for (name, is_static, method) in declared_methods(db, &data.methods) {
            push_candidate(
                &mut out,
                name,
                true,
                is_static,
                MemberSource::Inherent,
                MemberDecl::Method(method),
            );
        }
    } else if let Some((qtn, _)) = external_class_for_type(facts, receiver, 8)
        && let Some(crate::package_interface::ExportedType::Class {
            fields, methods, ..
        }) = crate::package_interface::mounted_type_row(db, &qtn)
    {
        for (name, ..) in fields {
            push_candidate(
                &mut out,
                name.clone(),
                false,
                false,
                MemberSource::Inherent,
                MemberDecl::Mounted,
            );
        }
        for method in methods {
            push_candidate(
                &mut out,
                method.name.clone(),
                true,
                is_static_exported(method),
                MemberSource::Inherent,
                MemberDecl::Mounted,
            );
        }
    }

    // Tier 2: the interfaces the receiver reaches — the same split the
    // lookups make between symbolic receivers (bounds) and concrete ones
    // (the impls they match).
    match member_roots(db, facts, receiver) {
        Some((roots, existential)) => {
            for root in &roots {
                extend_from_interface(db, facts, &mut out, root, existential);
                for required in crate::impls::direct_requires_closure(db, root, receiver, 8) {
                    extend_from_interface(db, facts, &mut out, &required, existential);
                }
            }
        }
        None => {
            for resolved in crate::impls::impls_for_type(db, receiver) {
                if !env_discharges_rigid_bounds(db, facts, &resolved) {
                    continue;
                }
                let implemented = resolved.implemented();
                extend_from_interface(db, facts, &mut out, &implemented, false);
            }
        }
    }

    out
}

/// The members a TYPE exposes to a qualified access: `int.parse("3")`,
/// `Range.new(0, 10)`, `baml.ops.Compare.cmp(a, b)`.
///
/// Keyed on the DECLARATION rather than a `Ty` because a type qualifier
/// names a declaration, not a value of it — `Range` is not `Range<int>`, and
/// synthesizing generic arguments to ask the value-side question would be
/// inventing a type nobody wrote. Both static members and instance members
/// come back: BAML has UFCS, so `int.min(a, b)` is the same call as
/// `a.min(b)`, and `is_static` is what tells the two forms apart.
///
/// FIELDS are not members of a type. UFCS turns a `self` receiver into an
/// argument, and a field has no argument to become — `Point.x` and
/// `Greet.field` are both `unresolved name`, so neither is enumerated here.
/// Enum variants are the opposite case: `Status.Active` is the only way to
/// name one, so they are the type's members and nothing else's.
///
// BUG: UFCS through a COMPANION CARRIER does not type-check. `Point.norm(p)`
// works, but `int.abs(a)` / `string.includes(s, "x")` report a mismatch of
// `baml.Int` against `int`: a carrier method's `self` has the carrier CLASS
// type (`class_self_ty` returns `Class(baml.Int)` with no bridging), and a
// primitive is not a subtype of the class that carries its methods. The
// value-receiver path never compares the two, which is why `a.abs()` is
// fine. Fixing it means deciding what `Self` realizes to inside a carrier —
// a TYPE_SYSTEM.md question, not a local one — so the enumeration keeps
// reporting what the language says (UFCS reaches instance methods) rather
// than what today's checker manages.
pub fn type_member_candidates<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    definition: Definition<'db>,
) -> Vec<MemberCandidate<'db>> {
    let mut out = Vec::new();
    match definition {
        Definition::Class(class) => {
            let data = baml_compiler2_ppir::item_data::class_data(db, class);
            for (name, is_static, method) in declared_methods(db, &data.methods) {
                push_candidate(
                    &mut out,
                    name,
                    true,
                    is_static,
                    MemberSource::Inherent,
                    MemberDecl::Method(method),
                );
            }
        }
        Definition::Enum(enum_loc) => {
            let data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
            for (index, variant) in data.variants.iter().enumerate() {
                push_candidate(
                    &mut out,
                    variant.name.clone(),
                    false,
                    true,
                    MemberSource::Inherent,
                    MemberDecl::EnumVariant { enum_loc, index },
                );
            }
        }
        Definition::Interface(interface) => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
            // No one-`Self` exclusion here: a type qualifier names `Self`
            // explicitly (`(T as Iface).m` or inferred), so every method is
            // reachable — the exclusion is the EXISTENTIAL receiver's.
            for (name, is_static, method) in declared_methods(db, &data.methods) {
                push_candidate(
                    &mut out,
                    name,
                    true,
                    is_static,
                    MemberSource::Inherent,
                    MemberDecl::Method(method),
                );
            }
        }
        // Only a type declaration has members to qualify into.
        _ => {}
    }
    out
}

/// The user-facing methods among a declaration's method list, with their
/// staticness — the one walk behind every enumeration of what a class or
/// interface declares, so the value rung and the type rung cannot disagree
/// about it. Language-internal members are dropped here, once: they are
/// "outside BAML's user-facing language surface" (`FunctionMetadata`), so no
/// source position can name one.
fn declared_methods<'a, 'db: 'a>(
    db: &'db dyn baml_compiler2_ppir::Db,
    methods: &'a [FunctionLoc<'db>],
) -> impl Iterator<Item = (Name, bool, FunctionLoc<'db>)> + 'a {
    methods.iter().filter_map(move |&method| {
        let function = baml_compiler2_ppir::item_data::function_data(db, method);
        if function.metadata.is_language_internal {
            return None;
        }
        Some((function.name.clone(), is_static_source(db, method), method))
    })
}

fn push_candidate<'db>(
    out: &mut Vec<MemberCandidate<'db>>,
    name: Name,
    is_method: bool,
    is_static: bool,
    source: MemberSource,
    decl: MemberDecl<'db>,
) {
    if out.iter().any(|candidate| candidate.name == name) {
        return;
    }
    out.push(MemberCandidate {
        name,
        is_method,
        is_static,
        source,
        decl,
    });
}

/// Whether a source function is declared without a `self` receiver, and is
/// therefore reached through the type rather than through a value.
fn is_static_source(db: &dyn baml_compiler2_ppir::Db, method: FunctionLoc<'_>) -> bool {
    baml_compiler2_ppir::item_data::function_data(db, method)
        .params
        .first()
        .is_none_or(|param| param.name.as_str() != "self")
}

/// The exported twin of [`is_static_source`], over the same predicate the
/// dispatch shape is built from.
fn is_static_exported(method: &crate::package_interface::ExportedFunction) -> bool {
    !crate::package_interface::exported_takes_self(method)
}

fn extend_from_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    out: &mut Vec<MemberCandidate<'db>>,
    target: &InterfaceRef,
    existential: bool,
) {
    for (name, is_method, is_static, decl) in interface_member_rows(db, facts, target, existential)
    {
        push_candidate(
            out,
            name,
            is_method,
            is_static,
            MemberSource::Interface(target.clone()),
            decl,
        );
    }
}

/// The member rows `target` declares, as `(name, is_method)`.
///
/// The row SOURCES are [`member_on_interface`]'s, branch for branch — a
/// mounted interface row, else the source declaration — including its
/// one-`Self` exclusion for existential receivers. Keep the two in step: this
/// answers "what can be written", that one answers "what does it mean".
fn interface_member_rows<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    target: &InterfaceRef,
    existential: bool,
) -> Vec<(Name, bool, bool, MemberDecl<'db>)> {
    let mut rows = Vec::new();
    if let Some(crate::package_interface::ExportedType::Interface {
        self_param,
        fields,
        required_methods,
        default_methods,
        ..
    }) = crate::package_interface::mounted_type_row(db, &target.name)
    {
        for (name, ..) in fields {
            rows.push((name.clone(), false, false, MemberDecl::Mounted));
        }
        for method in required_methods.iter().chain(default_methods) {
            let function = crate::package_interface::resolved_exported_function(
                method,
                Vec::new(),
                Vec::new(),
            );
            if existential && exported_signature_breaks_one_self(&function, self_param) {
                continue;
            }
            rows.push((
                method.name.clone(),
                true,
                is_static_exported(method),
                MemberDecl::Mounted,
            ));
        }
        return rows;
    }
    let Some(Definition::Interface(interface)) = facts.definition_of(&target.name) else {
        return rows;
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    for (index, field) in data.fields.iter().enumerate() {
        rows.push((
            field.name.clone(),
            false,
            false,
            MemberDecl::InterfaceField { interface, index },
        ));
    }
    for (name, is_static, method) in declared_methods(db, &data.methods) {
        if existential && signature_breaks_one_self(crate::lower::function_signature(db, method)) {
            continue;
        }
        rows.push((name, true, is_static, MemberDecl::Method(method)));
    }
    rows
}

pub(crate) fn instantiate_external_signature(
    function: &crate::package_interface::ResolvedFunction,
    instantiation: &[Ty],
) -> Ty {
    let params = function
        .params
        .iter()
        .map(|param| baml_type::interned::FunctionParam {
            name: param.name.clone(),
            ty: crate::lower::substitute_params(&Ty::from_plain(&param.ty), instantiation),
            mode: param.mode,
        })
        .collect();
    Ty::intern(TyKind::Function {
        params,
        ret: crate::lower::substitute_params(&Ty::from_plain(&function.return_type), instantiation),
        throws: crate::lower::substitute_params(
            &Ty::from_plain(&function.callable_throws),
            instantiation,
        ),
        attr: TyAttr::default(),
    })
}

fn exported_signature_breaks_one_self(
    function: &crate::package_interface::ResolvedFunction,
    self_param: &ParamTy,
) -> bool {
    let contains = |ty: &baml_type::Ty, top_ok: bool| {
        self_occurs(&Ty::from_plain(ty), top_ok)
            || matches!(ty, baml_type::Ty::TypeVar(param, _) if param == self_param && !top_ok)
    };
    function
        .params
        .iter()
        .skip(1)
        .any(|param| contains(&param.ty, false))
        || contains(&function.return_type, true)
        || contains(&function.callable_throws, true)
}

fn external_interface_callable(
    db: &dyn baml_compiler2_ppir::Db,
    interface: &baml_type::QualifiedTypeName,
    name: &Name,
) -> Option<std::sync::Arc<crate::callable::ExternalCallable>> {
    let package = baml_compiler2_hir::package::PackageId::new(db, interface.package().clone());
    let row = crate::package_interface::package_interface(db, package)
        .lookup_type(interface.namespace(), interface.name())?;
    let crate::package_interface::ExportedType::Interface {
        required_methods,
        default_methods,
        ..
    } = row
    else {
        return None;
    };
    let method = required_methods
        .iter()
        .chain(default_methods)
        .find(|method| method.name == *name)?;
    crate::package_interface::resolved_exported_function(method, Vec::new(), Vec::new()).external
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

// ── Union member resolution (TIR's `resolve_union_member`) ─────────────

/// The outcome of resolving a member on a UNION receiver. A union behaves
/// as the intersection existential over the interfaces every arm
/// provides: the member resolves through the SINGLE shared interface that
/// declares it - inference sugar for `union.as<I>.member`. Class-owned
/// fields/methods of the arms are never reachable (they need not agree
/// across arms).
pub enum UnionMemberLookup<'db> {
    Found(InterfaceMember<'db>),
    /// Two or more shared interfaces declare the member.
    Ambiguous {
        sources: Vec<InterfaceRef>,
        is_field: bool,
    },
    /// The single shared declarer's method uses `Self` outside the
    /// receiver position - uncallable through the union view.
    SelfRestricted {
        interface: InterfaceRef,
        position: crate::diagnostics::SelfCallPosition,
    },
    /// Every arm is a concrete class carrying an OWN field of this name
    /// at the SAME type - the spec's TS-style member-wise read (one
    /// agreed declaration, no interface view to record). Methods never
    /// take this road, and disagreeing field types fall through to
    /// `NoCommonInterface`.
    ClassFieldJoin(Ty),
    /// No interface shared by every arm declares the member.
    NoCommonInterface,
}

pub fn lookup_union_member<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    union_ty: &Ty,
    members: &[Ty],
    name: &Name,
) -> UnionMemberLookup<'db> {
    let mut shared: Option<Vec<InterfaceRef>> = None;
    for arm in members {
        let arm_ifaces = union_arm_interfaces(db, facts, arm, 4);
        shared = Some(match shared {
            None => arm_ifaces,
            Some(mut current) => {
                current.retain(|iface| arm_ifaces.iter().any(|other| other == iface));
                current
            }
        });
        if shared.as_ref().is_some_and(Vec::is_empty) {
            break;
        }
    }
    let mut declarers: Vec<(InterfaceRef, bool)> = Vec::new();
    for iface in shared.unwrap_or_default() {
        if let Some(is_field) = interface_declares_member(db, &iface, name)
            && !declarers.iter().any(|(seen, _)| *seen == iface)
        {
            declarers.push((iface, is_field));
        }
    }
    match declarers.len() {
        0 => match union_class_field_join(db, facts, members, name) {
            Some(ty) => UnionMemberLookup::ClassFieldJoin(ty),
            None => UnionMemberLookup::NoCommonInterface,
        },
        1 => {
            let (iface, _) = declarers.pop().expect("len checked");
            // The virtual dispatch goes through the shared interface, but
            // `Self` binds to the union itself (the subtype of the
            // interface existential), so a `Self`-returning method yields
            // the union rather than the erased interface. One-`Self`
            // object safety applies exactly as for an existential.
            match member_on_interface(db, facts, &iface, union_ty, name, true) {
                Some(member) => UnionMemberLookup::Found(member),
                None => match declared_method_self_restriction(db, facts, &iface, name) {
                    Some(position) => UnionMemberLookup::SelfRestricted {
                        interface: iface,
                        position,
                    },
                    None => UnionMemberLookup::NoCommonInterface,
                },
            }
        }
        _ => UnionMemberLookup::Ambiguous {
            is_field: declarers[0].1,
            sources: declarers.into_iter().map(|(iface, _)| iface).collect(),
        },
    }
}

/// The realized interfaces one union arm provides: an interface
/// existential contributes itself plus its `requires` closure; a bounded
/// type variable contributes each conjunct's contribution; a concrete arm
/// contributes its matched impls' realized interfaces.
fn union_arm_interfaces<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    arm: &Ty,
    fuel: u32,
) -> Vec<InterfaceRef> {
    if fuel == 0 {
        return Vec::new();
    }
    match arm.kind() {
        TyKind::Interface(qtn, args, pins, _) => {
            let root = InterfaceRef::new(qtn.clone(), (args.to_vec()).into(), pins.to_vec());
            let mut out = vec![root.clone()];
            for required in crate::impls::direct_requires_closure(db, &root, arm, 8) {
                if !out.contains(&required) {
                    out.push(required);
                }
            }
            out
        }
        TyKind::TypeVar(param, _) => {
            let mut out = Vec::new();
            for bound in baml_type::normalize::TypeContext::type_var_bound(facts, param) {
                let root = InterfaceRef::from_constraint(&bound);
                if !out.contains(&root) {
                    out.push(root.clone());
                }
                for required in crate::impls::direct_requires_closure(db, &root, arm, 8) {
                    if !out.contains(&required) {
                        out.push(required);
                    }
                }
            }
            out
        }
        _ => {
            let mut out = Vec::new();
            for resolved in crate::impls::impls_for_type(db, arm) {
                if !env_discharges_rigid_bounds(db, facts, &resolved) {
                    continue;
                }
                let implemented = resolved.implemented();
                if !out.contains(&implemented) {
                    out.push(implemented);
                }
            }
            out
        }
    }
}

/// Whether the interface declares `name`, and as which kind
/// (`Some(true)` = field). Side-effect-free - used to count shared
/// declarers before committing to a virtual resolution.
///
/// A thin reading of the shared declaration oracle: value-namespace
/// membership is one question, asked here and by item-projection
/// determination alike.
fn interface_declares_member(
    db: &dyn baml_compiler2_ppir::Db,
    target: &InterfaceRef,
    name: &Name,
) -> Option<bool> {
    use crate::interfaces::{InterfaceMemberKind, MemberNamespace, ValueMemberKind};
    match crate::interfaces::interface_declared_kind(
        db,
        &target.name,
        name,
        MemberNamespace::Value,
    )? {
        InterfaceMemberKind::Value(ValueMemberKind::Field) => Some(true),
        InterfaceMemberKind::Value(ValueMemberKind::Method) => Some(false),
        InterfaceMemberKind::AssociatedType => None,
    }
}

/// The spec's TS-style union field read: every arm is a concrete class
/// whose OWN declared field `name` (realized at the arm's args) exists,
/// and the access types as the JOIN of the per-arm field types - the
/// discriminant-narrowing shape (`x.type == "foo"` over a tagged class
/// union) reads the union of the tags. `None` whenever an arm is
/// symbolic (interface/var arms only reach members through a shared
/// interface) or lacks the field.
fn union_class_field_join<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    members: &[Ty],
    name: &Name,
) -> Option<Ty> {
    let mut parts: Vec<Ty> = Vec::new();
    for arm in members {
        if matches!(
            arm.kind(),
            TyKind::Interface(..) | TyKind::TypeVar(..) | TyKind::AssociatedTypeProjection { .. }
        ) {
            return None;
        }
        let field_ty = if let Some((class, class_args)) = receiver_class(facts, arm, 8) {
            crate::lower::class_field_types(db, class)
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, ty)| crate::lower::substitute_params(ty, &class_args))?
        } else if let TyKind::Class(qtn, class_args, _) = arm.kind()
            && let Some(crate::package_interface::ExportedType::Class { fields, .. }) =
                crate::package_interface::mounted_type_row(db, qtn)
        {
            let (_, ty, _) = fields.iter().find(|(field, ..)| field == name)?;
            crate::lower::substitute_params(&Ty::from_plain(ty), class_args)
        } else {
            return None;
        };
        parts.push(field_ty);
    }
    (!parts.is_empty()).then(|| Ty::union(parts))
}
