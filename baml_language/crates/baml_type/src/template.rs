//! Semantic impls for [`TyTemplate`], the `Ty`-shaped type family member that
//! may contain unresolved generic references.
//!
//! The enum itself, its satellites ([`TyTemplateInterface`],
//! [`TyTemplateFunctionParamTy`]), and the `RealizedTy ⇔ TyTemplate` conversions
//! are generated in [`crate::family`] by the `ty_family!` macro. `TyTemplate`
//! swaps the name-based `TypeVar` for the positional [`TyTemplate::TypeArgRef`]
//! (and the deprecated dispatch-guard [`TyTemplate::TypeArgRefOrWildcard`]) plus
//! the [`TyTemplate::Wildcard`] hole, and keeps [`TyTemplate::AssociatedTypeProjection`]
//! for symbolic projections — so a template can express an unresolved generic
//! *structurally*, never as an opaque name.
//!
//! A template that contains none of those template-only leaves is "fully
//! realized" and narrows to a [`RealizedTy`] (the generated `TryFrom`); that
//! narrowing is the [`TyTemplate::is_fully_concrete`] check.
//!
//! # De Bruijn indexing
//!
//! `TypeArgRef(n)` refers to the n-th entry in the enclosing call-frame's
//! `type_args` vector (0-based). The compiler assigns indices using
//! `function_generic_params` ordering so the mapping is stable across
//! compilation. `TypeArgRefOrWildcard(n)` has the same index but is only valid
//! in dispatch guards: an unconcretized runtime slot matches any actual type
//! argument instead of materializing `unknown` as a constraint.

// `substitute`/`Display` walk every variant, including the deprecated
// dispatch-guard `TypeArgRefOrWildcard`. Handling it is not misuse.
#![expect(deprecated, reason = "TypeArgRefOrWildcard is a live template variant")]

use std::fmt;

use crate::{
    Name, RealizedTy, RuntimeFunctionParamTy, RuntimeInterface, RuntimeTy, TyAttr, TyTemplate,
    TyTemplateInterface, TypeName,
};

impl TyTemplate {
    // --- Ergonomic constructors (default TyAttr) ---

    /// `T[]` (list) with default attributes.
    pub fn list(inner: TyTemplate) -> Self {
        TyTemplate::List(Box::new(inner), TyAttr::default())
    }

    /// `map<K, V>` with default attributes.
    pub fn map(key: TyTemplate, value: TyTemplate) -> Self {
        TyTemplate::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }

    /// `A | B | ...` (union) with default attributes.
    pub fn union(members: impl IntoIterator<Item = TyTemplate>) -> Self {
        TyTemplate::Union(members.into_iter().collect(), TyAttr::default())
    }

    /// `Class<A1, A2, ...>` (generic class instantiation) with default attributes.
    pub fn class(name: TypeName, args: Vec<TyTemplate>) -> Self {
        TyTemplate::Class(name, args, TyAttr::default())
    }

    /// `Interface<A1, Assoc = A2, ...>` with default attributes.
    pub fn interface(
        name: TypeName,
        args: Vec<TyTemplate>,
        associated_bindings: Vec<(Name, TyTemplate)>,
    ) -> Self {
        TyTemplate::Interface(name, args, associated_bindings, TyAttr::default())
    }

    /// Walk the template once, substituting each `TypeArgRef(n)` with
    /// `type_args[n]`.
    ///
    /// If `n` is out of range (e.g. when a generic function is called from a
    /// non-generic context that didn't supply type arguments), the substitution
    /// falls back to `RuntimeTy::unknown()` rather than panicking. This handles
    /// stdlib paths where `T`/`S` are declared in the function signature for
    /// type-checking purposes but the corresponding Rust sys-op implementation
    /// doesn't use the type-arg slot.
    pub fn substitute(&self, type_args: &[RuntimeTy]) -> RuntimeTy {
        match self {
            // ── Template-only leaves ──────────────────────────────────────────
            Self::TypeArgRef(n) | Self::TypeArgRefOrWildcard(n) => type_args
                .get(*n as usize)
                .cloned()
                .unwrap_or_else(RuntimeTy::unknown),
            // A wildcard never reaches `LoadType` materialization; if it ever
            // does, fall back to `unknown` rather than panicking.
            Self::Wildcard => RuntimeTy::unknown(),

            // ── Composites: recurse, resolving nested template refs ───────────
            Self::List(inner, attr) => {
                RuntimeTy::List(Box::new(inner.substitute(type_args)), attr.clone())
            }
            Self::Map { key, value, attr } => RuntimeTy::Map {
                key: Box::new(key.substitute(type_args)),
                value: Box::new(value.substitute(type_args)),
                attr: attr.clone(),
            },
            Self::Union(parts, attr) => RuntimeTy::Union(
                parts.iter().map(|p| p.substitute(type_args)).collect(),
                attr.clone(),
            ),
            Self::Class(name, args, attr) => RuntimeTy::Class(
                name.clone(),
                args.iter().map(|a| a.substitute(type_args)).collect(),
                attr.clone(),
            ),
            Self::Interface(name, args, associated_bindings, attr) => RuntimeTy::Interface(
                name.clone(),
                args.iter().map(|a| a.substitute(type_args)).collect(),
                associated_bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.substitute(type_args)))
                    .collect(),
                attr.clone(),
            ),
            Self::Function {
                params,
                ret,
                throws,
                attr,
            } => RuntimeTy::Function {
                params: params
                    .iter()
                    .map(|p| RuntimeFunctionParamTy {
                        name: p.name.clone(),
                        ty: p.ty.substitute(type_args),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.substitute(type_args)),
                throws: Box::new(throws.substitute(type_args)),
                attr: attr.clone(),
            },
            Self::Future(value, error, attr) => RuntimeTy::Future(
                Box::new(value.substitute(type_args)),
                Box::new(error.substitute(type_args)),
                attr.clone(),
            ),
            Self::WatchAccessor(inner, attr) => {
                RuntimeTy::WatchAccessor(Box::new(inner.substitute(type_args)), attr.clone())
            }
            // The projection stays symbolic — only its base/interface positions
            // realize. Resolving it to the witness type needs impl knowledge the
            // substitution environment doesn't carry.
            Self::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => RuntimeTy::AssociatedTypeProjection {
                base: Box::new(base.substitute(type_args)),
                interface: interface
                    .as_ref()
                    .map(|iface| Box::new(iface.substitute(type_args))),
                member: member.clone(),
                attr: attr.clone(),
            },

            // ── Realized leaf ─────────────────────────────────────────────────
            // No template refs and no nested type positions: it narrows to a
            // `RealizedTy` (proving realizedness) which widens to `RuntimeTy` by
            // transmute. A composite variant is handled above, so a narrowing
            // failure here would be a missing arm — surfaced loudly.
            other => RealizedTy::try_from(other.clone())
                .unwrap_or_else(|e| unreachable!("realized-leaf template narrowing failed: {e}"))
                .into(),
        }
    }

    /// Returns `true` when the template contains no template-only leaf
    /// (`TypeArgRef`, `TypeArgRefOrWildcard`, `Wildcard`) at any depth — i.e. it
    /// is a fully realized type that narrows to a [`RealizedTy`].
    pub fn is_fully_concrete(&self) -> bool {
        <&RealizedTy>::try_from(self).is_ok()
    }

    /// Whether a `Wildcard` hole appears anywhere in the template.
    ///
    /// Distinct from (not) [`Self::is_fully_concrete`]: a template can be
    /// non-concrete without holes (frame references materialize to exactly one
    /// type under `substitute`), while a hole never materializes — it matches
    /// any type at its own position, so a holey template only constrains the
    /// positions around its holes.
    pub fn contains_wildcard(&self) -> bool {
        match self {
            Self::Wildcard => true,
            Self::TypeArgRef(_) | Self::TypeArgRefOrWildcard(_) => false,
            Self::List(inner, _) | Self::WatchAccessor(inner, _) => inner.contains_wildcard(),
            Self::Map { key, value, .. } => key.contains_wildcard() || value.contains_wildcard(),
            Self::Future(value, error, _) => value.contains_wildcard() || error.contains_wildcard(),
            Self::Union(parts, _) => parts.iter().any(Self::contains_wildcard),
            Self::Class(_, args, _) => args.iter().any(Self::contains_wildcard),
            Self::Interface(_, args, associated_bindings, _) => {
                args.iter().any(Self::contains_wildcard)
                    || associated_bindings
                        .iter()
                        .any(|(_, ty)| ty.contains_wildcard())
            }
            Self::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params.iter().any(|p| p.ty.contains_wildcard())
                    || ret.contains_wildcard()
                    || throws.contains_wildcard()
            }
            Self::AssociatedTypeProjection {
                base, interface, ..
            } => {
                base.contains_wildcard()
                    || interface.as_ref().is_some_and(|iface| {
                        iface.generics.iter().any(Self::contains_wildcard)
                            || iface
                                .associated_types
                                .iter()
                                .any(|(_, ty)| ty.contains_wildcard())
                    })
            }
            // Realized leaves carry no nested type positions.
            Self::Int { .. }
            | Self::Bigint { .. }
            | Self::Float { .. }
            | Self::String { .. }
            | Self::Bool { .. }
            | Self::Null { .. }
            | Self::Uint8Array { .. }
            | Self::Media(..)
            | Self::Literal(..)
            | Self::Enum(..)
            | Self::EnumVariant(..)
            | Self::RustType { .. }
            | Self::Type { .. }
            | Self::Resource { .. }
            | Self::PromptAst { .. }
            | Self::Void { .. }
            | Self::TypeAlias(..)
            | Self::BuiltinUnknown { .. }
            | Self::Never { .. } => false,
        }
    }
}

impl TyTemplateInterface {
    /// Substitute frame type args through the interface's generic and
    /// associated-binding positions (see [`TyTemplate::substitute`]).
    fn substitute(&self, type_args: &[RuntimeTy]) -> RuntimeInterface {
        RuntimeInterface::new(
            self.name.clone(),
            self.generics
                .iter()
                .map(|g| g.substitute(type_args))
                .collect(),
            self.associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), ty.substitute(type_args)))
                .collect(),
        )
    }
}

impl fmt::Display for TyTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Render through `Ty`'s `Display` so every shared construct (list-postfix
        // parenthesization, `future<…>` casing, function `throws` handling, …)
        // stays byte-identical to the canonical type renderer — no per-variant
        // drift. Only the template-only leaves need a placeholder: a frame ref
        // shows as `#n` (a `TypeVar` named `#n`) and a hole as `_` (`Infer`).
        fmt::Display::fmt(&self.to_display_ty(), f)
    }
}

impl TyTemplate {
    /// A lossy [`Ty`] view for rendering only: frame refs become
    /// `TypeVar("#n")`, the dispatch-guard ref `TypeVar("#n?")`, and a wildcard
    /// hole `Infer` (`_`). Every other node maps structurally, so
    /// [`fmt::Display`] can delegate to `Ty`'s renderer.
    fn to_display_ty(&self) -> crate::Ty {
        use crate::Ty;
        match self {
            Self::TypeArgRef(n) => Ty::TypeVar(Name::new(format!("#{n}")), TyAttr::default()),
            Self::TypeArgRefOrWildcard(n) => {
                Ty::TypeVar(Name::new(format!("#{n}?")), TyAttr::default())
            }
            Self::Wildcard => Ty::Infer {
                attr: TyAttr::default(),
            },
            Self::List(inner, attr) => Ty::List(Box::new(inner.to_display_ty()), attr.clone()),
            Self::Map { key, value, attr } => Ty::Map {
                key: Box::new(key.to_display_ty()),
                value: Box::new(value.to_display_ty()),
                attr: attr.clone(),
            },
            Self::Union(parts, attr) => Ty::Union(
                parts.iter().map(Self::to_display_ty).collect(),
                attr.clone(),
            ),
            Self::Class(name, args, attr) => Ty::Class(
                name.clone(),
                args.iter().map(Self::to_display_ty).collect(),
                attr.clone(),
            ),
            Self::Interface(name, args, associated_bindings, attr) => Ty::Interface(
                name.clone(),
                args.iter().map(Self::to_display_ty).collect(),
                associated_bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), t.to_display_ty()))
                    .collect(),
                attr.clone(),
            ),
            Self::Function {
                params,
                ret,
                throws,
                attr,
            } => Ty::Function {
                params: params
                    .iter()
                    .map(|p| crate::FunctionParamTy {
                        name: p.name.clone(),
                        ty: p.ty.to_display_ty(),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(ret.to_display_ty()),
                throws: Box::new(throws.to_display_ty()),
                attr: attr.clone(),
            },
            Self::Future(value, error, attr) => Ty::Future(
                Box::new(value.to_display_ty()),
                Box::new(error.to_display_ty()),
                attr.clone(),
            ),
            Self::WatchAccessor(inner, attr) => {
                Ty::WatchAccessor(Box::new(inner.to_display_ty()), attr.clone())
            }
            Self::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => Ty::AssociatedTypeProjection {
                base: Box::new(base.to_display_ty()),
                interface: interface.as_ref().map(|iface| {
                    Box::new(crate::Interface {
                        name: iface.name.clone(),
                        generics: iface.generics.iter().map(Self::to_display_ty).collect(),
                        associated_types: iface
                            .associated_types
                            .iter()
                            .map(|(n, t)| (n.clone(), t.to_display_ty()))
                            .collect(),
                    })
                }),
                member: member.clone(),
                attr: attr.clone(),
            },
            // A realized leaf widens into `Ty` by the generated conversion.
            other => Ty::from(RealizedTy::try_from(other.clone()).unwrap_or_else(|e| {
                unreachable!("realized-leaf template widening for display failed: {e}")
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_template_substitutes_to_itself() {
        let tmpl = TyTemplate::from(RealizedTy::int());
        assert_eq!(tmpl.substitute(&[]), RuntimeTy::int());
        assert!(tmpl.is_fully_concrete());
    }

    #[test]
    fn type_arg_ref_substitutes_correctly() {
        let tmpl = TyTemplate::TypeArgRef(0);
        let ty = tmpl.substitute(&[RuntimeTy::string()]);
        assert_eq!(ty, RuntimeTy::string());
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn array_of_type_arg_ref() {
        // list<#0> with type_args=[int] → int[]
        let tmpl = TyTemplate::list(TyTemplate::TypeArgRef(0));
        let ty = tmpl.substitute(&[RuntimeTy::int()]);
        assert_eq!(ty, RuntimeTy::list(RuntimeTy::int()));
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn optional_of_type_arg_ref() {
        // (#0 | null) with type_args=[string] → string?
        let tmpl = TyTemplate::union([
            TyTemplate::TypeArgRef(0),
            TyTemplate::from(RealizedTy::null()),
        ]);
        let ty = tmpl.substitute(&[RuntimeTy::string()]);
        assert_eq!(ty, RuntimeTy::optional(RuntimeTy::string()));
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn concrete_array_is_fully_concrete() {
        let tmpl = TyTemplate::list(TyTemplate::from(RealizedTy::int()));
        assert!(tmpl.is_fully_concrete());
    }

    #[test]
    fn union_of_concrete_is_fully_concrete() {
        let tmpl = TyTemplate::union([
            TyTemplate::from(RealizedTy::int()),
            TyTemplate::from(RealizedTy::string()),
        ]);
        assert!(tmpl.is_fully_concrete());
        let ty = tmpl.substitute(&[]);
        assert_eq!(
            ty,
            RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()])
        );
    }

    #[test]
    fn union_containing_type_arg_ref_not_concrete() {
        let tmpl = TyTemplate::union([
            TyTemplate::from(RealizedTy::int()),
            TyTemplate::TypeArgRef(0),
        ]);
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn class_with_type_arg_ref_substitution() {
        let tmpl = TyTemplate::class(
            TypeName::local(crate::Name::new("Container")),
            vec![TyTemplate::TypeArgRef(0)],
        );
        let user = RuntimeTy::user_class("User");
        let result = tmpl.substitute(std::slice::from_ref(&user));
        assert_eq!(
            result,
            RuntimeTy::class_with_args(TypeName::local(crate::Name::new("Container")), vec![user])
        );
        assert!(!tmpl.is_fully_concrete());
    }

    #[test]
    fn class_no_args_is_fully_concrete() {
        let tmpl = TyTemplate::class(TypeName::local(crate::Name::new("User")), vec![]);
        assert!(tmpl.is_fully_concrete());
        let result = tmpl.substitute(&[]);
        assert_eq!(
            result,
            RuntimeTy::Class(
                TypeName::local(crate::Name::new("User")),
                vec![],
                crate::TyAttr::default()
            )
        );
    }
}
