//! Monomorphization: a generic function at concrete type arguments.
//!
//! The VM erases generics: a generic function has one body, and a call
//! passes its type arguments as values (`Object::Type`) that the body reads
//! through `load_type(#n)` and that reflection can observe. Native code
//! has no type values, so a generic function is compiled once per
//! [`Instance`]: the body is analyzed with its [`Frame`] bound to the
//! checker's concrete type arguments at the call site, every type variable
//! and every `TypeArgRef` template realized before it is mapped to a
//! [`crate::types::NativeTy`], and the instance named after its arguments
//! (`user_identity__int`). Identical argument tuples share one instance
//! (D4). A type argument that is `unknown`, or that has no native type,
//! rejects the call (D3).

use std::fmt;

use baml_compiler2_hir::{
    item_data::{ImplSubjectData, MethodOwner, impl_block_data, method_owner},
    loc::FunctionLoc,
};
use baml_compiler2_hir_ty::{
    facts::Facts,
    lower::{class_generic_frame, class_qualified_name, function_generic_frame},
    package_interface::reduce_ground_projections,
};
use baml_compiler2_mir::{RealizedTy, RuntimeTy, TyTemplate};
use baml_type::{
    DeclName, ParamTy, RuntimeGenericLayout, Ty,
    unify::{rewrite_ty, substitute_ty},
};

/// How many associated type projections may reduce through one another
/// while a type is realized.
const PROJECTION_FUEL: u32 = 32;

use crate::{Rejection, types::Unsupported};

/// A declared function at concrete type arguments: the unit the module
/// compiles. A non-generic function is the instance with no arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Instance<'db> {
    pub loc: FunctionLoc<'db>,
    /// The runtime frame's type arguments, one per slot of
    /// [`function_generic_frame`] (an owner's parameters first, then the
    /// function's own), each fully realized.
    pub type_args: Vec<RealizedTy>,
}

impl<'db> Instance<'db> {
    /// The instance of a function that takes no type arguments.
    pub fn plain(loc: FunctionLoc<'db>) -> Self {
        Self {
            loc,
            type_args: Vec::new(),
        }
    }

    pub fn is_generic(&self) -> bool {
        !self.type_args.is_empty()
    }
}

/// The generic frame of one instance: which runtime slot each type
/// parameter of the function occupies, and the type bound to each slot.
#[derive(Clone)]
pub(crate) struct Frame<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    layout: RuntimeGenericLayout,
    args: Vec<RealizedTy>,
    /// For a method of a generic class (declared in it or in its in-body
    /// `implements` block): the class's head and parameters, which open the
    /// frame. Lowering types the method's `self` as the bare class, without
    /// its arguments; they are the frame's leading slots.
    owner: Option<(DeclName, Vec<ParamTy>)>,
}

impl fmt::Debug for Frame<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("params", &self.layout.params())
            .field("args", &self.args)
            .finish()
    }
}

/// Why an instance's argument list does not fit its function's frame.
pub(crate) enum FrameMismatch {
    /// A generic function instantiated with no arguments: a root, or a
    /// value, that the subset has no erased form for.
    Uninstantiated,
    /// The wrong number of arguments: a compiler bug.
    Arity { expected: usize, given: usize },
}

impl<'db> Frame<'db> {
    /// The frame of `loc` bound to `args`.
    pub(crate) fn new(
        db: &'db dyn baml_compiler2_mir::Db,
        loc: FunctionLoc<'db>,
        args: Vec<RealizedTy>,
    ) -> Result<Self, FrameMismatch> {
        let layout = RuntimeGenericLayout::new(&function_generic_frame(db, loc));
        let expected = layout.params().len();
        if expected != args.len() {
            return Err(if args.is_empty() {
                FrameMismatch::Uninstantiated
            } else {
                FrameMismatch::Arity {
                    expected,
                    given: args.len(),
                }
            });
        }
        let owner_class = match method_owner(db, loc) {
            Some(MethodOwner::Class(class)) => Some(class),
            Some(MethodOwner::Impl(block)) => match impl_block_data(db, block).subject {
                ImplSubjectData::InClass { class, .. } => Some(class),
                ImplSubjectData::Free { .. } => None,
            },
            Some(MethodOwner::Interface(_)) | None => None,
        };
        let owner = owner_class
            .map(|class| {
                (
                    class_qualified_name(db, class),
                    class_generic_frame(db, class),
                )
            })
            .filter(|(_, params)| !params.is_empty());
        Ok(Self {
            db,
            layout,
            args,
            owner,
        })
    }

    /// The frame of a lambda created inside this one with the type
    /// arguments `args`, which are this frame's slots forwarded (lowering
    /// captures every enclosing slot, in order).
    pub(crate) fn for_lambda(&self, args: &[RealizedTy]) -> Result<Self, Rejection> {
        if args != self.args {
            return Err(Rejection::unsupported(
                "lambda with type arguments other than its creator's frame",
            ));
        }
        Ok(self.clone())
    }

    pub(crate) fn args(&self) -> &[RealizedTy] {
        &self.args
    }

    /// `ty` with every type variable of this frame replaced by its argument
    /// and every associated type projection over a now-concrete base
    /// reduced, as the VM realizes a template against `frame.type_args`.
    /// A type variable this frame does not bind, or a projection that does
    /// not reduce, is reported.
    pub(crate) fn realize(&self, ty: &RuntimeTy) -> Result<RuntimeTy, Unsupported> {
        if self.args.is_empty() && self.owner.is_none() && !mentions_projection(ty) {
            return Ok(ty.clone());
        }
        let plain = Ty::from(ty);
        // `self` of a generic class's method: the class at its own
        // parameters, which the bindings then realize.
        let plain = match &self.owner {
            Some((head, params)) => rewrite_ty(&plain, &mut |node| match node {
                Ty::Class(name, args) if name == head && args.is_empty() => Some(Ty::Class(
                    head.clone(),
                    params.iter().cloned().map(Ty::TypeVar).collect(),
                )),
                _ => None,
            }),
            None => plain,
        };
        let bindings = self
            .layout
            .params()
            .iter()
            .zip(&self.args)
            .map(|(param, arg)| (param.clone(), Ty::from(arg.as_runtime_ty())))
            .collect();
        let substituted = substitute_ty(&plain, &bindings);
        let reduced = reduce_ground_projections(self.db, &substituted, PROJECTION_FUEL);
        let realized = RealizedTy::try_from(&reduced).map_err(|error| {
            Unsupported(match error.variant {
                "TypeVar" => format!(
                    "type variable (`{}` is not bound by the instance's frame)",
                    self.spell(ty)
                ),
                _ => format!(
                    "associated type projection (`{}` does not reduce)",
                    self.spell(ty)
                ),
            })
        })?;
        Ok(RuntimeTy::from(realized))
    }

    /// `template` realized against this frame: `#n` is the `n`th argument,
    /// a projection reduces through the impl registry.
    pub(crate) fn realize_template(
        &self,
        template: &TyTemplate,
    ) -> Result<RealizedTy, Unsupported> {
        template
            .substitute(&self.args, &Facts::new(self.db))
            .map_err(|error| {
                let spelling = baml_compiler2_hir::package::spelling(self.db);
                let spelled = template.map_heads(&mut |decl| spelling.wire(decl));
                Unsupported(format!(
                    "type argument `{spelled}` could not be realized: {error}"
                ))
            })
    }

    fn spell(&self, ty: &RuntimeTy) -> String {
        let spelling = baml_compiler2_hir::package::spelling(self.db);
        ty.map_heads(&mut |decl| spelling.wire(decl)).to_string()
    }
}

/// Whether `ty` carries an associated type projection anywhere, which
/// needs the frame's reduction even when no type variable is bound.
fn mentions_projection(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::AssociatedTypeProjection { .. } => true,
        RuntimeTy::List(inner) => mentions_projection(inner),
        RuntimeTy::Map { key, value } => mentions_projection(key) || mentions_projection(value),
        RuntimeTy::Union(members) => members.iter().any(mentions_projection),
        RuntimeTy::Class(_, args) => args.iter().any(mentions_projection),
        RuntimeTy::Interface(_, args, assoc) => {
            args.iter().any(mentions_projection)
                || assoc.iter().any(|(_, ty)| mentions_projection(ty))
        }
        RuntimeTy::Function {
            params,
            ret,
            throws,
        } => {
            params.iter().any(|param| mentions_projection(&param.ty))
                || mentions_projection(ret)
                || mentions_projection(throws)
        }
        RuntimeTy::Future(value, error) => mentions_projection(value) || mentions_projection(error),
        _ => false,
    }
}

/// Whether `ty` is, or mentions, `unknown`: a type argument the subset
/// has no value for (D3).
pub(crate) fn mentions_unknown(ty: &RealizedTy) -> bool {
    match ty {
        RealizedTy::Unknown => true,
        RealizedTy::List(inner) => mentions_unknown(inner),
        RealizedTy::Map { key, value } => mentions_unknown(key) || mentions_unknown(value),
        RealizedTy::Union(members) => members.iter().any(mentions_unknown),
        RealizedTy::Class(_, args) => args.iter().any(mentions_unknown),
        RealizedTy::Interface(_, args, assoc) => {
            args.iter().any(mentions_unknown) || assoc.iter().any(|(_, ty)| mentions_unknown(ty))
        }
        RealizedTy::Function {
            params,
            ret,
            throws,
        } => {
            params.iter().any(|param| mentions_unknown(&param.ty))
                || mentions_unknown(ret)
                || mentions_unknown(throws)
        }
        RealizedTy::Future(value, error) => mentions_unknown(value) || mentions_unknown(error),
        _ => false,
    }
}
