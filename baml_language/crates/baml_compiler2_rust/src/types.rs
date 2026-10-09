//! The native type of a BAML value.
//!
//! [`NativeTy`] is the subset of BAML's runtime types the backend represents
//! natively, and [`from_runtime_ty`] decides membership. The mapping to Rust
//! spellings ([`NativeTy::to_tokens`]) is the value model of the design: `int`
//! is `Int63`, `string` is `bex_aot::Str`, arrays and class instances are
//! `Shared<..>` handles with reference semantics, `T | null` is
//! `Option<T>`, and a closed union of two or more other types is a generated
//! Rust enum with one variant per member ([`NativeTy::Union`]).

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
    /// BAML `bigint`: `bex_aot::BigInt`, a reference-counted value.
    Bigint,
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
    /// BAML `map<K, V>`: `bex_aot::map::Map<K, V>`, a handle over an
    /// insertion-ordered table. `K` is `int`, `bool` or `string`.
    Map(Box<NativeTy<'db>>, Box<NativeTy<'db>>),
    /// BAML `T | null`: `Option<T>`.
    Option(Box<NativeTy<'db>>),
    /// A closed union of two or more members, none of them `null` (which
    /// wraps the union in [`NativeTy::Option`]) and none a union: a
    /// generated Rust enum with one variant per member, in the order the
    /// type system canonicalizes the members (so one spelling's `float |
    /// int` is the other's `int | float`). Members are the types above;
    /// `unknown`, interfaces and generic classes make a union open, which
    /// has no native type.
    Union(Vec<NativeTy<'db>>),
    /// The iterator `iter()` yields on a `T[]`: `bex_aot::array::Iter<T>`.
    /// Never declared in BAML source; a refined type.
    ArrayIter(Box<NativeTy<'db>>),
    /// The value a `catch` handler lands with: `bex_aot::Thrown`, a panic or
    /// a thrown class instance. Only a handler's error local and the
    /// temporaries a `catch` arm narrows from it have this type; never a
    /// parameter, a return or a field.
    Thrown,
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
            // The generated enum derives `Copy` exactly when every member is.
            Self::Union(members) => members.iter().all(NativeTy::is_copy),
            Self::Str
            | Self::Bigint
            | Self::Array(_)
            | Self::Map(..)
            | Self::Class(_)
            | Self::ArrayIter(_)
            | Self::Thrown => false,
        }
    }

    /// The members of a union, or of a nullable union.
    pub fn union_members(&self) -> Option<&[NativeTy<'db>]> {
        match self {
            Self::Union(members) => Some(members),
            Self::Option(inner) => match &**inner {
                Self::Union(members) => Some(members),
                _ => None,
            },
            _ => None,
        }
    }

    /// The identifier of the generated enum for a union with these members,
    /// a pure function of the member types: `Union_int_or_float`,
    /// `Union_user_A_or_user_B`.
    pub fn union_ident(members: &[NativeTy<'db>], name: &dyn Fn(TypeDecl<'db>) -> Ident) -> Ident {
        Ident::new(
            &format!("Union_{}", Self::mangle_members(members, name)),
            proc_macro2::Span::call_site(),
        )
    }

    /// The members mangled and joined with `_or_`.
    fn mangle_members(members: &[NativeTy<'db>], name: &dyn Fn(TypeDecl<'db>) -> Ident) -> String {
        members
            .iter()
            .map(|member| member.mangle(name))
            .collect::<Vec<_>>()
            .join("_or_")
    }

    /// This type as an identifier fragment: the variant name of a union
    /// member (`int`, `string`, `int_array`, `user_A`), and a part of the
    /// union's own name.
    pub fn mangle(&self, name: &dyn Fn(TypeDecl<'db>) -> Ident) -> String {
        match self {
            Self::Int => "int".into(),
            Self::Bool => "bool".into(),
            Self::Float => "float".into(),
            Self::Bigint => "bigint".into(),
            Self::Str => "string".into(),
            Self::Null => "null".into(),
            Self::Array(inner) => format!("{}_array", inner.mangle(name)),
            Self::Map(key, value) => format!("map_{}_{}", key.mangle(name), value.mangle(name)),
            Self::Class(class) => name(TypeDecl::Class(*class)).to_string(),
            Self::Enum(enum_ref) => name(TypeDecl::Enum(*enum_ref)).to_string(),
            Self::Option(inner) => format!("{}_or_null", inner.mangle(name)),
            Self::Union(members) => Self::mangle_members(members, name),
            Self::ArrayIter(inner) => format!("{}_iter", inner.mangle(name)),
            Self::Thrown => "thrown".into(),
        }
    }

    /// Every union this type mentions, innermost first, each as its member
    /// list.
    pub fn unions(&self, out: &mut Vec<Vec<NativeTy<'db>>>) {
        match self {
            Self::Union(members) => {
                for member in members {
                    member.unions(out);
                }
                if !out.contains(members) {
                    out.push(members.clone());
                }
            }
            Self::Array(inner) | Self::Option(inner) | Self::ArrayIter(inner) => {
                inner.unions(out);
            }
            Self::Map(key, value) => {
                key.unions(out);
                value.unions(out);
            }
            Self::Int
            | Self::Bool
            | Self::Float
            | Self::Bigint
            | Self::Str
            | Self::Null
            | Self::Class(_)
            | Self::Enum(_)
            | Self::Thrown => {}
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
            Self::Map(key, value) => {
                key.classes(out);
                value.classes(out);
            }
            Self::Union(members) => {
                for member in members {
                    member.classes(out);
                }
            }
            Self::Int
            | Self::Bool
            | Self::Float
            | Self::Bigint
            | Self::Str
            | Self::Null
            | Self::Enum(_)
            | Self::Thrown => {}
        }
    }

    /// Every enum this type mentions, outermost first.
    pub fn enums(&self, out: &mut Vec<EnumRef<'db>>) {
        match self {
            Self::Enum(enum_ref) => out.push(*enum_ref),
            Self::Array(inner) | Self::Option(inner) | Self::ArrayIter(inner) => {
                inner.enums(out);
            }
            Self::Map(key, value) => {
                key.enums(out);
                value.enums(out);
            }
            Self::Union(members) => {
                for member in members {
                    member.enums(out);
                }
            }
            Self::Int
            | Self::Bool
            | Self::Float
            | Self::Bigint
            | Self::Str
            | Self::Null
            | Self::Class(_)
            | Self::Thrown => {}
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
            Self::Bigint => quote! { BigInt },
            Self::Str => quote! { Str },
            Self::Null => quote! { () },
            Self::Array(inner) => {
                let inner = inner.to_tokens(class_name);
                quote! { Shared<Vec<#inner>> }
            }
            Self::Map(key, value) => {
                let key = key.to_tokens(class_name);
                let value = value.to_tokens(class_name);
                quote! { Map<#key, #value> }
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
            Self::Union(members) => {
                let name = Self::union_ident(members, class_name);
                quote! { #name }
            }
            Self::ArrayIter(inner) => {
                let inner = inner.to_tokens(class_name);
                quote! { bex_aot::array::Iter<#inner> }
            }
            Self::Thrown => quote! { Thrown },
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
            Self::Bigint => "bigint".into(),
            Self::Str => "string".into(),
            Self::Null => "null".into(),
            Self::Array(inner) => format!("{}[]", inner.describe(class_name)),
            Self::Map(key, value) => format!(
                "map<{}, {}>",
                key.describe(class_name),
                value.describe(class_name)
            ),
            Self::Class(class) => class_name(TypeDecl::Class(*class)),
            Self::Enum(enum_ref) => class_name(TypeDecl::Enum(*enum_ref)),
            Self::Option(inner) => format!("{} | null", inner.describe(class_name)),
            Self::Union(members) => members
                .iter()
                .map(|member| member.describe(class_name))
                .collect::<Vec<_>>()
                .join(" | "),
            Self::ArrayIter(inner) => {
                format!("baml.iter.Iterator<Item = {}>", inner.describe(class_name))
            }
            Self::Thrown => "thrown value".into(),
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
    /// or a value the checker narrowed after a null test, `v.expect(..)`.
    Unwrap,
    /// A member into its union: the variant holding it.
    Inject,
    /// A member into a nullable union: `Some(variant)`.
    InjectSome,
    /// A union into a union with every member of it and more: a `match`
    /// re-tagging each variant.
    Widen,
    /// [`Self::Widen`], then `Some`.
    WidenSome,
    /// A nullable member into a nullable union: `v.map(variant)`.
    MapInject,
    /// A nullable union into a wider nullable union.
    MapWiden,
    /// A union into one of its members: a value the checker narrowed by a
    /// type test; the other variants are unreachable.
    Narrow,
    /// A union into a union of some of its members: narrowed likewise.
    NarrowUnion,
    /// A nullable union into one of its members: narrowed by a null test
    /// and a type test.
    UnwrapNarrow,
}

impl Coercion {
    /// Whether the store needs no unwrap: anywhere else a value is a value
    /// the checker narrowed, which only a `Use` or an operand may be.
    pub(crate) fn is_total(self) -> bool {
        !self.narrows()
    }

    /// Whether the store relies on the checker's narrowing: the source may
    /// hold a value the destination cannot, which the checker proved it
    /// does not here.
    pub(crate) fn narrows(self) -> bool {
        matches!(
            self,
            Self::Unwrap | Self::Narrow | Self::NarrowUnion | Self::UnwrapNarrow
        )
    }
}

/// Whether every member of `inner` is a member of `outer`.
fn subset(inner: &[NativeTy<'_>], outer: &[NativeTy<'_>]) -> bool {
    inner.iter().all(|member| outer.contains(member))
}

/// Whether `actual` can be stored into a place of type `expected`, and how.
pub(crate) fn coercion(actual: &NativeTy<'_>, expected: &NativeTy<'_>) -> Option<Coercion> {
    use NativeTy::{Null, Option as Opt, Union};
    if actual == expected {
        return Some(Coercion::Identity);
    }
    Some(match (actual, expected) {
        (_, Opt(inner)) if **inner == *actual => Coercion::Wrap,
        (Null, Opt(_)) => Coercion::Null,
        (Opt(inner), _) if **inner == *expected => Coercion::Unwrap,
        (Union(from), Union(to)) if subset(from, to) => Coercion::Widen,
        (Union(from), Union(to)) if subset(to, from) => Coercion::NarrowUnion,
        (member, Union(members)) if members.contains(member) => Coercion::Inject,
        (Union(members), member) if members.contains(member) => Coercion::Narrow,
        (Opt(from), Opt(to)) => match (&**from, &**to) {
            (Union(from), Union(to)) if subset(from, to) => Coercion::MapWiden,
            (member, Union(members)) if members.contains(member) => Coercion::MapInject,
            _ => return None,
        },
        (Union(from), Opt(to)) => match &**to {
            Union(to) if subset(from, to) => Coercion::WidenSome,
            _ => return None,
        },
        (member, Opt(to)) => match &**to {
            Union(members) if members.contains(member) => Coercion::InjectSome,
            _ => return None,
        },
        (Opt(from), member) => match &**from {
            Union(members) if members.contains(member) => Coercion::UnwrapNarrow,
            _ => return None,
        },
        _ => return None,
    })
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
/// from their defining rvalue by the caller), maps keyed by anything but
/// `int`, `bool` or `string`, unions other than `T | null`, `uint8array`,
/// media, functions, futures, type aliases, type variables and the
/// compiler-only sentinels.
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
            Literal::Bigint(_) => NativeTy::Bigint,
        },
        RuntimeTy::List(inner) => NativeTy::Array(Box::new(from_runtime_ty(inner, class)?)),
        RuntimeTy::Class(head, args) => class.class(head, args)?,
        RuntimeTy::Union(members) => {
            // Members fold to their native types first: every literal of one
            // primitive is that primitive (`"a" | "b"` is a `string`), a
            // variant type its enum, so a union of literals erases; a nested
            // union contributes its members. `null` makes the result
            // nullable. Two or more distinct members are a generated enum,
            // in the order the type system gives them.
            let mut nullable = false;
            let mut distinct: Vec<NativeTy<'db>> = Vec::new();
            let mut flat = Vec::new();
            flatten_union(members, &mut flat);
            for member in flat {
                if matches!(member, RuntimeTy::Null) {
                    nullable = true;
                    continue;
                }
                let native = from_runtime_ty(member, class).map_err(|Unsupported(reason)| {
                    Unsupported(format!("{reason} (a union member)"))
                })?;
                match native {
                    // `T | null` as a member is `T` and `null` as members.
                    NativeTy::Option(inner) => {
                        nullable = true;
                        if !distinct.contains(&inner) {
                            distinct.push(*inner);
                        }
                    }
                    NativeTy::Union(inner) => {
                        for member in inner {
                            if !distinct.contains(&member) {
                                distinct.push(member);
                            }
                        }
                    }
                    other => {
                        if !distinct.contains(&other) {
                            distinct.push(other);
                        }
                    }
                }
            }
            let inner = match distinct.len() {
                0 => NativeTy::Null,
                1 => distinct.remove(0),
                _ => NativeTy::Union(distinct),
            };
            if nullable && inner != NativeTy::Null {
                NativeTy::Option(Box::new(inner))
            } else {
                inner
            }
        }
        // A variant type (`Color.Red`) is a literal type: its value is the
        // enum's.
        RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => class.enum_(head)?,
        RuntimeTy::Map { key, value } => {
            let key = from_runtime_ty(key, class)?;
            if !matches!(key, NativeTy::Int | NativeTy::Bool | NativeTy::Str) {
                // Keys compare by value on both backends only for these;
                // a `float` key's NaN handling, a class's `Hash` and
                // `Equals`, are not reproduced natively.
                return Err(Unsupported(format!(
                    "map key type {}",
                    key.describe(&|_| "class".into())
                )));
            }
            let value = from_runtime_ty(value, class)?;
            NativeTy::Map(Box::new(key), Box::new(value))
        }
        RuntimeTy::Bigint => NativeTy::Bigint,
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

/// The members of a union with nested unions spliced in, in order.
fn flatten_union<'a>(members: &'a [RuntimeTy], out: &mut Vec<&'a RuntimeTy>) {
    for member in members {
        match member {
            RuntimeTy::Union(inner) => flatten_union(inner, out),
            other => out.push(other),
        }
    }
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
        assert_eq!(
            map(&wide).unwrap(),
            NativeTy::Option(Box::new(NativeTy::Union(vec![
                NativeTy::Int,
                NativeTy::Str
            ])))
        );
        let two = RuntimeTy::Union(Box::new([RuntimeTy::Int, RuntimeTy::Float]));
        assert_eq!(
            map(&two).unwrap(),
            NativeTy::Union(vec![NativeTy::Int, NativeTy::Float])
        );
    }

    #[test]
    fn closed_unions_keep_their_members_in_order() {
        let int_float = NativeTy::Union(vec![NativeTy::Int, NativeTy::Float]);
        assert_eq!(tokens(&int_float), "Union_int_or_float");
        assert_eq!(describe(&int_float), "int | float");
        assert!(int_float.is_copy());
        let with_string = RuntimeTy::Union(Box::new([
            RuntimeTy::Int,
            RuntimeTy::Union(Box::new([RuntimeTy::Float, RuntimeTy::Null])),
            RuntimeTy::List(Box::new(RuntimeTy::String)),
        ]));
        let mapped = map(&with_string).unwrap();
        assert_eq!(
            mapped,
            NativeTy::Option(Box::new(NativeTy::Union(vec![
                NativeTy::Int,
                NativeTy::Float,
                NativeTy::Array(Box::new(NativeTy::Str)),
            ])))
        );
        assert!(!mapped.is_copy());
        assert_eq!(
            tokens(&mapped),
            "Option<Union_int_or_float_or_string_array>"
        );
        assert_eq!(describe(&mapped), "int | float | string[] | null");
        let mut unions = Vec::new();
        mapped.unions(&mut unions);
        assert_eq!(unions.len(), 1);
        let open = RuntimeTy::Union(Box::new([RuntimeTy::Int, RuntimeTy::Unknown]));
        assert_eq!(map(&open).unwrap_err().0, "unknown (a union member)");
    }

    #[test]
    fn union_coercions() {
        use NativeTy::{Float, Int, Str};
        let int_float = NativeTy::Union(vec![Int, Float]);
        let three = NativeTy::Union(vec![Int, Float, Str]);
        let nullable = NativeTy::Option(Box::new(int_float.clone()));
        assert_eq!(coercion(&Int, &int_float), Some(Coercion::Inject));
        assert_eq!(coercion(&Int, &nullable), Some(Coercion::InjectSome));
        assert_eq!(coercion(&Str, &int_float), None);
        assert_eq!(coercion(&int_float, &three), Some(Coercion::Widen));
        assert_eq!(
            coercion(&int_float, &NativeTy::Option(Box::new(three.clone()))),
            Some(Coercion::WidenSome)
        );
        assert_eq!(coercion(&three, &int_float), Some(Coercion::NarrowUnion));
        assert_eq!(coercion(&int_float, &Int), Some(Coercion::Narrow));
        assert_eq!(coercion(&nullable, &Int), Some(Coercion::UnwrapNarrow));
        assert_eq!(coercion(&nullable, &int_float), Some(Coercion::Unwrap));
        assert_eq!(
            coercion(&NativeTy::Option(Box::new(Int)), &nullable),
            Some(Coercion::MapInject)
        );
        assert_eq!(
            coercion(&nullable, &NativeTy::Option(Box::new(three))),
            Some(Coercion::MapWiden)
        );
        assert!(Coercion::Inject.is_total());
        assert!(!Coercion::Narrow.is_total());
    }

    #[test]
    fn literal_unions_erase_to_their_primitive() {
        let literal =
            |text: &str| RuntimeTy::Literal(Literal::String(text.into()), Freshness::Regular);
        let ab = RuntimeTy::Union(Box::new([literal("a"), literal("b")]));
        assert_eq!(map(&ab).unwrap(), NativeTy::Str);
        let nullable = RuntimeTy::Union(Box::new([literal("a"), RuntimeTy::Null, literal("b")]));
        assert_eq!(
            map(&nullable).unwrap(),
            NativeTy::Option(Box::new(NativeTy::Str))
        );
        let ints = RuntimeTy::Union(Box::new([
            RuntimeTy::Literal(Literal::Int(1), Freshness::Regular),
            RuntimeTy::Literal(Literal::Int(2), Freshness::Regular),
            RuntimeTy::Int,
        ]));
        assert_eq!(map(&ints).unwrap(), NativeTy::Int);
        let mixed = RuntimeTy::Union(Box::new([
            RuntimeTy::Literal(Literal::Int(1), Freshness::Regular),
            literal("a"),
        ]));
        assert_eq!(
            map(&mixed).unwrap(),
            NativeTy::Union(vec![NativeTy::Int, NativeTy::Str])
        );
    }

    #[test]
    fn maps_take_string_int_and_bool_keys() {
        let map_ty = |key: RuntimeTy| RuntimeTy::Map {
            key: Box::new(key),
            value: Box::new(RuntimeTy::Int),
        };
        assert_eq!(
            map(&map_ty(RuntimeTy::String)).unwrap(),
            NativeTy::Map(Box::new(NativeTy::Str), Box::new(NativeTy::Int))
        );
        assert!(map(&map_ty(RuntimeTy::Int)).is_ok());
        assert!(map(&map_ty(RuntimeTy::Bool)).is_ok());
        assert_eq!(
            map(&map_ty(RuntimeTy::Float)).unwrap_err().0,
            "map key type float"
        );
        assert_eq!(
            map(&map_ty(RuntimeTy::List(Box::new(RuntimeTy::Int))))
                .unwrap_err()
                .0,
            "map key type int[]"
        );
        let nested = NativeTy::Map(
            Box::new(NativeTy::Str),
            Box::new(NativeTy::Array(Box::new(NativeTy::Bool))),
        );
        assert_eq!(tokens(&nested), "Map<Str,Shared<Vec<bool>>>");
        assert_eq!(describe(&nested), "map<string, bool[]>");
        assert!(!nested.is_copy());
    }

    #[test]
    fn rejected_types_name_their_construct() {
        assert_eq!(map(&RuntimeTy::Bigint).unwrap(), NativeTy::Bigint);
        assert!(!NativeTy::Bigint.is_copy());
        assert_eq!(tokens(&NativeTy::Bigint), "BigInt");
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
