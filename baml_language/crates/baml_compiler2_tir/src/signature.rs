//! Lowering a callable's declared signature to a function type constructor.
//!
//! Every callable in BAML — a free function, a class method, an interface method — has the
//! same shape: a set of generic parameters (ordered, outermost enclosing scope first) and an
//! optional `Self`, which together inform four type-bearing groups:
//!
//! - **args** — positional, required
//! - **kwargs** — named, optional
//! - **return** — required
//! - **throws** — required
//!
//! The generics and `Self` are carried by a [`TypeExprContext`]; this module lowers the four
//! groups through it. A `Ty::Function` that mentions free `Ty::TypeVar`s can be a genuine type
//! — but only when those variables are *rigid*: fixed abstract types skolemized by an enclosing
//! scope (a `T` inside `Foo<T>`'s body denotes one specific, if unknown, type). That is not
//! what this produces. Here the free variables are the callable's *own* generic parameters plus
//! any non-rigid ones inherited from the enclosing class/interface — the parameters
//! specialization binds. So the result is a **template**: a type *constructor* denoting the
//! family of concrete function types obtained by binding them, not itself an inhabitable type.
//! It becomes one only once *specialized* — a separate
//! [`substitute_ty`](crate::generics::substitute_ty) pass that binds those variables to
//! concrete arguments. Callers differ only in the context they build and whether they
//! specialize — never in this lowering.

use baml_base::Name;
use baml_compiler2_hir::type_ref::{TypeRefBuilder, TypeRefId, TypeRefKind, TypeRefStore};

use crate::{
    infer_context::TirTypeError,
    lower_type_expr::{TypeExprContext, lower_type_ref},
    ty::{FunctionParamMode, FunctionParamTy, Ty, TyAttr},
};

/// One type slot in a [`DeclaredSignature`] — a written type, or one of the two
/// slots the declaration leaves implicit.
#[derive(Debug, Clone, Copy)]
pub enum SigTypeRef {
    /// A written type: an id into the signature's arena.
    Id(TypeRefId),
    /// The `self` receiver, desugared to `self: Self` — lowers as the `Self` path.
    SelfReceiver,
    /// No written type — lowers to `unknown`.
    Missing,
}

/// A callable's declared, type-bearing signature: positional (required) args, keyword
/// (optional) args, and the return + throws types. The whole of what the enclosing generic
/// parameters and `Self` inform.
pub struct DeclaredSignature<'a> {
    /// The arena every [`SigTypeRef::Id`] below indexes.
    pub type_refs: &'a TypeRefStore,
    /// Positional parameters, in declaration order (each required).
    pub positional: Vec<(Option<Name>, SigTypeRef)>,
    /// Keyword parameters (each optional — has a default).
    pub keyword: Vec<(Option<Name>, SigTypeRef)>,
    pub return_type: SigTypeRef,
    pub throws: SigTypeRef,
}

/// A function type *constructor* — the same data as a `Ty::Function`, but held in its own type
/// because it is a **template** over its free type variables (the callable's own generic params
/// plus any non-rigid ones inherited from an enclosing class/interface), not an inhabitable
/// type. [`lower_signature`] produces it; [`into_ty`](Self::into_ty) reinterprets it as a
/// `Ty::Function` at the boundary where it is specialized (fed to
/// [`substitute_ty`](crate::generics::substitute_ty)) or deliberately stored unspecialized. The
/// distinct type keeps a raw template from being mistaken for a real type anywhere in between.
pub struct FunctionTypeConstructor {
    pub params: Vec<FunctionParamTy>,
    pub ret: Box<Ty>,
    pub throws: Box<Ty>,
    pub attr: TyAttr,
}

impl FunctionTypeConstructor {
    /// Reinterpret the constructor as a `Ty::Function`. The result still carries the free type
    /// variables, so call this only at the point of specialization (binding them via
    /// [`substitute_ty`](crate::generics::substitute_ty)) or of deliberately storing the
    /// template as an unspecialized signature.
    pub fn into_ty(self) -> Ty {
        Ty::Function {
            params: self.params,
            ret: self.ret,
            throws: self.throws,
            attr: self.attr,
        }
    }
}

/// Lower a [`DeclaredSignature`] to a [`FunctionTypeConstructor`], resolving every part through
/// `ctx` — which supplies the in-scope generic params (kept as free `Ty::TypeVar`s), `Self`,
/// and typevar bounds for `T.member` projections. Its free variables are the callable's own (or
/// non-rigid inherited) parameters, so the result is a template, not an inhabitable type (see
/// the module docs); binding them to concrete arguments yields a real function type.
pub fn lower_signature(
    sig: &DeclaredSignature<'_>,
    ctx: &dyn TypeExprContext<'_>,
    diagnostics: &mut Vec<TirTypeError>,
) -> FunctionTypeConstructor {
    // The two implicit slots, synthesized into a scratch arena so they lower
    // through the same path as written types (`Self` resolves via the context's
    // `self_ty`, `unknown` via the ordinary `Unknown` arm).
    let mut scratch = TypeRefBuilder::new();
    let self_id = scratch.alloc_synthetic(TypeRefKind::Path {
        segments: vec![Name::new("Self")],
        generic_args: Box::new([]),
        associated_type_bindings: Box::new([]),
    });
    let unknown_id = scratch.alloc_synthetic(TypeRefKind::Unknown);
    let (scratch_store, _) = scratch.finish();
    let lower = |slot: SigTypeRef, diagnostics: &mut Vec<TirTypeError>| match slot {
        SigTypeRef::Id(id) => lower_type_ref(sig.type_refs, id, ctx, diagnostics),
        SigTypeRef::SelfReceiver => lower_type_ref(&scratch_store, self_id, ctx, diagnostics),
        SigTypeRef::Missing => lower_type_ref(&scratch_store, unknown_id, ctx, diagnostics),
    };

    let params = sig
        .positional
        .iter()
        .map(|(name, ty)| (name, ty, FunctionParamMode::Required))
        .chain(
            sig.keyword
                .iter()
                .map(|(name, ty)| (name, ty, FunctionParamMode::Optional)),
        )
        .map(|(name, &ty, mode)| FunctionParamTy {
            name: name.clone(),
            ty: lower(ty, diagnostics),
            mode,
        })
        .collect();
    FunctionTypeConstructor {
        params,
        ret: Box::new(lower(sig.return_type, diagnostics)),
        throws: Box::new(lower(sig.throws, diagnostics)),
        attr: TyAttr::default(),
    }
}
