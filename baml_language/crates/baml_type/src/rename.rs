//! Structural rewriting of the qualified type names a type mentions.
//!
//! A runtime-compiled package's declarations are named by the ordinary
//! compiler, which knows nothing about mints: two packages that each declare
//! `Item` both emit `user.Item`, indistinguishable from each other and from a
//! static `Item`. `map_type_names` is how the VM re-spells a whole compiled
//! image under mint-unique names once it has minted them, so that a name is
//! again a *key* — see [`crate::QualifiedTypeName::runtime_local`].
//!
//! Each match below is deliberately exhaustive with no wildcard arm: a new
//! variant that carries a name or a nested type must be handled here or the
//! crate stops compiling. A missed arm would silently leave one spelling of a
//! declaration behind, which is exactly the class of wrong-identity bug the
//! re-spelling exists to prevent.

use crate::{
    FunctionParamTy, Interface, RealizedInterface, RealizedTy, RuntimeInterface, RuntimeTy, Ty,
    TyTemplate, TyTemplateFunctionParamTy, TyTemplateInterface, TypeName,
};

/// Maps a qualified name to its replacement, or `None` to leave it alone.
pub type RenameTypeName<'a> = &'a dyn Fn(&TypeName) -> Option<TypeName>;

fn renamed(name: &TypeName, rename: RenameTypeName<'_>) -> TypeName {
    rename(name).unwrap_or_else(|| name.clone())
}

/// Emit `map_type_names` for one family member. The arms every member shares
/// live here once; `extra` carries the axis-specific ones (`tir` for [`Ty`],
/// `frame` for [`TyTemplate`]) so each expansion is still checked exhaustively.
macro_rules! impl_map_type_names {
    (
        $member:ident,
        param = $param:ident,
        interface = $interface:ident,
        this = $this:ident,
        boxed = $boxed:ident,
        extra = { $($extra:tt)* }
    ) => {
        impl $member {
            /// Rebuild this type with every qualified name it mentions — at any
            /// depth — passed through `rename`.
            #[must_use]
            pub fn map_type_names(&self, rename: RenameTypeName<'_>) -> $member {
                let $this = self;
                let map = |ty: &$member| ty.map_type_names(rename);
                let $boxed = |ty: &$member| Box::new(ty.map_type_names(rename));
                match $this {
                    // Leaves with neither a name nor a nested type.
                    $member::Int { .. }
                    | $member::Bigint { .. }
                    | $member::Float { .. }
                    | $member::String { .. }
                    | $member::Bool { .. }
                    | $member::Null { .. }
                    | $member::Uint8Array { .. }
                    | $member::Media(..)
                    | $member::Literal(..)
                    | $member::RustType { .. }
                    | $member::Type { .. }
                    | $member::Resource { .. }
                    | $member::PromptAst { .. }
                    | $member::Void { .. }
                    | $member::BuiltinUnknown { .. }
                    | $member::Never { .. } => $this.clone(),

                    $member::Class(name, args, attr) => $member::Class(
                        renamed(name, rename),
                        args.iter().map(map).collect(),
                        attr.clone(),
                    ),
                    $member::Interface(name, args, assoc, attr) => $member::Interface(
                        renamed(name, rename),
                        args.iter().map(map).collect(),
                        assoc
                            .iter()
                            .map(|(binding, ty)| (binding.clone(), map(ty)))
                            .collect(),
                        attr.clone(),
                    ),
                    $member::Enum(name, attr) => $member::Enum(renamed(name, rename), attr.clone()),
                    $member::EnumVariant(name, variant, attr) => {
                        $member::EnumVariant(renamed(name, rename), variant.clone(), attr.clone())
                    }
                    $member::TypeAlias(name, attr) => {
                        $member::TypeAlias(renamed(name, rename), attr.clone())
                    }

                    $member::List(inner, attr) => $member::List($boxed(inner), attr.clone()),
                    $member::Map { key, value, attr } => $member::Map {
                        key: $boxed(key),
                        value: $boxed(value),
                        attr: attr.clone(),
                    },
                    $member::Union(members, attr) => {
                        $member::Union(members.iter().map(map).collect(), attr.clone())
                    }
                    $member::Future(value, throws, attr) => {
                        $member::Future($boxed(value), $boxed(throws), attr.clone())
                    }
                    $member::Function {
                        params,
                        ret,
                        throws,
                        attr,
                    } => $member::Function {
                        params: params
                            .iter()
                            .map(|param| $param {
                                name: param.name.clone(),
                                ty: map(&param.ty),
                                mode: param.mode,
                            })
                            .collect(),
                        ret: $boxed(ret),
                        throws: $boxed(throws),
                        attr: attr.clone(),
                    },
                    $member::AssociatedTypeProjection {
                        base,
                        interface,
                        member,
                        attr,
                    } => $member::AssociatedTypeProjection {
                        base: $boxed(base),
                        interface: Box::new($interface::new(
                            renamed(&interface.name, rename),
                            interface.generics.iter().map(map).collect(),
                            interface
                                .associated_types
                                .iter()
                                .map(|(binding, ty)| (binding.clone(), map(ty)))
                                .collect(),
                        )),
                        member: member.clone(),
                        attr: attr.clone(),
                    },

                    $($extra)*
                }
            }
        }

        impl $interface {
            /// [`map_type_names`](Ty::map_type_names) for an interface
            /// constraint: its own name, its generic arguments, and every
            /// associated-type binding.
            #[must_use]
            pub fn map_type_names(&self, rename: RenameTypeName<'_>) -> $interface {
                $interface::new(
                    renamed(&self.name, rename),
                    self.generics
                        .iter()
                        .map(|ty| ty.map_type_names(rename))
                        .collect(),
                    self.associated_types
                        .iter()
                        .map(|(binding, ty)| (binding.clone(), ty.map_type_names(rename)))
                        .collect(),
                )
            }
        }
    };
}

impl_map_type_names!(
    Ty,
    param = FunctionParamTy,
    interface = Interface,
    this = this,
    boxed = boxed,
    extra = {
        // `typevar` and `tir` axes: leaves plus the two evolving containers.
        Ty::TypeVar(..) | Ty::Unknown { .. } | Ty::Error { .. } | Ty::Infer { .. } => this.clone(),
        Ty::EvolvingList(inner, attr) => Ty::EvolvingList(boxed(inner), attr.clone()),
        Ty::EvolvingMap(key, value, attr) => {
            Ty::EvolvingMap(boxed(key), boxed(value), attr.clone())
        }
    }
);

impl_map_type_names!(
    TyTemplate,
    param = TyTemplateFunctionParamTy,
    interface = TyTemplateInterface,
    this = this,
    boxed = boxed,
    extra = {
        // `frame` axis: a positional reference carries no name.
        TyTemplate::TypeArgRef(..) => this.clone(),
    }
);

/// `RuntimeTy` and `RealizedTy` are both subsets of [`Ty`], so they borrow its
/// implementation: widen, rewrite, narrow. Renaming preserves every variant, so
/// the narrowing back is infallible.
macro_rules! impl_map_type_names_via_ty {
    ($member:ident) => {
        impl $member {
            /// [`Ty::map_type_names`], applied through the zero-cost upcast.
            #[must_use]
            pub fn map_type_names(&self, rename: RenameTypeName<'_>) -> $member {
                let renamed = self.as_ty().map_type_names(rename);
                $member::try_from(&renamed).unwrap_or_else(|_| {
                    unreachable!(
                        "renaming a qualified name preserves every variant, so the narrowing \
                         back into the same family member cannot fail"
                    )
                })
            }
        }
    };
}

impl_map_type_names_via_ty!(RuntimeTy);
impl_map_type_names_via_ty!(RealizedTy);

/// The interface-constraint companions of the borrowed members, rebuilt from
/// their already-renamed parts.
macro_rules! impl_interface_map_type_names {
    ($interface:ident, $member:ident) => {
        impl $interface {
            /// [`Interface::map_type_names`] for this family member.
            #[must_use]
            pub fn map_type_names(&self, rename: RenameTypeName<'_>) -> $interface {
                $interface::new(
                    renamed(&self.name, rename),
                    self.generics
                        .iter()
                        .map(|ty: &$member| ty.map_type_names(rename))
                        .collect(),
                    self.associated_types
                        .iter()
                        .map(|(binding, ty)| (binding.clone(), ty.map_type_names(rename)))
                        .collect(),
                )
            }
        }
    };
}

impl_interface_map_type_names!(RuntimeInterface, RuntimeTy);
impl_interface_map_type_names!(RealizedInterface, RealizedTy);

#[cfg(test)]
mod tests {
    use baml_base::Name;

    use crate::{Interface, RealizedTy, Ty, TyAttr, TyTemplate, TypeName};

    fn qtn(name: &str) -> TypeName {
        TypeName::local(Name::new(name))
    }

    fn minted(name: &TypeName) -> Option<TypeName> {
        (name.name().as_str() == "Item").then(|| TypeName::runtime_local(name.name().clone(), 7))
    }

    #[test]
    fn renames_a_name_nested_under_containers_and_functions() {
        let ty = Ty::Function {
            params: vec![crate::FunctionParamTy::required(
                Some(Name::new("x")),
                Ty::List(
                    Box::new(Ty::Class(qtn("Item"), Vec::new(), TyAttr::default())),
                    TyAttr::default(),
                ),
            )],
            ret: Box::new(Ty::Map {
                key: Box::new(Ty::String {
                    attr: TyAttr::default(),
                }),
                value: Box::new(Ty::Enum(qtn("Item"), TyAttr::default())),
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Class(qtn("Other"), Vec::new(), TyAttr::default())),
            attr: TyAttr::default(),
        };

        let out = ty.map_type_names(&minted);
        assert!(
            matches!(&out, Ty::Function { params, .. }
                if matches!(&params[0].ty, Ty::List(inner, _)
                    if matches!(inner.as_ref(), Ty::Class(name, ..) if name.is_runtime_minted()))),
            "param list element was not renamed: {out}"
        );
        assert!(
            matches!(&out, Ty::Function { ret, .. }
                if matches!(ret.as_ref(), Ty::Map { value, .. }
                    if matches!(value.as_ref(), Ty::Enum(name, _) if name.is_runtime_minted()))),
            "map value was not renamed: {out}"
        );
        assert!(
            matches!(&out, Ty::Function { throws, .. }
                if matches!(throws.as_ref(), Ty::Class(name, ..) if !name.is_runtime_minted())),
            "an unrelated name must be left alone: {out}"
        );
    }

    #[test]
    fn renames_through_an_interface_constraint_and_projection() {
        let interface = Interface::new(
            qtn("Holder"),
            vec![Ty::Class(qtn("Item"), Vec::new(), TyAttr::default())],
            vec![(
                Name::new("Out"),
                Ty::Class(qtn("Item"), Vec::new(), TyAttr::default()),
            )],
        );
        let ty = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::Class(qtn("Item"), Vec::new(), TyAttr::default())),
            interface: Box::new(interface),
            member: Name::new("Out"),
            attr: TyAttr::default(),
        };

        let Ty::AssociatedTypeProjection {
            base, interface, ..
        } = ty.map_type_names(&minted)
        else {
            panic!("projection must stay a projection");
        };
        assert!(matches!(base.as_ref(), Ty::Class(name, ..) if name.is_runtime_minted()));
        assert!(matches!(&interface.generics[0], Ty::Class(name, ..) if name.is_runtime_minted()));
        assert!(
            matches!(&interface.associated_types[0].1, Ty::Class(name, ..) if name.is_runtime_minted())
        );
    }

    #[test]
    fn leaves_a_type_with_no_matching_name_untouched() {
        let ty = Ty::Union(
            vec![
                Ty::Int {
                    attr: TyAttr::default(),
                },
                Ty::Class(qtn("Other"), Vec::new(), TyAttr::default()),
            ],
            TyAttr::default(),
        );
        assert_eq!(ty.map_type_names(&minted), ty);
    }

    /// A template keeps its positional frame references and renames around them.
    #[test]
    fn a_template_renames_under_a_frame_reference() {
        let template = TyTemplate::Class(
            qtn("Item"),
            vec![TyTemplate::TypeArgRef(0)],
            TyAttr::default(),
        );
        let TyTemplate::Class(name, args, _) = template.map_type_names(&minted) else {
            panic!("class template must stay a class template");
        };
        assert!(name.is_runtime_minted());
        assert_eq!(args, vec![TyTemplate::TypeArgRef(0)]);
    }

    /// The narrowed members route through `Ty` and come back as themselves.
    #[test]
    fn a_realized_type_round_trips_through_the_widened_rewrite() {
        let ty = RealizedTy::list(RealizedTy::Class(
            qtn("Item"),
            Vec::new(),
            TyAttr::default(),
        ));
        let RealizedTy::List(inner, _) = ty.map_type_names(&minted) else {
            panic!("list must stay a list");
        };
        assert!(matches!(inner.as_ref(), RealizedTy::Class(name, ..) if name.is_runtime_minted()));
    }
}
