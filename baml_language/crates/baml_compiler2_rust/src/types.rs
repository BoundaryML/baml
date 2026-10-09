//! The native type of a BAML value.
//!
//! [`NativeTy`] is the subset of BAML's runtime types the backend represents
//! natively, and [`from_runtime_ty`] decides membership. The mapping to Rust
//! spellings ([`NativeTy::to_tokens`]) is the value model of the design: `int`
//! is `Int63`, `string` is `bex_aot::Str`, arrays and class instances are
//! `Shared<..>` handles with reference semantics, and `T | null` is
//! `Option<T>`.

use std::fmt;

use baml_compiler2_hir_ty::extern_loc::{ClassRef, EnumRef};
use baml_compiler2_mir::RuntimeTy;
use baml_type::{DeclName, Literal};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

/// The native representation of a BAML type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NativeTy<'db> {
    /// BAML `int`: `bex_aot::Int63`.
    Int,
    /// BAML `bool`.
    Bool,
    /// BAML `float`: `f64`.
    Float,
    /// BAML `string`: `bex_aot::Str`.
    Str,
    /// BAML `null` (and so `void`, which is `null`): the unit type.
    Null,
    /// BAML `T[]`: `Shared<Vec<T>>`.
    Array(Box<NativeTy<'db>>),
    /// A non-generic class: `Shared<Struct>` over a generated struct.
    Class(ClassRef<'db>),
    /// A BAML enum: a generated fieldless Rust enum, one variant per BAML
    /// variant in declaration order, so the discriminant is the VM's.
    Enum(EnumRef<'db>),
    /// BAML `T | null`: `Option<T>`.
    Option(Box<NativeTy<'db>>),
    /// The iterator `iter()` yields on a `T[]`: `bex_aot::array::Iter<T>`.
    /// Never declared in BAML source; a refined type.
    ArrayIter(Box<NativeTy<'db>>),
}

/// A declaration a native type names, which the generated module emits as
/// an item: the key under which [`NativeTy::to_tokens`] and
/// [`NativeTy::describe`] look its name up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeDecl<'db> {
    Class(ClassRef<'db>),
    Enum(EnumRef<'db>),
}

/// Why a runtime type has no native representation: the construct it uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unsupported(pub(crate) String);

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'db> NativeTy<'db> {
    /// Whether values of this type are `Copy` in the generated code, so a
    /// read needs no `.clone()`.
    pub fn is_copy(&self) -> bool {
        match self {
            Self::Int | Self::Bool | Self::Float | Self::Null | Self::Enum(_) => true,
            Self::Option(inner) => inner.is_copy(),
            Self::Str | Self::Array(_) | Self::Class(_) | Self::ArrayIter(_) => false,
        }
    }

    /// Whether the host shim can parse a command-line argument of this type:
    /// `int`, `bool`, `float` and `string`. Every type prints through
    /// `ToBaml`, so results are never the problem.
    pub fn is_shim_argument(&self) -> bool {
        matches!(self, Self::Int | Self::Bool | Self::Float | Self::Str)
    }

    /// Every class this type mentions, outermost first.
    pub fn classes(&self, out: &mut Vec<ClassRef<'db>>) {
        match self {
            Self::Class(class) => out.push(*class),
            Self::Array(inner) | Self::Option(inner) | Self::ArrayIter(inner) => {
                inner.classes(out);
            }
            Self::Int | Self::Bool | Self::Float | Self::Str | Self::Null | Self::Enum(_) => {}
        }
    }

    /// Every enum this type mentions, outermost first.
    pub fn enums(&self, out: &mut Vec<EnumRef<'db>>) {
        match self {
            Self::Enum(enum_ref) => out.push(*enum_ref),
            Self::Array(inner) | Self::Option(inner) | Self::ArrayIter(inner) => {
                inner.enums(out);
            }
            Self::Int | Self::Bool | Self::Float | Self::Str | Self::Null | Self::Class(_) => {}
        }
    }

    /// The Rust spelling of this type. `name` gives each class's generated
    /// struct identifier and each enum's generated enum identifier.
    pub fn to_tokens(&self, name: &dyn Fn(TypeDecl<'db>) -> Ident) -> TokenStream {
        let class_name = name;
        match self {
            Self::Int => quote! { Int63 },
            Self::Bool => quote! { bool },
            Self::Float => quote! { f64 },
            Self::Str => quote! { Str },
            Self::Null => quote! { () },
            Self::Array(inner) => {
                let inner = inner.to_tokens(class_name);
                quote! { Shared<Vec<#inner>> }
            }
            Self::Class(class) => {
                let name = class_name(TypeDecl::Class(*class));
                quote! { Shared<#name> }
            }
            Self::Enum(enum_ref) => {
                let name = class_name(TypeDecl::Enum(*enum_ref));
                quote! { #name }
            }
            Self::Option(inner) => {
                let inner = inner.to_tokens(class_name);
                quote! { Option<#inner> }
            }
            Self::ArrayIter(inner) => {
                let inner = inner.to_tokens(class_name);
                quote! { bex_aot::array::Iter<#inner> }
            }
        }
    }

    /// A BAML-flavoured description, with classes and enums rendered by
    /// `name`.
    pub fn describe(&self, name: &dyn Fn(TypeDecl<'db>) -> String) -> String {
        let class_name = name;
        match self {
            Self::Int => "int".into(),
            Self::Bool => "bool".into(),
            Self::Float => "float".into(),
            Self::Str => "string".into(),
            Self::Null => "null".into(),
            Self::Array(inner) => format!("{}[]", inner.describe(class_name)),
            Self::Class(class) => class_name(TypeDecl::Class(*class)),
            Self::Enum(enum_ref) => class_name(TypeDecl::Enum(*enum_ref)),
            Self::Option(inner) => format!("{} | null", inner.describe(class_name)),
            Self::ArrayIter(inner) => {
                format!("baml.iter.Iterator<Item = {}>", inner.describe(class_name))
            }
        }
    }
}

/// How a value of one native type is stored into a place of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Coercion {
    /// The same type.
    Identity,
    /// `T` into `T | null`: `Some(v)`.
    Wrap,
    /// A `null`-typed value (the unit) into `T | null`: `None`.
    Null,
    /// `T | null` into `T`: the for-in element copy after the `Done` test,
    /// `v.expect(..)`. Only an assignment's `Use` may do this; anywhere else
    /// it is a value the checker narrowed, which the subset cannot follow.
    Unwrap,
}

impl Coercion {
    /// Whether the store needs no unwrap.
    pub(crate) fn is_total(self) -> bool {
        matches!(self, Self::Identity | Self::Wrap | Self::Null)
    }
}

/// Whether `actual` can be stored into a place of type `expected`, and how.
pub(crate) fn coercion(actual: &NativeTy<'_>, expected: &NativeTy<'_>) -> Option<Coercion> {
    if actual == expected {
        Some(Coercion::Identity)
    } else if matches!(expected, NativeTy::Option(inner) if **inner == *actual) {
        Some(Coercion::Wrap)
    } else if *actual == NativeTy::Null && matches!(expected, NativeTy::Option(_)) {
        Some(Coercion::Null)
    } else if matches!(actual, NativeTy::Option(inner) if **inner == *expected) {
        Some(Coercion::Unwrap)
    } else {
        None
    }
}

/// Resolves the declarations a runtime type names: a class head (with its
/// type arguments) to a [`NativeTy::Class`] and an enum head to a
/// [`NativeTy::Enum`], or explains why the declaration is outside the subset.
pub(crate) trait Resolver<'db> {
    fn class(&mut self, head: &DeclName, args: &[RuntimeTy]) -> Result<NativeTy<'db>, Unsupported>;
    fn enum_(&mut self, head: &DeclName) -> Result<NativeTy<'db>, Unsupported>;
}

/// Map a runtime type to its native representation, resolving the classes
/// and enums it names through `decls`.
///
/// Rejected here: `unknown` and interface types (they may still be refined
/// from their defining rvalue by the caller), maps, unions other than
/// `T | null`, `bigint`, `uint8array`, media, functions, futures, type
/// aliases, type variables and the compiler-only sentinels.
pub(crate) fn from_runtime_ty<'db>(
    ty: &RuntimeTy,
    decls: &mut dyn Resolver<'db>,
) -> Result<NativeTy<'db>, Unsupported> {
    let class = decls;
    Ok(match ty {
        RuntimeTy::Int => NativeTy::Int,
        RuntimeTy::Bool => NativeTy::Bool,
        RuntimeTy::Float => NativeTy::Float,
        RuntimeTy::String => NativeTy::Str,
        RuntimeTy::Null => NativeTy::Null,
        RuntimeTy::Literal(literal, _) => match literal {
            Literal::Int(_) => NativeTy::Int,
            Literal::Bool(_) => NativeTy::Bool,
            Literal::Float(_) => NativeTy::Float,
            Literal::String(_) => NativeTy::Str,
            Literal::Bigint(_) => return Err(Unsupported("bigint".into())),
        },
        RuntimeTy::List(inner) => NativeTy::Array(Box::new(from_runtime_ty(inner, class)?)),
        RuntimeTy::Class(head, args) => class.class(head, args)?,
        RuntimeTy::Union(members) => {
            let mut value = None;
            for member in members {
                match member {
                    RuntimeTy::Null => {}
                    other if value.is_none() => value = Some(other),
                    _ => return Err(Unsupported("union other than `T | null`".into())),
                }
            }
            match value {
                Some(inner) if members.len() == 2 => {
                    let inner = from_runtime_ty(inner, class)?;
                    if matches!(inner, NativeTy::Option(_)) {
                        return Err(Unsupported("union other than `T | null`".into()));
                    }
                    NativeTy::Option(Box::new(inner))
                }
                _ => return Err(Unsupported("union other than `T | null`".into())),
            }
        }
        // A variant type (`Color.Red`) is a literal type: its value is the
        // enum's.
        RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => class.enum_(head)?,
        RuntimeTy::Map { .. } => return Err(Unsupported("map".into())),
        RuntimeTy::Bigint => return Err(Unsupported("bigint".into())),
        RuntimeTy::Uint8Array => return Err(Unsupported("uint8array".into())),
        RuntimeTy::Media(_) => return Err(Unsupported("media".into())),
        RuntimeTy::Function { .. } => return Err(Unsupported("function type".into())),
        RuntimeTy::Future(..) => return Err(Unsupported("future".into())),
        RuntimeTy::Interface(..) => return Err(Unsupported("interface".into())),
        RuntimeTy::Unknown => return Err(Unsupported("unknown".into())),
        RuntimeTy::Never => return Err(Unsupported("never".into())),
        RuntimeTy::TypeAlias(_) => return Err(Unsupported("type alias".into())),
        RuntimeTy::TypeVar(_) => return Err(Unsupported("type variable".into())),
        RuntimeTy::AssociatedTypeProjection { .. } => {
            return Err(Unsupported("associated type projection".into()));
        }
        RuntimeTy::RustType | RuntimeTy::Resource | RuntimeTy::PromptAst => {
            return Err(Unsupported("opaque runtime type".into()));
        }
        RuntimeTy::Type => return Err(Unsupported("type value".into())),
    })
}

#[cfg(test)]
mod tests {
    use baml_type::Freshness;

    use super::*;

    struct NoDecls;

    impl<'db> Resolver<'db> for NoDecls {
        fn class(&mut self, _: &DeclName, _: &[RuntimeTy]) -> Result<NativeTy<'db>, Unsupported> {
            Err(Unsupported("class".into()))
        }

        fn enum_(&mut self, _: &DeclName) -> Result<NativeTy<'db>, Unsupported> {
            Err(Unsupported("enum".into()))
        }
    }

    fn map<'db>(ty: &RuntimeTy) -> Result<NativeTy<'db>, Unsupported> {
        from_runtime_ty(ty, &mut NoDecls)
    }

    fn describe(ty: &NativeTy<'_>) -> String {
        ty.describe(&|_| "C".into())
    }

    fn tokens(ty: &NativeTy<'_>) -> String {
        ty.to_tokens(&|_| Ident::new("C", proc_macro2::Span::call_site()))
            .to_string()
            .replace(' ', "")
    }

    #[test]
    fn primitives_and_containers() {
        assert_eq!(map(&RuntimeTy::Int).unwrap(), NativeTy::Int);
        assert_eq!(map(&RuntimeTy::Bool).unwrap(), NativeTy::Bool);
        assert_eq!(map(&RuntimeTy::Float).unwrap(), NativeTy::Float);
        assert_eq!(map(&RuntimeTy::String).unwrap(), NativeTy::Str);
        assert_eq!(map(&RuntimeTy::Null).unwrap(), NativeTy::Null);
        assert_eq!(
            map(&RuntimeTy::List(Box::new(RuntimeTy::List(Box::new(
                RuntimeTy::String
            )))))
            .unwrap(),
            NativeTy::Array(Box::new(NativeTy::Array(Box::new(NativeTy::Str))))
        );
        assert_eq!(
            map(&RuntimeTy::Literal(Literal::Int(0), Freshness::Regular)).unwrap(),
            NativeTy::Int
        );
        assert_eq!(
            map(&RuntimeTy::Literal(
                Literal::String("x".into()),
                Freshness::Regular
            ))
            .unwrap(),
            NativeTy::Str
        );
    }

    #[test]
    fn nullable_union_is_option() {
        let nullable = RuntimeTy::Union(Box::new([RuntimeTy::Int, RuntimeTy::Null]));
        assert_eq!(
            map(&nullable).unwrap(),
            NativeTy::Option(Box::new(NativeTy::Int))
        );
        let reversed = RuntimeTy::Union(Box::new([RuntimeTy::Null, RuntimeTy::String]));
        assert_eq!(
            map(&reversed).unwrap(),
            NativeTy::Option(Box::new(NativeTy::Str))
        );
        let wide = RuntimeTy::Union(Box::new([
            RuntimeTy::Int,
            RuntimeTy::String,
            RuntimeTy::Null,
        ]));
        assert_eq!(map(&wide).unwrap_err().0, "union other than `T | null`");
        let two = RuntimeTy::Union(Box::new([RuntimeTy::Int, RuntimeTy::Float]));
        assert!(map(&two).is_err());
    }

    #[test]
    fn rejected_types_name_their_construct() {
        let map_ty = RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(RuntimeTy::Int),
        };
        assert_eq!(map(&map_ty).unwrap_err().0, "map");
        assert_eq!(map(&RuntimeTy::Bigint).unwrap_err().0, "bigint");
        assert_eq!(map(&RuntimeTy::Uint8Array).unwrap_err().0, "uint8array");
        assert_eq!(map(&RuntimeTy::Unknown).unwrap_err().0, "unknown");
        assert_eq!(
            map(&RuntimeTy::List(Box::new(RuntimeTy::Unknown)))
                .unwrap_err()
                .0,
            "unknown"
        );
        assert_eq!(map(&RuntimeTy::Type).unwrap_err().0, "type value");
    }

    #[test]
    fn coercions() {
        let int = NativeTy::Int;
        let nullable = NativeTy::Option(Box::new(NativeTy::Int));
        assert_eq!(coercion(&int, &int), Some(Coercion::Identity));
        assert_eq!(coercion(&int, &nullable), Some(Coercion::Wrap));
        assert_eq!(coercion(&nullable, &int), Some(Coercion::Unwrap));
        assert_eq!(coercion(&NativeTy::Str, &nullable), None);
        assert_eq!(coercion(&NativeTy::Null, &nullable), Some(Coercion::Null));
        assert_eq!(coercion(&nullable, &NativeTy::Null), None);
    }

    #[test]
    fn rust_spellings() {
        let array = NativeTy::Array(Box::new(NativeTy::Int));
        assert_eq!(tokens(&array), "Shared<Vec<Int63>>");
        assert_eq!(
            tokens(&NativeTy::Option(Box::new(NativeTy::Str))),
            "Option<Str>"
        );
        assert_eq!(
            tokens(&NativeTy::ArrayIter(Box::new(NativeTy::Float))),
            "bex_aot::array::Iter<f64>"
        );
        assert_eq!(tokens(&NativeTy::Null), "()");
        assert_eq!(describe(&array), "int[]");
        assert_eq!(
            describe(&NativeTy::Option(Box::new(array.clone()))),
            "int[] | null"
        );
        assert!(NativeTy::Int.is_copy());
        assert!(NativeTy::Option(Box::new(NativeTy::Bool)).is_copy());
        assert!(!NativeTy::Str.is_copy());
        assert!(!array.is_copy());
    }
}
