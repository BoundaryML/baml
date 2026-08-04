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
    loc::{ClassLoc, FunctionLoc, InterfaceLoc},
};
use baml_type::{
    Literal, MediaKind, Name, ParamTy, TyAttr, TypeName,
    interned::{Ty, TyKind},
    normalize::TypeContext as _,
};

use crate::facts::Facts;
use crate::impls::InterfaceTarget;

/// One resolved method: the function plus the receiver-driven
/// instantiation of its owning class's generic params (the frame prefix
/// that `function_generic_frame` prepends for methods).
pub struct MethodCandidate<'db> {
    pub method: FunctionLoc<'db>,
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
        .find(|&method| {
            baml_compiler2_ppir::item_data::function_data(db, method).name == *name
        })?;
    Some(MethodCandidate { method, class_args })
}

/// The class whose declaration owns `receiver`'s methods, with the generic
/// arguments the receiver pins. This table IS the language's builtin-class
/// correspondence (TIR: `resolve_builtin_member` call sites), one row per
/// structural kind; literals defer to their base primitive's class.
fn receiver_class<'db>(
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
pub struct InterfaceMember {
    pub ty: Ty,
    /// Methods bind their receiver; fields do not take one.
    pub is_method: bool,
}

/// Resolves `name` as an interface member of `receiver` - an interface
/// existential (`Self` = the existential, one-`Self` gated) or a rigid
/// bounded var (`Self` = the variable; `Self`-typed params are sound).
/// Root-wins tiering over the `requires` closure; distinct realized
/// declarers are ambiguous (Error, S17's diagnostic). Concrete
/// receivers' impl-provided members join with the impls-for-receiver
/// step (pinned pending).
pub fn lookup_interface_member<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    receiver: &Ty,
    name: &Name,
) -> Option<InterfaceMember> {
    let (roots, existential): (Vec<InterfaceTarget>, bool) = match receiver.kind() {
        TyKind::Interface(qtn, args, pins, _) => (
            vec![InterfaceTarget {
                name: qtn.clone(),
                args: args.to_vec(),
                pins: pins.to_vec(),
            }],
            true,
        ),
        TyKind::TypeVar(param, _) => (
            baml_type::normalize::TypeContext::type_var_bound(facts, param)
                .iter()
                .map(InterfaceTarget::from_constraint)
                .collect(),
            false,
        ),
        // Concrete receivers resolve through the impls they match - the
        // trait-impl candidate tier (I6).
        _ => return lookup_impl_member(db, facts, receiver, name),
    };
    for root in &roots {
        // Root-wins: the directly-named interface shadows its closure.
        if let Some(member) = member_on_interface(db, facts, root, receiver, name, existential) {
            return Some(member);
        }
        // Then the requires closure, deduped by realized identity;
        // distinct declarers are ambiguous.
        let mut found: Option<(InterfaceTarget, InterfaceMember)> = None;
        for required in crate::impls::direct_requires_closure(db, root, receiver, 8) {
            if let Some(member) =
                member_on_interface(db, facts, &required, receiver, name, existential)
            {
                match &found {
                    Some((seen, _)) if *seen != required => return None,
                    Some(_) => {}
                    None => found = Some((required.clone(), member)),
                }
            }
        }
        if let Some((_, member)) = found {
            return Some(member);
        }
    }
    None
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
) -> Option<InterfaceMember> {
    let mut providers: Vec<(InterfaceTarget, InterfaceMember)> = Vec::new();
    for resolved in crate::impls::impls_for_type(db, receiver) {
        let implemented = resolved.implemented();
        if let Some(member) = member_on_interface(db, facts, &implemented, receiver, name, false)
            && !providers.iter().any(|(seen, _)| *seen == implemented)
        {
            providers.push((implemented, member));
        }
    }
    if providers.len() > 1 {
        let heads: Vec<InterfaceTarget> = providers.iter().map(|(target, _)| target.clone()).collect();
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

/// A member declared DIRECTLY on `target`, instantiated for `receiver`.
fn member_on_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &Facts<'db>,
    target: &InterfaceTarget,
    receiver: &Ty,
    name: &Name,
    existential: bool,
) -> Option<InterfaceMember> {
    let Some(Definition::Interface(interface)) = facts.definition_of(&target.name) else {
        return None;
    };
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let instantiation = interface_instantiation(receiver, target, data);

    // Fields first (mirroring the class path's field-before-method).
    if let Some(field) = data.fields.iter().find(|field| field.name == *name) {
        let frame = interface_frame(interface, db);
        let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db))
            .with_frame(frame)
            .with_bounds(crate::lower::interface_scope_bounds(db, interface));
        let field_ty = ctx.lower_type_ref(&data.type_refs, field.type_ref);
        return Some(InterfaceMember {
            ty: crate::lower::substitute_params(&field_ty, &instantiation),
            is_method: false,
        });
    }

    // Default methods carry bodies (ordinary FunctionLocs).
    if let Some(&method) = data.default_methods.iter().find(|&&method| {
        baml_compiler2_ppir::item_data::function_data(db, method).name == *name
    }) {
        let signature = crate::lower::function_signature(db, method);
        if existential && signature_breaks_one_self(&signature) {
            return None;
        }
        return Some(InterfaceMember {
            ty: instantiate_signature(&signature, &instantiation),
            is_method: true,
        });
    }

    // Required methods are signatures only.
    if let Some(sig) = data.required_methods.iter().find(|sig| sig.name == *name) {
        if !sig.generic_params.is_empty() {
            // Method-own generics on required signatures: rare; joins
            // with the obligations work.
            return None;
        }
        let frame = interface_frame(interface, db);
        let ctx = crate::lower::lower_ctx_for_file(db, interface.file(db))
            .with_frame(frame)
            .with_bounds(crate::lower::interface_scope_bounds(db, interface));
        let self_var = Ty::intern(TyKind::TypeVar(
            ParamTy::new(0, Name::new("Self")),
            TyAttr::default(),
        ));
        let params: Box<[baml_type::interned::FunctionParam]> = sig
            .params
            .iter()
            .map(|param| baml_type::interned::FunctionParam {
                name: Some(param.name.clone()),
                ty: match param.type_ref {
                    Some(type_ref) if param.name.as_str() != "self" => {
                        ctx.lower_type_ref(&data.type_refs, type_ref)
                    }
                    _ => self_var.clone(),
                },
                mode: if param.has_default {
                    baml_type::FunctionParamMode::Optional
                } else {
                    baml_type::FunctionParamMode::Required
                },
            })
            .collect();
        let ret = sig
            .return_type
            .map(|type_ref| ctx.lower_type_ref(&data.type_refs, type_ref))
            .unwrap_or_else(Ty::error);
        // Interface signatures MUST declare throws (spec rule 1).
        let throws = sig
            .throws
            .map(|type_ref| ctx.lower_type_ref(&data.type_refs, type_ref))
            .unwrap_or_else(Ty::error);
        let fn_ty = Ty::intern(TyKind::Function {
            params,
            ret,
            throws,
            attr: TyAttr::default(),
        });
        let signature_view = fn_ty.clone();
        if existential && fn_breaks_one_self(&signature_view) {
            return None;
        }
        return Some(InterfaceMember {
            ty: crate::lower::substitute_params(&fn_ty, &instantiation),
            is_method: true,
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
    target: &InterfaceTarget,
    data: &baml_compiler2_ppir::item_data::InterfaceData<'_>,
) -> Vec<Ty> {
    let mut out = vec![receiver.clone()];
    for (index, _) in data.generic_params.iter().enumerate() {
        out.push(target.args.get(index).cloned().unwrap_or_else(Ty::error));
    }
    let interface_ref = baml_type::interned::InterfaceRef::new(
        target.name.clone(),
        target.args.clone().into_boxed_slice(),
        target.pins.clone(),
    );
    for assoc in &data.associated_types {
        let slot = target
            .pins
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

fn instantiate_signature(
    signature: &crate::lower::FunctionSignature,
    instantiation: &[Ty],
) -> Ty {
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

fn fn_breaks_one_self(fn_ty: &Ty) -> bool {
    let TyKind::Function {
        params, ret, throws, ..
    } = fn_ty.kind()
    else {
        return false;
    };
    params.iter().skip(1).any(|param| self_occurs(&param.ty, false))
        || self_occurs(ret, true)
        || self_occurs(throws, true)
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
