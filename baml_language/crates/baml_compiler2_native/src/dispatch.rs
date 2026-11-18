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
    impls::{ResolvedImplementation, resolve_implementation},
    layout,
};
use baml_compiler2_mir::{TyTemplate, TyTemplateInterface};
use baml_type::{
    StructuralInterface,
    interned::{InferInterface, Ty},
};

use crate::{
    Blocker,
    program::{Entry, FnKey},
    types::{NativeTy, Signature, TypeArg, template_ty, type_argument},
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
}

impl Slot {
    pub(crate) fn new(interface: &TyTemplateInterface, method: &str) -> Self {
        Self {
            interface: format!("{interface:?}"),
            method: method.to_owned(),
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
        NativeTy::Unknown => baml_type::Ty::Unknown,
        NativeTy::Enum(enum_ref) => baml_type::Ty::Enum(layout::enum_head(db, *enum_ref)),
        NativeTy::Class(class, arguments) => baml_type::Ty::Class(
            layout::class_head(db, *class),
            arguments
                .iter()
                .map(|argument| plain(db, argument.as_ref()?))
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
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
/// does in a way native code is not generated for.
pub(crate) fn resolve<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    receiver: &NativeTy<'db>,
    interface: &TyTemplateInterface,
    method: &str,
    arguments: &[TypeArg<'db>],
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
    match resolve_implementation(db, &Ty::from_plain(&concrete), &goal) {
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
            match resolved.provided_method(db, &name) {
                Some(DeclRef::Source(function)) => {
                    if !resolved.facts.generic_params().is_empty() {
                        return Err(unsupported("an implementation with type parameters"));
                    }
                    if let Some(native) = crate::natives::of(db, DeclRef::Source(function)) {
                        return native.callable().map(|_| Some(Target::Native(native)));
                    }
                    Ok(Some(Target::Function(FnKey::new(function, Entry::Direct))))
                }
                Some(DeclRef::External(_)) => Err(unsupported("an implementation without source")),
                // The interface's default body, whose frame is `Self` and
                // then the interface's arguments.
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
                    let frame = std::iter::once(type_argument(Some(receiver.clone())))
                        .chain(
                            interface
                                .generics
                                .iter()
                                .map(|generic| type_argument(template_ty(db, generic, arguments))),
                        )
                        .collect();
                    Ok(Some(Target::Function(
                        FnKey::new(function, Entry::Direct).with_arguments(frame),
                    )))
                }
            }
        }
    }
}
