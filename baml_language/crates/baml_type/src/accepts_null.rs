//! Whether `null` is a value of a type, by what the type means rather than how
//! it is spelled.
//!
//! A class field may be left out of a class literal, and may be missing from
//! JSON, exactly when `null` is a value of its type: the field then holds
//! `null`. `int?` spells that with a union, but `unknown` admits `null` too,
//! and so does `baml.json.json`, a type alias whose union contains `null`.
//! The compiler, the JSON decoders, and `baml.json.schema` all ask
//! [`accepts_null`], so they agree on which fields are required.

use crate::{DeclName, Ty, interned};

/// One level of a type, as [`accepts_null`] reads it.
pub enum NullForm<'t, T, H> {
    /// `null` itself, or `unknown`, which admits every value.
    Admits,
    /// A union, which admits `null` when one of its members does.
    Union(&'t [T]),
    /// A type alias, which admits `null` when its definition does.
    Alias(&'t H),
    /// Any other type.
    Refuses,
}

/// A type representation [`accepts_null`] can read.
pub trait NullForms<H>: Sized {
    fn null_form(&self) -> NullForm<'_, Self, H>;
}

impl<H: Clone> NullForms<H> for Ty<H> {
    fn null_form(&self) -> NullForm<'_, Self, H> {
        match self {
            Ty::Null | Ty::Unknown => NullForm::Admits,
            Ty::Union(members) => NullForm::Union(members),
            Ty::TypeAlias(head) => NullForm::Alias(head),
            _ => NullForm::Refuses,
        }
    }
}

impl NullForms<DeclName> for interned::Ty {
    fn null_form(&self) -> NullForm<'_, Self, DeclName> {
        match self.kind() {
            interned::InferTy::Null | interned::InferTy::Unknown => NullForm::Admits,
            interned::InferTy::Union(members) => NullForm::Union(members),
            interned::InferTy::TypeAlias(name) => NullForm::Alias(name),
            _ => NullForm::Refuses,
        }
    }
}

/// Whether `null` is a value of `ty`: `ty` is `null` or `unknown`, a union
/// with such a member, or a type alias whose definition is one of these.
/// `alias_def` gives a type alias's definition; an alias it does not know
/// admits nothing.
pub fn accepts_null<H: PartialEq + Clone, T: NullForms<H>>(
    ty: &T,
    alias_def: &impl Fn(&H) -> Option<Ty<H>>,
) -> bool {
    accepts_null_within(ty, alias_def, &mut Vec::new())
}

/// [`accepts_null`], inside the aliases in `within`: an alias met again adds
/// nothing.
fn accepts_null_within<H: PartialEq + Clone, T: NullForms<H>>(
    ty: &T,
    alias_def: &impl Fn(&H) -> Option<Ty<H>>,
    within: &mut Vec<H>,
) -> bool {
    match ty.null_form() {
        NullForm::Admits => true,
        NullForm::Refuses => false,
        NullForm::Union(members) => members
            .iter()
            .any(|member| accepts_null_within(member, alias_def, within)),
        NullForm::Alias(head) => {
            if within.contains(head) {
                return false;
            }
            let Some(definition) = alias_def(head) else {
                return false;
            };
            within.push(head.clone());
            let accepts = accepts_null_within(&definition, alias_def, within);
            within.pop();
            accepts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Name, TypeName};

    fn alias(name: &str) -> Ty<TypeName> {
        Ty::TypeAlias(TypeName::local(Name::new(name)))
    }

    /// `Json = null | int | Json[]`, `Tree = Tree[] | int`, and `Loop = Loop`.
    fn definition(head: &TypeName) -> Option<Ty<TypeName>> {
        match head.name().as_str() {
            "Json" => Some(Ty::Union(Box::new([
                Ty::Null,
                Ty::Int,
                Ty::list(alias("Json")),
            ]))),
            "Tree" => Some(Ty::Union(Box::new([Ty::list(alias("Tree")), Ty::Int]))),
            "Loop" => Some(alias("Loop")),
            _ => None,
        }
    }

    #[test]
    fn null_is_a_value_by_meaning() {
        for (ty, expected) in [
            (Ty::Null, true),
            (Ty::Unknown, true),
            (Ty::optional(Ty::Int), true),
            (Ty::Union(Box::new([Ty::Int, Ty::Unknown])), true),
            (alias("Json"), true),
            (Ty::Union(Box::new([Ty::String, alias("Json")])), true),
            (Ty::Int, false),
            (alias("Tree"), false),
            (alias("Loop"), false),
            (alias("Missing"), false),
            (Ty::list(Ty::Null), false),
        ] {
            assert_eq!(accepts_null(&ty, &definition), expected, "{ty:?}");
        }
    }
}
