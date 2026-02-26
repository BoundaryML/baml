//! Based on the compiler-internal type system and SAP annotations: https://beps.boundaryml.com/beps/6
//! These are the interface used to transform JSON-like data into a typed representation.

mod assertions;
mod from_literal;
mod type_name;

use std::{borrow::Cow, fmt::Display};

pub use assertions::*;
use derive_more::From;
pub use from_literal::FromLiteral;
use indexmap::IndexMap;
pub use type_name::TypeName;

use crate::{
    baml_value::{
        BamlArray, BamlBool, BamlClass, BamlEnum, BamlFloat, BamlInt, BamlMap, BamlMedia, BamlNull,
        BamlPrimitive, BamlStreamState, BamlString, BamlValue,
    },
    deserializer::{
        coercer::{ParsingContext, ParsingError},
        types::{BamlValueWithFlags, DeserializerMeta},
    },
};

/// An identifier for a type. Used to look up a type in a `TypeRefDb`.
pub trait TypeIdent: Eq + std::hash::Hash + Display + Clone {}
impl TypeIdent for &'_ str {}
impl TypeIdent for String {}
impl TypeIdent for Cow<'_, str> {}
impl TypeIdent for u32 {}
impl TypeIdent for u64 {}
impl TypeIdent for usize {}

/// Stores all the "named" types (classes, enums, and type aliases) for lookup.
pub struct TypeRefDb<'t, N: TypeIdent> {
    types: IndexMap<N, TyResolved<'t, N>>,
}

impl<'t, N: TypeIdent> TypeRefDb<'t, N> {
    pub fn new() -> Self {
        Self {
            types: IndexMap::new(),
        }
    }

    /// Tries to add a type to the database. Returns an error if the identifier is already in use.
    pub fn try_add(&mut self, ident: N, ty: TyResolved<'t, N>) -> Result<(), &TyResolved<'t, N>> {
        match self.types.entry(ident) {
            indexmap::map::Entry::Occupied(entry) => Err(entry.into_mut()),
            indexmap::map::Entry::Vacant(entry) => {
                entry.insert(ty);
                Ok(())
            }
        }
    }

    pub fn resolve(&'t self, ty: &'t Ty<'t, N>) -> Result<TyResolvedRef<'t, N>, &'t N> {
        match ty {
            Ty::Resolved(ty) => Ok(ty.as_ref()),
            Ty::ResolvedRef(ty) => Ok(ty.clone()),
            Ty::Unresolved(ident) => self.types.get(ident).map(TyResolved::as_ref).ok_or(ident),
        }
    }

    pub fn resolve_with_meta(
        &'t self,
        ty: TyWithMeta<&'t Ty<'t, N>, &'t TypeAnnotations<'t, N>>,
    ) -> Result<TyWithMeta<TyResolvedRef<'t, N>, &'t TypeAnnotations<'t, N>>, &'t N> {
        self.resolve(ty.ty).map(|res| TyWithMeta::new(res, ty.meta))
    }
}

/// A trait that associates a BAML value type with a SAP model type.
/// Should be implemented for SAP model types.
pub trait TypeValue {
    /// The BAML value type associated with this SAP model type.
    type Value;
}

/// Contains a SAP model type, generally part of the one passed into the deserializer.
///
/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone, From)]
pub enum TyResolved<'t, N: TypeIdent> {
    Primitive(PrimitiveTy),
    Literal(LiteralTy<'t>),
    Array(ArrayTy<'t, N>),
    Map(MapTy<'t, N>),
    Class(ClassTy<'t, N>),
    Enum(EnumTy<'t, N>),
    Union(UnionTy<'t, N>),
    /// A type that tells you if it is completed or not.
    StreamState(StreamStateTy<'t, N>),
}
impl<'t, N: TypeIdent> TypeValue for TyResolved<'t, N> {
    type Value = BamlValue<'t, N>;
}
impl<'t, N: TypeIdent> TyResolved<'t, N> {
    pub fn as_ref(&'t self) -> TyResolvedRef<'t, N> {
        match self {
            TyResolved::Primitive(p) => TyResolvedRef::Primitive(*p),
            TyResolved::Literal(l) => TyResolvedRef::Literal(l),
            TyResolved::Array(a) => TyResolvedRef::Array(a),
            TyResolved::Map(m) => TyResolvedRef::Map(m),
            TyResolved::Class(c) => TyResolvedRef::Class(c),
            TyResolved::Enum(e) => TyResolvedRef::Enum(e),
            TyResolved::Union(u) => TyResolvedRef::Union(u),
            TyResolved::StreamState(s) => TyResolvedRef::StreamState(&*s),
        }
    }
}

/// [`TyResolved`] but with a reference to the SAP model type.
///
/// This is a reference to a SAP model type, generally part of the one passed into the deserializer.
/// At the top-level, any type references (identified by `N`) are resolved in this type.
#[derive(Clone, From)]
pub enum TyResolvedRef<'t, N: TypeIdent> {
    Primitive(PrimitiveTy),
    Literal(&'t LiteralTy<'t>),
    Array(&'t ArrayTy<'t, N>),
    Map(&'t MapTy<'t, N>),
    Class(&'t ClassTy<'t, N>),
    Enum(&'t EnumTy<'t, N>),
    Union(&'t UnionTy<'t, N>),
    /// A type that tells you if it is completed or not.
    StreamState(&'t StreamStateTy<'t, N>),
}
// Manual Copy impl because derive(Copy) adds an unnecessary `N: Copy` bound.
// All variants are either Copy-by-value (PrimitiveTy) or references (&'t T),
// so Copy is valid regardless of N.
impl<'t, N: TypeIdent> Copy for TyResolvedRef<'t, N> {}
impl<'t, N: TypeIdent> TyResolvedRef<'t, N> {
    /// Returns true if the type may be `null`.
    ///
    /// Requires `db` in case we need to look up type aliases that may contain optional unions.
    pub fn is_optional(&self, db: &TypeRefDb<'t, N>) -> bool {
        match self {
            TyResolvedRef::Primitive(PrimitiveTy::Null(..)) => true,
            TyResolvedRef::Union(u) => u.is_optional(db),
            TyResolvedRef::StreamState(s) => s.value.ty.is_optional(db),
            _ => false,
        }
    }
}
impl<'t, N: TypeIdent> TypeValue for TyResolvedRef<'t, N> {
    type Value = BamlValue<'t, N>;
}

#[derive(Clone, From)]
pub enum Ty<'t, N: TypeIdent> {
    #[from(TyResolved<'t, N>, LiteralTy<'t>, ArrayTy<'t, N>, MapTy<'t, N>, ClassTy<'t, N>, EnumTy<'t, N>, UnionTy<'t, N>)]
    Resolved(TyResolved<'t, N>),
    #[from(TyResolvedRef<'t, N>, PrimitiveTy, &'t LiteralTy<'t>, &'t ArrayTy<'t, N>, &'t MapTy<'t, N>, &'t ClassTy<'t, N>, &'t EnumTy<'t, N>, &'t UnionTy<'t, N>)]
    ResolvedRef(TyResolvedRef<'t, N>),
    /// Type needs to be looked up in the [`TypeRefDb`].
    /// This is since types may be recursive so we need some indirection.
    ///
    /// Note that the type may have a different identifier due to type aliases.
    ///
    /// Generally this should only be created when there is a class, enum?, or type alias,
    /// with the identifier being their unique identifiers.
    Unresolved(N),
}
impl<'t, N: TypeIdent> Ty<'t, N> {
    /// Returns true if the type may be `null`.
    ///
    /// Requires `db` in case we need to look up type aliases that may contain optional unions.
    pub fn is_optional(&'t self, db: &'t TypeRefDb<'t, N>) -> bool {
        db.resolve(self).map_or(false, |ty| ty.is_optional(db))
    }
}
impl<'t, N: TypeIdent> TypeValue for Ty<'t, N> {
    type Value = BamlValue<'t, N>;
}

/// Represents a type with additional metadata.
pub struct TyWithMeta<T, M> {
    pub ty: T,
    pub meta: M,
}
impl<T, M> TyWithMeta<T, M> {
    pub const fn new(ty: T, meta: M) -> Self {
        Self { ty, meta }
    }
    pub const fn as_ref(&self) -> TyWithMeta<&T, &M> {
        TyWithMeta {
            ty: &self.ty,
            meta: &self.meta,
        }
    }
    pub fn map_ty<U, F: FnOnce(T) -> U>(self, f: F) -> TyWithMeta<U, M> {
        TyWithMeta {
            ty: f(self.ty),
            meta: self.meta,
        }
    }
    pub fn map_meta<U, F: FnOnce(M) -> U>(self, f: F) -> TyWithMeta<T, U> {
        TyWithMeta {
            ty: self.ty,
            meta: f(self.meta),
        }
    }
}
impl<T: Clone, M: Clone> Clone for TyWithMeta<T, M> {
    fn clone(&self) -> Self {
        Self {
            ty: self.ty.clone(),
            meta: self.meta.clone(),
        }
    }
}

pub type AnnotatedTy<'t, N> = TyWithMeta<Ty<'t, N>, TypeAnnotations<'t, N>>;
pub type AnnotatedTyRef<'t, N> = TyWithMeta<&'t Ty<'t, N>, &'t TypeAnnotations<'t, N>>;

#[derive(Clone, Copy, PartialEq, Eq, From)]
pub enum PrimitiveTy {
    Int(IntTy),
    Float(FloatTy),
    String(StringTy),
    Bool(BoolTy),
    Null(NullTy),
    #[from(forward)]
    Media(MediaTy),
}
impl TypeValue for PrimitiveTy {
    type Value = BamlPrimitive;
}
impl PrimitiveTy {
    /// Returns a `&'static` reference to an equivalent `PrimitiveTy`.
    ///
    /// Since all `PrimitiveTy` variants are unit-like (no data), the compiler
    /// promotes these const expressions to static memory. This is useful when
    /// we have a `PrimitiveTy` by value but need a `&'t` reference for trait methods.
    pub fn as_static_ref(&self) -> &'static PrimitiveTy {
        match self {
            PrimitiveTy::Int(_) => &PrimitiveTy::Int(IntTy),
            PrimitiveTy::Float(_) => &PrimitiveTy::Float(FloatTy),
            PrimitiveTy::String(_) => &PrimitiveTy::String(StringTy),
            PrimitiveTy::Bool(_) => &PrimitiveTy::Bool(BoolTy),
            PrimitiveTy::Null(_) => &PrimitiveTy::Null(NullTy),
            PrimitiveTy::Media(m) => match m {
                MediaTy::Image => &PrimitiveTy::Media(MediaTy::Image),
                MediaTy::Audio => &PrimitiveTy::Media(MediaTy::Audio),
                MediaTy::Pdf => &PrimitiveTy::Media(MediaTy::Pdf),
                MediaTy::Video => &PrimitiveTy::Media(MediaTy::Video),
            },
        }
    }
}

/// Corresponds to the BAML `int` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IntTy;
impl TypeValue for IntTy {
    type Value = BamlInt;
}

/// Corresponds to the BAML `float` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FloatTy;
impl TypeValue for FloatTy {
    type Value = BamlFloat;
}

/// Corresponds to the BAML `string` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StringTy;
impl TypeValue for StringTy {
    type Value = BamlString;
}

/// Corresponds to the BAML `bool` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BoolTy;
impl TypeValue for BoolTy {
    type Value = BamlBool;
}

/// Corresponds to the BAML `null` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NullTy;
impl TypeValue for NullTy {
    type Value = BamlNull;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MediaTy {
    Image,
    Audio,
    Pdf,
    Video,
}
impl TypeValue for MediaTy {
    type Value = BamlMedia;
}

#[derive(Clone, PartialEq, Eq, From)]
pub enum LiteralTy<'t> {
    String(StringLiteralTy<'t>),
    Int(IntLiteralTy),
    Bool(BoolLiteralTy),
}
impl TypeValue for LiteralTy<'_> {
    type Value = BamlPrimitive;
}

/// Corresponds to the BAML string literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct StringLiteralTy<'t>(pub Cow<'t, str>);
impl TypeValue for StringLiteralTy<'_> {
    type Value = BamlString;
}

/// Corresponds to the BAML int literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct IntLiteralTy(pub i64);
impl TypeValue for IntLiteralTy {
    type Value = BamlInt;
}

/// Corresponds to the BAML bool literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct BoolLiteralTy(pub bool);
impl TypeValue for BoolLiteralTy {
    type Value = BamlBool;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct ArrayTy<'t, N: TypeIdent> {
    pub ty: Box<AnnotatedTy<'t, N>>,
}
impl<'t, N: TypeIdent> TypeValue for ArrayTy<'t, N> {
    type Value = BamlArray<'t, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct MapTy<'t, N: TypeIdent> {
    pub key: Box<AnnotatedTy<'t, N>>,
    pub value: Box<AnnotatedTy<'t, N>>,
}
impl<'t, N: TypeIdent> TypeValue for MapTy<'t, N> {
    type Value = BamlMap<'t, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct ClassTy<'t, N: TypeIdent> {
    pub name: N,
    pub fields: Vec<AnnotatedField<'t, N>>,
}
impl<'t, N: TypeIdent> TypeValue for ClassTy<'t, N> {
    type Value = BamlClass<'t, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct EnumTy<'t, N: TypeIdent> {
    pub name: N,
    pub variants: Vec<AnnotatedEnumVariant<'t>>,
}
impl<'t, N: TypeIdent + 't> TypeValue for EnumTy<'t, N> {
    type Value = BamlEnum<'t, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct UnionTy<'t, N: TypeIdent> {
    pub variants: Vec<AnnotatedTy<'t, N>>,
}
impl<'t, N: TypeIdent> UnionTy<'t, N> {
    /// Returns true if any of the variants are `null`.
    ///
    /// Requires `ctx` in case we need to look up type aliases that may contain optional unions.
    pub fn is_optional(&'t self, db: &'t TypeRefDb<'t, N>) -> bool {
        self.variants.iter().any(|v| v.ty.is_optional(db))
    }
}
impl<'t, N: TypeIdent> TypeValue for UnionTy<'t, N> {
    type Value = BamlValue<'t, N>;
}

/// Represents that the value should be wrapped in a stream state enum.
#[derive(Clone)]
pub struct StreamStateTy<'t, N: TypeIdent> {
    pub value: Box<AnnotatedTy<'t, N>>,
}
impl<'t, N: TypeIdent> TypeValue for StreamStateTy<'t, N> {
    type Value = BamlStreamState<'t, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct TypeAnnotations<'t, N: TypeIdent> {
    /// Represents the behavior when streaming and incomplete.
    ///
    /// If `Some`, this is the value to use when streaming and the value is incomplete.
    /// If `None`, the partial value will be used.
    /// If `Some(never)`, the value should be excluded until done.
    /// If `Some("Loading...")`, then `"Loading..."` should be used until done.
    pub in_progress: Option<Literal<'t, N>>,
    /// Represents the behavior when completed but the value is invalid.
    pub on_error: Literal<'t, N>,

    pub asserts: Vec<Assertion<'t, N>>,
}
impl<N: TypeIdent> Default for TypeAnnotations<'_, N> {
    fn default() -> Self {
        Self {
            in_progress: None,
            on_error: Literal::Null,
            asserts: Vec::new(),
        }
    }
}
impl<'t, N: TypeIdent> TypeAnnotations<'t, N> {
    pub fn check_asserts(
        &self,
        value: &BamlValue<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<bool, ParsingError> {
        for assert in self.asserts.iter() {
            if !assert.evaluate(value, ctx)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct AnnotatedField<'t, N: TypeIdent> {
    pub name: Cow<'t, str>,
    pub ty: AnnotatedTy<'t, N>,
    /// If the parent object is incomplete and this field has not yet been started,
    /// use this value instead. If it is `never`, then the field is excluded.
    pub before_started: Literal<'t, N>,
    /// If the parent object is complete and this field is missing, use this value instead.
    /// If it is `never`, then the field is excluded.
    pub missing: Literal<'t, N>,

    pub aliases: Vec<Cow<'t, str>>,
}

#[derive(Clone)]
pub struct AnnotatedEnumVariant<'t> {
    pub name: Cow<'t, str>,
    pub aliases: Vec<Cow<'t, str>>,
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
///
/// Used in attributes like `@sap.missing(...)` and `@sap.default(...)`
#[derive(Clone)]
pub enum Literal<'t, N: TypeIdent> {
    /// the `never` bottom type.
    Never,
    /// the `null` type
    Null,
    Int(i64),
    Float(f64),
    String(Cow<'t, str>),
    Bool(bool),
    Array(Vec<Literal<'t, N>>),
    Object {
        name: &'t N,
        data: IndexMap<Cow<'t, str>, Literal<'t, N>>,
    },
    Map(IndexMap<Cow<'t, str>, Literal<'t, N>>),
    EnumVariant {
        enum_name: &'t N,
        variant_name: Cow<'t, str>,
    },
}
