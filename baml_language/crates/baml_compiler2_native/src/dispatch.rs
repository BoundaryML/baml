//! Which function an interface method call runs.
//!
//! A program is compiled whole, so every type an interface value can hold is
//! known when it is packed. A call on a receiver whose type is known where
//! the call is written is resolved there, to a direct call. A call on a
//! receiver that is only known to be some implementor loads its code from a
//! table: the receiver's type id picks a row, the method a column.
//!
//! Which implementation a type has is the type checker's own answer
//! ([`resolve_implementation`]), asked with the native type mapped back to
//! the checker's.

use baml_base::Name;
use baml_compiler2_hir::{
    item_data::{function_data, function_has_body, interface_data},
    loc::DeclRef,
};
use baml_compiler2_hir_ty::{
    impls::{ResolvedImplementation, resolve_impl, resolve_implementation},
    layout,
};
use baml_compiler2_mir::{TyTemplate, TyTemplateInterface, resolved_aliases_for_package};
use baml_type::{
    DeclName, StructuralInterface,
    interned::{ClosedTy, InferInterface, Ty},
};

use crate::{
    Blocker,
    program::{Entry, FnKey},
    types::{NativeTy, Signature, TypeArg, native_ty, template_ty, type_argument},
};

/// What runs for an interface method on a receiver of one type.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Target<'db> {
    /// A function written in source: what the type's implementation
    /// provides, or the interface's default body.
    Function(FnKey<'db>),
    /// A method the runtime implements in Rust, called where the
    /// receiver's type is known.
    Native(&'static crate::natives::Native),
    /// The language's own `to_string` of a type that declares none.
    ToString,
    /// The standard library's `sort` of an array whose elements the runtime
    /// orders itself, one of [`crate::layout::SORT_INTS`] and the kinds
    /// after it: the standard library's own implementation sorts these in
    /// Rust too.
    Sort(usize),
}

/// One method of one interface: what a column of the method table is for.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Slot {
    /// The interface with its arguments, spelled out.
    pub(crate) interface: String,
    pub(crate) method: String,
    /// The method's own type arguments, spelled out: each set of them is
    /// another copy of every implementation.
    pub(crate) arguments: String,
}

impl Slot {
    pub(crate) fn new(
        interface: &TyTemplateInterface,
        method: &str,
        arguments: &[TypeArg<'_>],
    ) -> Self {
        Self {
            interface: format!("{interface:?}"),
            method: method.to_owned(),
            arguments: format!("{arguments:?}"),
        }
    }

    /// The parameters after the receiver and the result every
    /// implementation of the language's `to_string` has.
    pub(crate) fn to_string_signature<'db>() -> Signature<'db> {
        Signature {
            params: Vec::new(),
            result: NativeTy::String,
        }
    }
}

/// A native type as the type checker spells it, when it is one an interface
/// can be implemented for.
fn plain(db: &dyn baml_compiler2_mir::Db, ty: &NativeTy<'_>) -> Option<baml_type::Ty> {
    Some(match ty {
        NativeTy::Int => baml_type::Ty::Int,
        NativeTy::Float => baml_type::Ty::Float,
        NativeTy::Bool => baml_type::Ty::Bool,
        NativeTy::String => baml_type::Ty::String,
        NativeTy::Bigint => baml_type::Ty::Bigint,
        NativeTy::Bytes => baml_type::Ty::Uint8Array,
        NativeTy::Unknown => baml_type::Ty::Unknown,
        NativeTy::Enum(enum_ref) => baml_type::Ty::Enum(layout::enum_head(db, *enum_ref)),
        NativeTy::Class(class, arguments) => baml_type::Ty::Class(
            layout::class_head(db, *class),
            arguments
                .iter()
                .map(|argument| match argument {
                    Some(argument) => plain(db, argument),
                    // No value of the argument is held: `never`, the error
                    // type of code that throws nothing.
                    None => Some(baml_type::Ty::Never),
                })
                .collect::<Option<_>>()?,
        ),
        NativeTy::Array(element) => baml_type::Ty::List(Box::new(plain(db, element)?)),
        _ => return None,
    })
}

/// A whole type of the checker's as native code holds its values.
fn native<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    interface: &DeclName,
    ty: &Ty,
) -> Option<NativeTy<'db>> {
    let ty = ClosedTy::try_from(ty).ok()?.to_plain();
    let aliases = resolved_aliases_for_package(db, interface.root());
    native_ty(db, &aliases.0.convert(&ty), &[])
}

/// What an implementor of type `base` binds the associated type `member`
/// of `interface` to.
pub(crate) fn associated<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    base: &NativeTy<'db>,
    interface: &DeclName,
    member: &Name,
) -> Option<NativeTy<'db>> {
    let concrete = Ty::from_plain(&plain(db, base)?);
    let goal = InferInterface::new(interface.clone(), Box::new([]), Box::new([]));
    let view = resolve_impl(db, &concrete, &goal)?.implemented_view(db, &concrete);
    let (_, bound) = view
        .associated_types
        .iter()
        .find(|(name, _)| name == member)?;
    native(db, interface, bound)
}

/// `interface` as a copy of a function compiled for `arguments` names it:
/// with the frame's types in place of its type parameters. One held as any
/// reference is `unknown`, which is how [`resolve`] compares it. `None`
/// when the frame has no such type.
pub(crate) fn in_frame<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    interface: &TyTemplateInterface,
    arguments: &[TypeArg<'db>],
) -> Option<TyTemplateInterface> {
    let mut known = true;
    let mut filled = |slot: u32| match arguments.get(slot as usize) {
        Some(Some(NativeTy::Int)) => TyTemplate::Int,
        Some(Some(NativeTy::Float)) => TyTemplate::Float,
        Some(Some(NativeTy::Bool)) => TyTemplate::Bool,
        Some(Some(NativeTy::Enum(enum_ref))) => TyTemplate::Enum(layout::enum_head(db, *enum_ref)),
        Some(Some(ty)) if ty.is_ref() => TyTemplate::Unknown,
        // No value of the argument is held: `never`.
        Some(None) => TyTemplate::Never,
        _ => {
            known = false;
            TyTemplate::TypeArgRef(slot)
        }
    };
    let mut interface = interface.clone();
    for generic in &mut interface.generics {
        generic.replace_type_arg_refs(&mut filled);
    }
    for (_, bound) in &mut interface.associated_types {
        bound.replace_type_arg_refs(&mut filled);
    }
    known.then_some(interface)
}

/// The checker's view of an interface whose arguments are whole types.
fn goal(interface: &TyTemplateInterface) -> Option<InferInterface> {
    let whole = |template: &TyTemplate| {
        template
            .is_fully_concrete()
            .then(|| Ty::from_plain(&template.to_display_ty()))
    };
    Some(InferInterface::new(
        interface.name.clone(),
        interface
            .generics
            .iter()
            .map(whole)
            .collect::<Option<_>>()?,
        interface
            .associated_types
            .iter()
            .map(|(name, binding)| Some((name.clone(), whole(binding)?)))
            .collect::<Option<_>>()?,
    ))
}

/// Whether values of type `ty` are values of the interface type.
pub(crate) fn implements(
    db: &dyn baml_compiler2_mir::Db,
    ty: &NativeTy<'_>,
    interface: &TyTemplateInterface,
) -> bool {
    match (goal(interface), plain(db, ty)) {
        (Some(goal), Some(concrete)) => {
            resolve_implementation(db, &Ty::from_plain(&concrete), &goal).is_some()
        }
        _ => false,
    }
}

/// What `interface`'s `method` runs on a receiver of type `receiver`:
/// `None` when the type does not implement the interface, a blocker when it
/// does in a way native code is not generated for. `arguments` are those of
/// the frame the interface is written in, and `method_arguments` the
/// method's own, which end the frame of the function that runs.
pub(crate) fn resolve<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    receiver: &NativeTy<'db>,
    interface: &TyTemplateInterface,
    method: &str,
    arguments: &[TypeArg<'db>],
    method_arguments: &[TypeArg<'db>],
) -> Result<Option<Target<'db>>, Blocker> {
    let unsupported =
        |detail: &str| Blocker::new("interface method call", format!("{method}: {detail}"));
    if let NativeTy::Array(element) = receiver
        && method == "sort"
        && interface.name.name().as_str() == "Sortable"
        && interface.name.namespace().is_empty()
        && interface.name.key().kind(db) == baml_base::SourceRootKind::Stdlib
    {
        return match **element {
            NativeTy::Int => Ok(Some(Target::Sort(crate::layout::SORT_INTS))),
            NativeTy::Float => Ok(Some(Target::Sort(crate::layout::SORT_FLOATS))),
            NativeTy::String => Ok(Some(Target::Sort(crate::layout::SORT_STRINGS))),
            NativeTy::Bigint => Ok(Some(Target::Sort(crate::layout::SORT_BIGINTS))),
            _ => Err(unsupported("of elements with a compare of their own")),
        };
    }
    let goal = goal(interface).ok_or_else(|| unsupported("on an interface of type parameters"))?;
    let Some(concrete) = plain(db, receiver) else {
        return Ok(None);
    };
    let name = Name::new(method);
    // The implementation is found by the interface's name and arguments.
    // A type argument held as any reference is not the type the interface
    // names, so what the implementation binds the associated types to is
    // compared below, as far as native code tells types apart.
    let unpinned = InferInterface::new(goal.name.clone(), goal.generics.clone(), Box::new([]));
    match resolve_implementation(db, &Ty::from_plain(&concrete), &unpinned) {
        None => Ok(None),
        Some(ResolvedImplementation::StructuralDefault(StructuralInterface::ToString))
            if method == StructuralInterface::ToString.primary_method() =>
        {
            Ok(Some(Target::ToString))
        }
        Some(ResolvedImplementation::StructuralDefault(_)) => {
            Err(unsupported("the language's own implementation"))
        }
        Some(ResolvedImplementation::Explicit(resolved)) => {
            let subject = Ty::from_plain(&concrete);
            let view = resolved.implemented_view(db, &subject);
            let held = |ty: &Ty| type_argument(native(db, &interface.name, ty));
            let differs = goal.associated_types.iter().any(|(member, wanted)| {
                view.associated_types
                    .iter()
                    .any(|(bound, found)| bound == member && held(found) != held(wanted))
            });
            if differs {
                return Ok(None);
            }
            match resolved.provided_method(db, &name) {
                // The frame of what an implementation provides is the
                // implementation's type parameters, as the receiver's type
                // binds them, and then the method's own.
                Some(DeclRef::Source(function)) => {
                    if let Some(native) = crate::natives::of(db, DeclRef::Source(function)) {
                        if !resolved.facts.generic_params().is_empty() {
                            return Err(unsupported(
                                "a native of an implementation with type parameters",
                            ));
                        }
                        return native.callable().map(|_| Some(Target::Native(native)));
                    }
                    let frame = resolved
                        .facts
                        .generic_params()
                        .iter()
                        .map(|(parameter, _)| {
                            let bound = resolved.bindings.get(parameter).ok_or_else(|| {
                                unsupported("an implementation with a type parameter left open")
                            })?;
                            Ok(type_argument(native(db, &interface.name, bound)))
                        })
                        .chain(method_arguments.iter().cloned().map(Ok))
                        .collect::<Result<_, Blocker>>()?;
                    Ok(Some(Target::Function(
                        FnKey::new(function, Entry::Direct).with_arguments(frame),
                    )))
                }
                Some(DeclRef::External(_)) => Err(unsupported("an implementation without source")),
                // The interface's default body, whose frame is `Self`, the
                // interface's arguments and then the method's own. A class
                // is `Self` whole: what it binds the interface's associated
                // types to is part of the body's types.
                None => {
                    let Some(DeclRef::Source(declaring)) =
                        layout::interface_ref_of(db, &interface.name)
                    else {
                        return Err(unsupported("a default body without source"));
                    };
                    let function = interface_data(db, declaring)
                        .methods
                        .iter()
                        .copied()
                        .find(|&declared| {
                            function_data(db, declared).name == name
                                && function_has_body(db, declared)
                        })
                        .ok_or_else(|| unsupported("no body"))?;
                    let own = match receiver {
                        NativeTy::Class(..) => Some(receiver.clone()),
                        other => type_argument(Some(other.clone())),
                    };
                    let frame = std::iter::once(own)
                        .chain(
                            interface
                                .generics
                                .iter()
                                .map(|generic| type_argument(template_ty(db, generic, arguments))),
                        )
                        .chain(method_arguments.iter().cloned())
                        .collect();
                    Ok(Some(Target::Function(
                        FnKey::new(function, Entry::Direct).with_arguments(frame),
                    )))
                }
            }
        }
    }
}
