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

/// An identifier for a type. Used to look up a type in a [`TypeRefDb`].
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

    /// Unwraps a [`Ty`] into a [`TyResolvedRef`].
    ///
    /// If the type is already resolved, returns a reference to it.
    /// If the type is unresolved, looks up the type and returns a reference to it.
    ///
    /// # Errors
    /// If the type is unresolved and not found in the database, returns the identifier.
    pub fn resolve(&'t self, ty: &'t Ty<'t, N>) -> Result<TyResolvedRef<'t, N>, &'t N> {
        match ty {
            Ty::Resolved(ty) => Ok(ty.as_ref()),
            Ty::ResolvedRef(ty) => Ok(ty.clone()),
            Ty::Unresolved(ident) => self.types.get(ident).map(TyResolved::as_ref).ok_or(ident),
        }
    }

    pub fn resolved_from_ident(&'t self, ident: &'t N) -> Option<TyResolvedRef<'t, N>> {
        self.types.get(ident).map(TyResolved::as_ref)
    }

    /// Like [`TypeRefDb::resolve`], but maps the result to keep the type annotations.
    pub fn resolve_with_meta(
        &'t self,
        ty: TyWithMeta<&'t Ty<'t, N>, &'t TypeAnnotations<'t, N>>,
    ) -> Result<TyWithMeta<TyResolvedRef<'t, N>, &'t TypeAnnotations<'t, N>>, &'t N> {
        self.resolve(ty.ty).map(|res| TyWithMeta::new(res, ty.meta))
    }
}

/// A trait that associates a BAML value type with a SAP model type.
/// Should be implemented for SAP model types.
pub trait TypeValue<'s, 'v, 't>
where
    's: 'v,
{
    /// The BAML value type associated with this SAP model type.
    type Value;
}

/// Contains a SAP model type, generally part of the one passed into the deserializer.
///
/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone, From)]
pub enum TyResolved<'t, N: TypeIdent> {
    Int(IntTy),
    Float(FloatTy),
    String(StringTy),
    Bool(BoolTy),
    Null(NullTy),
    Media(MediaTy),
    LiteralString(StringLiteralTy<'t>),
    LiteralInt(IntLiteralTy),
    LiteralBool(BoolLiteralTy),
    Array(ArrayTy<'t, N>),
    Map(MapTy<'t, N>),
    Class(ClassTy<'t, N>),
    Enum(EnumTy<'t, N>),
    Union(UnionTy<'t, N>),
    /// A type that tells you if it is completed or not.
    StreamState(StreamStateTy<'t, N>),
}
impl<'t, N: TypeIdent> From<PrimitiveTy> for TyResolved<'t, N> {
    fn from(p: PrimitiveTy) -> Self {
        match p {
            PrimitiveTy::Int(v) => TyResolved::Int(v),
            PrimitiveTy::Float(v) => TyResolved::Float(v),
            PrimitiveTy::String(v) => TyResolved::String(v),
            PrimitiveTy::Bool(v) => TyResolved::Bool(v),
            PrimitiveTy::Null(v) => TyResolved::Null(v),
            PrimitiveTy::Media(v) => TyResolved::Media(v),
        }
    }
}
impl<'t, N: TypeIdent> From<LiteralTy<'t>> for TyResolved<'t, N> {
    fn from(l: LiteralTy<'t>) -> Self {
        match l {
            LiteralTy::String(v) => TyResolved::LiteralString(v),
            LiteralTy::Int(v) => TyResolved::LiteralInt(v),
            LiteralTy::Bool(v) => TyResolved::LiteralBool(v),
        }
    }
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for TyResolved<'t, N>
where
    's: 'v,
{
    type Value = BamlValue<'s, 'v, 't, N>;
}
impl<'t, N: TypeIdent> TyResolved<'t, N> {
    pub fn as_ref(&'t self) -> TyResolvedRef<'t, N> {
        match self {
            TyResolved::Int(v) => TyResolvedRef::Int(*v),
            TyResolved::Float(v) => TyResolvedRef::Float(*v),
            TyResolved::String(v) => TyResolvedRef::String(*v),
            TyResolved::Bool(v) => TyResolvedRef::Bool(*v),
            TyResolved::Null(v) => TyResolvedRef::Null(*v),
            TyResolved::Media(v) => TyResolvedRef::Media(*v),
            TyResolved::LiteralString(v) => TyResolvedRef::LiteralString(v),
            TyResolved::LiteralInt(v) => TyResolvedRef::LiteralInt(v),
            TyResolved::LiteralBool(v) => TyResolvedRef::LiteralBool(v),
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
    Int(IntTy),
    Float(FloatTy),
    String(StringTy),
    Bool(BoolTy),
    Null(NullTy),
    Media(MediaTy),
    LiteralString(&'t StringLiteralTy<'t>),
    LiteralInt(&'t IntLiteralTy),
    LiteralBool(&'t BoolLiteralTy),
    Array(&'t ArrayTy<'t, N>),
    Map(&'t MapTy<'t, N>),
    Class(&'t ClassTy<'t, N>),
    Enum(&'t EnumTy<'t, N>),
    Union(&'t UnionTy<'t, N>),
    /// A type that tells you if it is completed or not.
    StreamState(&'t StreamStateTy<'t, N>),
}
impl<'t, N: TypeIdent> From<PrimitiveTy> for TyResolvedRef<'t, N> {
    fn from(p: PrimitiveTy) -> Self {
        match p {
            PrimitiveTy::Int(v) => TyResolvedRef::Int(v),
            PrimitiveTy::Float(v) => TyResolvedRef::Float(v),
            PrimitiveTy::String(v) => TyResolvedRef::String(v),
            PrimitiveTy::Bool(v) => TyResolvedRef::Bool(v),
            PrimitiveTy::Null(v) => TyResolvedRef::Null(v),
            PrimitiveTy::Media(v) => TyResolvedRef::Media(v),
        }
    }
}
impl<'t, N: TypeIdent> From<&'t LiteralTy<'t>> for TyResolvedRef<'t, N> {
    fn from(l: &'t LiteralTy<'t>) -> Self {
        match l {
            LiteralTy::String(v) => TyResolvedRef::LiteralString(v),
            LiteralTy::Int(v) => TyResolvedRef::LiteralInt(v),
            LiteralTy::Bool(v) => TyResolvedRef::LiteralBool(v),
        }
    }
}
// Manual Copy impl because derive(Copy) adds an unnecessary `N: Copy` bound.
// All variants are either Copy-by-value (primitives) or references (&'t T),
// so Copy is valid regardless of N.
impl<'t, N: TypeIdent> Copy for TyResolvedRef<'t, N> {}
impl<'t, N: TypeIdent> TyResolvedRef<'t, N> {
    /// Returns true if the type may be `null`.
    ///
    /// Requires `db` in case we need to look up type aliases that may contain optional unions.
    pub fn is_optional(&self, db: &TypeRefDb<'t, N>) -> bool {
        match self {
            TyResolvedRef::Null(..) => true,
            TyResolvedRef::Union(u) => u.is_optional(db),
            TyResolvedRef::StreamState(s) => s.value.ty.is_optional(db),
            _ => false,
        }
    }
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for TyResolvedRef<'t, N>
where
    's: 'v,
{
    type Value = BamlValue<'s, 'v, 't, N>;
}

#[derive(Clone, From)]
pub enum Ty<'t, N: TypeIdent> {
    #[from(TyResolved<'t, N>, LiteralTy<'t>, StringLiteralTy<'t>, IntLiteralTy, BoolLiteralTy, ArrayTy<'t, N>, MapTy<'t, N>, ClassTy<'t, N>, EnumTy<'t, N>, UnionTy<'t, N>)]
    Resolved(TyResolved<'t, N>),
    #[from(TyResolvedRef<'t, N>, PrimitiveTy, IntTy, FloatTy, StringTy, BoolTy, NullTy, MediaTy, &'t LiteralTy<'t>, &'t StringLiteralTy<'t>, &'t IntLiteralTy, &'t BoolLiteralTy, &'t ArrayTy<'t, N>, &'t MapTy<'t, N>, &'t ClassTy<'t, N>, &'t EnumTy<'t, N>, &'t UnionTy<'t, N>)]
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
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for Ty<'t, N>
where
    's: 'v,
{
    type Value = BamlValue<'s, 'v, 't, N>;
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
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for PrimitiveTy
where
    's: 'v,
{
    type Value = BamlPrimitive<'s>;
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
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for IntTy
where
    's: 'v,
{
    type Value = BamlInt;
}

/// Corresponds to the BAML `float` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FloatTy;
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for FloatTy
where
    's: 'v,
{
    type Value = BamlFloat;
}

/// Corresponds to the BAML `string` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StringTy;
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for StringTy
where
    's: 'v,
{
    type Value = BamlString<'s>;
}

/// Corresponds to the BAML `bool` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BoolTy;
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for BoolTy
where
    's: 'v,
{
    type Value = BamlBool;
}

/// Corresponds to the BAML `null` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NullTy;
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for NullTy
where
    's: 'v,
{
    type Value = BamlNull;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MediaTy {
    Image,
    Audio,
    Pdf,
    Video,
}
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for MediaTy
where
    's: 'v,
{
    type Value = BamlMedia;
}

#[derive(Clone, PartialEq, Eq, From)]
pub enum LiteralTy<'t> {
    String(StringLiteralTy<'t>),
    Int(IntLiteralTy),
    Bool(BoolLiteralTy),
}
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for LiteralTy<'_>
where
    's: 'v,
{
    type Value = BamlPrimitive<'t>;
}
impl<'t> From<&'t str> for LiteralTy<'t> {
    fn from(s: &'t str) -> Self {
        LiteralTy::String(StringLiteralTy(Cow::Borrowed(s.as_ref())))
    }
}
impl From<i64> for LiteralTy<'static> {
    fn from(i: i64) -> Self {
        LiteralTy::Int(IntLiteralTy(i))
    }
}
impl From<bool> for LiteralTy<'static> {
    fn from(b: bool) -> Self {
        LiteralTy::Bool(BoolLiteralTy(b))
    }
}

/// Corresponds to the BAML string literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct StringLiteralTy<'t>(pub Cow<'t, str>);
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for StringLiteralTy<'t>
where
    's: 'v,
{
    type Value = BamlString<'t>;
}

/// Corresponds to the BAML int literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct IntLiteralTy(pub i64);
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for IntLiteralTy
where
    's: 'v,
{
    type Value = BamlInt;
}

/// Corresponds to the BAML bool literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct BoolLiteralTy(pub bool);
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for BoolLiteralTy
where
    's: 'v,
{
    type Value = BamlBool;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct ArrayTy<'t, N: TypeIdent> {
    pub ty: Box<AnnotatedTy<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for ArrayTy<'t, N>
where
    's: 'v,
{
    type Value = BamlArray<'s, 'v, 't, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct MapTy<'t, N: TypeIdent> {
    pub key: Box<AnnotatedTy<'t, N>>,
    pub value: Box<AnnotatedTy<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for MapTy<'t, N>
where
    's: 'v,
{
    type Value = BamlMap<'s, 'v, 't, N>;
}
impl<'t, N: TypeIdent> MapTy<'t, N> {
    pub fn new(key: AnnotatedTy<'t, N>, value: AnnotatedTy<'t, N>) -> Self {
        Self {
            key: Box::new(key),
            value: Box::new(value),
        }
    }
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct ClassTy<'t, N: TypeIdent> {
    pub name: N,
    pub fields: Vec<AnnotatedField<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for ClassTy<'t, N>
where
    's: 'v,
{
    type Value = BamlClass<'s, 'v, 't, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct EnumTy<'t, N: TypeIdent> {
    pub name: N,
    pub variants: Vec<AnnotatedEnumVariant<'t>>,
}
impl<'s, 'v, 't, N: TypeIdent + 't> TypeValue<'s, 'v, 't> for EnumTy<'t, N>
where
    's: 'v,
{
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
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for UnionTy<'t, N>
where
    's: 'v,
{
    type Value = BamlValue<'s, 'v, 't, N>;
}

/// Represents that the value should be wrapped in a stream state enum.
#[derive(Clone)]
pub struct StreamStateTy<'t, N: TypeIdent> {
    pub value: Box<AnnotatedTy<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for StreamStateTy<'t, N>
where
    's: 'v,
{
    type Value = BamlStreamState<'s, 'v, 't, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct TypeAnnotations<'t, N: TypeIdent> {
    /// Represents the behavior when streaming and incomplete.
    ///
    /// - If `None`, the partial value will be used.
    /// - If `Some(never)`, the value should be excluded until done.
    /// - If `Some(<value>)`, this is the value to use when streaming and the value is incomplete.
    ///
    /// Example:
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
            on_error: Literal::Never,
            asserts: Vec::new(),
        }
    }
}
impl<'t, N: TypeIdent> TypeAnnotations<'t, N> {
    /// Runs [`TypeAnnotations::check_asserts`] but also gives an error if the assertions fail
    pub fn expect_asserts<'s, 'v>(
        &self,
        value: &BamlValue<'s, 'v, 't, N>,
        ctx: &ParsingContext<'_, '_, 't, N>,
    ) -> Result<(), ParsingError> {
        match self.check_asserts(value, ctx) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ctx.error_assertion_failure()),
            Err(err) => Err(err),
        }
    }

    pub fn check_asserts<'s, 'v>(
        &self,
        value: &BamlValue<'s, 'v, 't, N>,
        ctx: &ParsingContext<'_, '_, 't, N>,
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
    /// use this value instead. If it is `never` then this causes an error.
    pub before_started: Literal<'t, N>,
    /// If the parent object is complete and this field is missing, use this value instead.
    /// If it is `never` then this causes an error.
    pub missing: Literal<'t, N>,

    pub aliases: Vec<Cow<'t, str>>,
}
impl<'t, N: TypeIdent> AnnotatedField<'t, N> {
    pub fn key_matches(&self, key: &str) -> bool {
        self.name == key || self.aliases.iter().any(|a| a == key)
    }
}

#[derive(Clone)]
pub struct AnnotatedEnumVariant<'t> {
    pub name: Cow<'t, str>,
    pub aliases: Vec<Cow<'t, str>>,
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
///
/// Used in attributes like `@sap.in_progress(...)` and `@sap.class_completed_field_missing(...)`
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
