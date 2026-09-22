//! The parser-facing model of BAML types.
//!
//! Streaming follows [BEP-075: Simplified Streaming](https://beps.boundaryml.com/beps/75):
//! attributes belong to declarations (`@stream.done` and `@stream.must_exist` on class
//! fields, `@@stream.done` on classes) and never to type nodes, and every class field has a
//! default ([`TypeRefDb::field_default`]) that stands in while the input has not reached the
//! field.

mod convert;
mod from_literal;
mod test_macros;
mod type_name;

use std::{borrow::Cow, fmt, fmt::Display};

pub use convert::*;
use derive_more::From;
pub use from_literal::FromLiteral;
use indexmap::IndexMap;
pub use type_name::TypeName;

use crate::baml_value::{
    BamlArray, BamlBigint, BamlBool, BamlClass, BamlEnum, BamlFloat, BamlInt, BamlMap, BamlMedia,
    BamlNull, BamlPrimitive, BamlStreamState, BamlString, BamlValue,
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
    /// Types in the database are stored by name.
    types: IndexMap<N, TyResolved<'t, N>>,
}

impl<N: TypeIdent> Default for TypeRefDb<'_, N> {
    fn default() -> Self {
        Self {
            types: IndexMap::new(),
        }
    }
}
impl<'t, N: TypeIdent> TypeRefDb<'t, N> {
    /// An empty database.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// A database of the given named types.
    pub fn from_types(types: IndexMap<N, TyResolved<'t, N>>) -> Self {
        Self { types }
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
            Ty::ResolvedRef(ty) => Ok(*ty),
            Ty::Unresolved(ident) => self.types.get(ident).map(TyResolved::as_ref).ok_or(ident),
        }
    }

    /// Resolve a named SAP type without first constructing a borrowed
    /// [`Ty::Unresolved`] wrapper.
    pub fn resolve_name(&'t self, ident: &N) -> Option<TyResolvedRef<'t, N>> {
        self.types.get(ident).map(TyResolved::as_ref)
    }

    /// What a class field holds while its object is incomplete and no (partial) value has
    /// arrived: the declaration's [`AnnotatedField::default`] if it pins one, otherwise
    /// [`TypeRefDb::partial_default`] of the field's type.
    ///
    /// # Errors
    /// The field's type names something the database does not contain.
    pub fn field_default(
        &'t self,
        field: &'t AnnotatedField<'t, N>,
    ) -> Result<Cow<'t, DefaultValue<N>>, &'t N> {
        match &field.default {
            Some(default) => Ok(Cow::Borrowed(default)),
            None => self.partial_default(&field.ty).map(Cow::Owned),
        }
    }

    /// BEP-075's field-default table over `ty`: nullable types → `null`, literals → the
    /// literal, `string` → `""`, arrays and maps → empty, a one-variant enum → its variant, a
    /// class → all of its fields' defaults, and everything else (numbers, `bool`, media, other
    /// enums, non-nullable unions, a class with a `never` field or containing itself) →
    /// [`DefaultValue::Never`].
    ///
    /// # Errors
    /// `ty` names something the database does not contain.
    pub fn partial_default(&'t self, ty: &'t Ty<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        DefaultDeriver {
            db: self,
            stack: Vec::new(),
        }
        .partial(ty)
    }

    /// What a complete object may omit: `null` for nullable types and an empty container for
    /// arrays and maps. Anything else is required ([`DefaultValue::Never`]).
    ///
    /// # Errors
    /// `ty` names something the database does not contain.
    pub fn missing_default(&'t self, ty: &'t Ty<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        DefaultDeriver {
            db: self,
            stack: Vec::new(),
        }
        .missing(ty)
    }
}

/// Derives field defaults over a database (BEP-075's field-default table).
///
/// `stack` holds the classes and aliases currently being derived: a type that needs its own
/// default to be inhabited has none.
struct DefaultDeriver<'t, N: TypeIdent> {
    db: &'t TypeRefDb<'t, N>,
    stack: Vec<N>,
}

impl<'t, N: TypeIdent> DefaultDeriver<'t, N> {
    /// Runs `derive` on the resolution of `ty`, guarding named entries against cycles.
    fn resolved(
        &mut self,
        ty: &'t Ty<'t, N>,
        derive: fn(&mut Self, TyResolvedRef<'t, N>) -> Result<DefaultValue<N>, &'t N>,
    ) -> Result<DefaultValue<N>, &'t N> {
        match ty {
            Ty::Resolved(resolved) => derive(self, resolved.as_ref()),
            Ty::ResolvedRef(resolved) => derive(self, *resolved),
            Ty::Unresolved(name) => {
                let Some(resolved) = self.db.types.get(name) else {
                    return Err(name);
                };
                if let TyResolved::Class(class) = resolved {
                    // Classes guard themselves by declaration name.
                    return derive(self, TyResolvedRef::Class(class));
                }
                if self.stack.contains(name) {
                    return Ok(DefaultValue::Never);
                }
                self.stack.push(name.clone());
                let default = derive(self, resolved.as_ref());
                self.stack.pop();
                default
            }
        }
    }

    fn partial(&mut self, ty: &'t Ty<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        self.resolved(ty, Self::partial_resolved)
    }

    fn partial_resolved(&mut self, ty: TyResolvedRef<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        Ok(match ty {
            TyResolvedRef::Null(_) => DefaultValue::Null,
            TyResolvedRef::Int(_)
            | TyResolvedRef::Bigint(_)
            | TyResolvedRef::Float(_)
            | TyResolvedRef::Bool(_)
            | TyResolvedRef::Media(_) => DefaultValue::Never,
            TyResolvedRef::String(_) => DefaultValue::String(String::new()),
            TyResolvedRef::LiteralString(lit) => DefaultValue::String(lit.0.to_string()),
            TyResolvedRef::LiteralInt(lit) => DefaultValue::Int(lit.0),
            TyResolvedRef::LiteralBigint(lit) => DefaultValue::Bigint(lit.0.clone()),
            TyResolvedRef::LiteralBool(lit) => DefaultValue::Bool(lit.0),
            TyResolvedRef::Array(_) => DefaultValue::EmptyArray,
            TyResolvedRef::Map(_) => DefaultValue::EmptyMap,
            // A one-variant enum is the same type as its variant literal, so it defaults
            // the same way.
            TyResolvedRef::Enum(e) => match e.variants.as_slice() {
                [only] => DefaultValue::EnumVariant {
                    enum_name: e.name.clone(),
                    variant_name: only.name.to_string(),
                },
                _ => DefaultValue::Never,
            },
            TyResolvedRef::EnumVariant(ev) => DefaultValue::EnumVariant {
                enum_name: ev.name.clone(),
                variant_name: ev.value.name.to_string(),
            },
            TyResolvedRef::Union(u) => self.union_default(u)?,
            TyResolvedRef::Class(c) => self.class_default(c)?,
            TyResolvedRef::StreamState(s) => match self.partial(&s.value)? {
                DefaultValue::Never => DefaultValue::Never,
                inner => DefaultValue::StreamStatePending(Box::new(inner)),
            },
        })
    }

    /// `null` when the union is nullable, otherwise `never`.
    fn union_default(&mut self, union: &'t UnionTy<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        for variant in &union.variants {
            if self.partial(variant)? == DefaultValue::Null {
                return Ok(DefaultValue::Null);
            }
        }
        Ok(DefaultValue::Never)
    }

    /// Every field's default, or `never` if any field has none (or the class contains itself).
    fn class_default(&mut self, class: &'t ClassTy<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        if self.stack.contains(&class.name) {
            return Ok(DefaultValue::Never);
        }
        self.stack.push(class.name.clone());
        let mut fields = IndexMap::with_capacity(class.fields.len());
        let mut default = DefaultValue::Never;
        for field in &class.fields {
            let field_default = match &field.default {
                Some(pinned) => pinned.clone(),
                None => self.partial(&field.ty)?,
            };
            if field_default == DefaultValue::Never {
                fields.clear();
                break;
            }
            fields.insert(field.name.to_string(), field_default);
        }
        if fields.len() == class.fields.len() {
            default = DefaultValue::Object {
                name: class.name.clone(),
                fields,
            };
        }
        self.stack.pop();
        Ok(default)
    }

    fn missing(&mut self, ty: &'t Ty<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        self.resolved(ty, Self::missing_resolved)
    }

    fn missing_resolved(&mut self, ty: TyResolvedRef<'t, N>) -> Result<DefaultValue<N>, &'t N> {
        Ok(match ty {
            TyResolvedRef::Null(_) => DefaultValue::Null,
            TyResolvedRef::Union(u) => self.union_default(u)?,
            TyResolvedRef::Array(_) => DefaultValue::EmptyArray,
            TyResolvedRef::Map(_) => DefaultValue::EmptyMap,
            TyResolvedRef::StreamState(s) => match self.missing(&s.value)? {
                DefaultValue::Never => DefaultValue::Never,
                inner => DefaultValue::StreamStatePending(Box::new(inner)),
            },
            TyResolvedRef::Int(_)
            | TyResolvedRef::Bigint(_)
            | TyResolvedRef::Float(_)
            | TyResolvedRef::String(_)
            | TyResolvedRef::Bool(_)
            | TyResolvedRef::Media(_)
            | TyResolvedRef::LiteralString(_)
            | TyResolvedRef::LiteralInt(_)
            | TyResolvedRef::LiteralBigint(_)
            | TyResolvedRef::LiteralBool(_)
            | TyResolvedRef::Class(_)
            | TyResolvedRef::Enum(_)
            | TyResolvedRef::EnumVariant(_) => DefaultValue::Never,
        })
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
#[derive(Clone, From, PartialEq)]
pub enum TyResolved<'t, N: TypeIdent> {
    Int(IntTy),
    Bigint(BigintTy),
    Float(FloatTy),
    String(StringTy),
    Bool(BoolTy),
    Null(NullTy),
    Media(MediaTy),
    LiteralString(StringLiteralTy<'t>),
    LiteralInt(IntLiteralTy),
    LiteralBigint(BigintLiteralTy),
    LiteralBool(BoolLiteralTy),
    Array(ArrayTy<'t, N>),
    Map(MapTy<'t, N>),
    Class(ClassTy<'t, N>),
    Enum(EnumTy<'t, N>),
    EnumVariant(EnumVariantTy<'t, N>),
    Union(UnionTy<'t, N>),
    /// A type that tells you if it is completed or not.
    StreamState(StreamStateTy<'t, N>),
}
impl<N: TypeIdent> From<PrimitiveTy> for TyResolved<'_, N> {
    fn from(p: PrimitiveTy) -> Self {
        match p {
            PrimitiveTy::Int(v) => TyResolved::Int(v),
            PrimitiveTy::Bigint(v) => TyResolved::Bigint(v),
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
            LiteralTy::Bigint(v) => TyResolved::LiteralBigint(v),
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
            TyResolved::Bigint(v) => TyResolvedRef::Bigint(*v),
            TyResolved::Float(v) => TyResolvedRef::Float(*v),
            TyResolved::String(v) => TyResolvedRef::String(*v),
            TyResolved::Bool(v) => TyResolvedRef::Bool(*v),
            TyResolved::Null(v) => TyResolvedRef::Null(*v),
            TyResolved::Media(v) => TyResolvedRef::Media(*v),
            TyResolved::LiteralString(v) => TyResolvedRef::LiteralString(v),
            TyResolved::LiteralInt(v) => TyResolvedRef::LiteralInt(v),
            TyResolved::LiteralBigint(v) => TyResolvedRef::LiteralBigint(v),
            TyResolved::LiteralBool(v) => TyResolvedRef::LiteralBool(v),
            TyResolved::Array(a) => TyResolvedRef::Array(a),
            TyResolved::Map(m) => TyResolvedRef::Map(m),
            TyResolved::Class(c) => TyResolvedRef::Class(c),
            TyResolved::Enum(e) => TyResolvedRef::Enum(e),
            TyResolved::EnumVariant(e) => TyResolvedRef::EnumVariant(e),
            TyResolved::Union(u) => TyResolvedRef::Union(u),
            TyResolved::StreamState(s) => TyResolvedRef::StreamState(s),
        }
    }
}

/// [`TyResolved`] but with a reference to the SAP model type.
///
/// This is a reference to a SAP model type, generally part of the one passed into the deserializer.
/// At the top-level, any type references (identified by `N`) are resolved in this type.
#[derive(Clone, From, PartialEq)]
pub enum TyResolvedRef<'t, N: TypeIdent> {
    Int(IntTy),
    Bigint(BigintTy),
    Float(FloatTy),
    String(StringTy),
    Bool(BoolTy),
    Null(NullTy),
    Media(MediaTy),
    LiteralString(&'t StringLiteralTy<'t>),
    LiteralInt(&'t IntLiteralTy),
    LiteralBigint(&'t BigintLiteralTy),
    LiteralBool(&'t BoolLiteralTy),
    Array(&'t ArrayTy<'t, N>),
    Map(&'t MapTy<'t, N>),
    Class(&'t ClassTy<'t, N>),
    Enum(&'t EnumTy<'t, N>),
    EnumVariant(&'t EnumVariantTy<'t, N>),
    Union(&'t UnionTy<'t, N>),
    /// A type that tells you if it is completed or not.
    StreamState(&'t StreamStateTy<'t, N>),
}
impl<N: TypeIdent> From<PrimitiveTy> for TyResolvedRef<'_, N> {
    fn from(p: PrimitiveTy) -> Self {
        match p {
            PrimitiveTy::Int(v) => TyResolvedRef::Int(v),
            PrimitiveTy::Bigint(v) => TyResolvedRef::Bigint(v),
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
            LiteralTy::Bigint(v) => TyResolvedRef::LiteralBigint(v),
            LiteralTy::Bool(v) => TyResolvedRef::LiteralBool(v),
        }
    }
}
// Manual Copy impl because derive(Copy) adds an unnecessary `N: Copy` bound.
// All variants are either Copy-by-value (primitives) or references (&'t T),
// so Copy is valid regardless of N.
impl<N: TypeIdent> Copy for TyResolvedRef<'_, N> {}
impl<'t, N: TypeIdent> TyResolvedRef<'t, N> {
    /// Returns true if the type may be `null`.
    ///
    /// Requires `db` in case we need to look up type aliases that may contain optional unions.
    pub fn is_optional(&self, db: &TypeRefDb<'t, N>) -> bool {
        match self {
            TyResolvedRef::Null(..) => true,
            TyResolvedRef::Union(u) => u.is_optional(db),
            TyResolvedRef::StreamState(s) => s.value.is_optional(db),
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
    #[from(TyResolved<'t, N>, LiteralTy<'t>, StringLiteralTy<'t>, IntLiteralTy, BoolLiteralTy, ArrayTy<'t, N>, MapTy<'t, N>, ClassTy<'t, N>, EnumTy<'t, N>, UnionTy<'t, N>, StreamStateTy<'t, N>)]
    Resolved(TyResolved<'t, N>),
    #[from(TyResolvedRef<'t, N>, PrimitiveTy, IntTy, FloatTy, StringTy, BoolTy, NullTy, MediaTy, &'t LiteralTy<'t>, &'t StringLiteralTy<'t>, &'t IntLiteralTy, &'t BoolLiteralTy, &'t ArrayTy<'t, N>, &'t MapTy<'t, N>, &'t ClassTy<'t, N>, &'t EnumTy<'t, N>, &'t UnionTy<'t, N>, &'t StreamStateTy<'t, N>)]
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
    pub fn is_optional(&self, db: &'t TypeRefDb<'t, N>) -> bool {
        db.resolve(self).is_ok_and(|ty| ty.is_optional(db))
    }
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for Ty<'t, N>
where
    's: 'v,
{
    type Value = BamlValue<'s, 'v, 't, N>;
}
impl<N: TypeIdent> PartialEq for Ty<'_, N> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Ty::Resolved(a), Ty::Resolved(b)) => a == b,
            (Ty::ResolvedRef(a), Ty::ResolvedRef(b)) => a == b,
            (Ty::Unresolved(a), Ty::Unresolved(b)) => a == b,
            (Ty::Resolved(a), Ty::ResolvedRef(b)) => a.as_ref() == *b,
            (Ty::ResolvedRef(a), Ty::Resolved(b)) => *a == b.as_ref(),
            (Ty::Unresolved(a), Ty::Resolved(TyResolved::Class(b))) => *a == b.name,
            (Ty::Unresolved(a), Ty::Resolved(TyResolved::Enum(b))) => *a == b.name,
            (Ty::Resolved(TyResolved::Class(a)), Ty::Unresolved(b)) => a.name == *b,
            (Ty::Resolved(TyResolved::Enum(a)), Ty::Unresolved(b)) => a.name == *b,
            (Ty::Unresolved(a), Ty::ResolvedRef(TyResolvedRef::Class(b))) => *a == b.name,
            (Ty::Unresolved(a), Ty::ResolvedRef(TyResolvedRef::Enum(b))) => *a == b.name,
            (Ty::ResolvedRef(TyResolvedRef::Class(a)), Ty::Unresolved(b)) => a.name == *b,
            (Ty::ResolvedRef(TyResolvedRef::Enum(a)), Ty::Unresolved(b)) => a.name == *b,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, From)]
pub enum PrimitiveTy {
    Int(IntTy),
    Bigint(BigintTy),
    Float(FloatTy),
    String(StringTy),
    Bool(BoolTy),
    Null(NullTy),
    #[from(forward)]
    Media(MediaTy),
}
impl<'s, 'v> TypeValue<'s, 'v, '_> for PrimitiveTy
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
            PrimitiveTy::Bigint(_) => &PrimitiveTy::Bigint(BigintTy),
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
impl<'s, 'v> TypeValue<'s, 'v, '_> for IntTy
where
    's: 'v,
{
    type Value = BamlInt;
}

/// Corresponds to the BAML `bigint` type — arbitrary-precision integer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BigintTy;
impl<'s, 'v> TypeValue<'s, 'v, '_> for BigintTy
where
    's: 'v,
{
    type Value = BamlBigint;
}

/// Corresponds to the BAML `float` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FloatTy;
impl<'s, 'v> TypeValue<'s, 'v, '_> for FloatTy
where
    's: 'v,
{
    type Value = BamlFloat;
}

/// Corresponds to the BAML `string` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StringTy;
impl<'s, 'v> TypeValue<'s, 'v, '_> for StringTy
where
    's: 'v,
{
    type Value = BamlString<'s>;
}

/// Corresponds to the BAML `bool` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BoolTy;
impl<'s, 'v> TypeValue<'s, 'v, '_> for BoolTy
where
    's: 'v,
{
    type Value = BamlBool;
}

/// Corresponds to the BAML `null` type.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NullTy;
impl<'s, 'v> TypeValue<'s, 'v, '_> for NullTy
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
impl<'s, 'v> TypeValue<'s, 'v, '_> for MediaTy
where
    's: 'v,
{
    type Value = BamlMedia;
}

#[derive(Clone, PartialEq, Eq, From)]
pub enum LiteralTy<'t> {
    String(StringLiteralTy<'t>),
    Int(IntLiteralTy),
    Bigint(BigintLiteralTy),
    Bool(BoolLiteralTy),
}
impl<'s, 'v, 't> TypeValue<'s, 'v, 't> for LiteralTy<'t>
where
    's: 'v,
{
    type Value = BamlPrimitive<'t>;
}
impl<'t> From<&'t str> for LiteralTy<'t> {
    fn from(s: &'t str) -> Self {
        LiteralTy::String(StringLiteralTy(Cow::Borrowed(s)))
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
impl<'s, 'v> TypeValue<'s, 'v, '_> for IntLiteralTy
where
    's: 'v,
{
    type Value = BamlInt;
}

/// Corresponds to the BAML bigint literal type — a fixed arbitrary-precision integer value.
#[derive(Clone, PartialEq, Eq)]
pub struct BigintLiteralTy(pub num_bigint::BigInt);
impl<'s, 'v> TypeValue<'s, 'v, '_> for BigintLiteralTy
where
    's: 'v,
{
    type Value = BamlBigint;
}

/// Corresponds to the BAML bool literal type.
#[derive(Clone, PartialEq, Eq)]
pub struct BoolLiteralTy(pub bool);
impl<'s, 'v> TypeValue<'s, 'v, '_> for BoolLiteralTy
where
    's: 'v,
{
    type Value = BamlBool;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone, PartialEq)]
pub struct ArrayTy<'t, N: TypeIdent> {
    pub ty: Box<Ty<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for ArrayTy<'t, N>
where
    's: 'v,
{
    type Value = BamlArray<'s, 'v, 't, N>;
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone, PartialEq)]
pub struct MapTy<'t, N: TypeIdent> {
    pub key: Box<Ty<'t, N>>,
    pub value: Box<Ty<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for MapTy<'t, N>
where
    's: 'v,
{
    type Value = BamlMap<'s, 'v, 't, N>;
}
impl<'t, N: TypeIdent> MapTy<'t, N> {
    pub fn new(key: Ty<'t, N>, value: Ty<'t, N>) -> Self {
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
    /// Not used in determining equality, as classes should be identical if their names are the same.
    pub fields: Vec<AnnotatedField<'t, N>>,
    /// `@@stream.done`: an incomplete object never parses as this class.
    pub stream_done: bool,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for ClassTy<'t, N>
where
    's: 'v,
{
    type Value = BamlClass<'s, 'v, 't, N>;
}
impl<N: TypeIdent> PartialEq for ClassTy<'_, N> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct EnumTy<'t, N: TypeIdent> {
    pub name: N,
    /// Not used in determining equality, as enums should be identical if their names are the same.
    pub variants: Vec<AnnotatedEnumVariant<'t>>,
}
impl<'s, 'v, 't, N: TypeIdent + 't> TypeValue<'s, 'v, 't> for EnumTy<'t, N>
where
    's: 'v,
{
    type Value = BamlEnum<'t, N>;
}
impl<N: TypeIdent> PartialEq for EnumTy<'_, N> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct EnumVariantTy<'t, N: TypeIdent> {
    pub name: N,
    pub value: AnnotatedEnumVariant<'t>,
}
impl<'s, 'v, 't, N: TypeIdent + 't> TypeValue<'s, 'v, 't> for EnumVariantTy<'t, N>
where
    's: 'v,
{
    type Value = BamlEnum<'t, N>;
}
impl<N: TypeIdent> PartialEq for EnumVariantTy<'_, N> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value.name == other.value.name
    }
}

/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone, PartialEq)]
pub struct UnionTy<'t, N: TypeIdent> {
    pub variants: Vec<Ty<'t, N>>,
}
impl<'t, N: TypeIdent> UnionTy<'t, N> {
    /// Returns true if any of the variants are `null`.
    ///
    /// Requires `ctx` in case we need to look up type aliases that may contain optional unions.
    pub fn is_optional(&self, db: &'t TypeRefDb<'t, N>) -> bool {
        self.variants.iter().any(|v| v.is_optional(db))
    }
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for UnionTy<'t, N>
where
    's: 'v,
{
    type Value = BamlValue<'s, 'v, 't, N>;
}

/// Represents that the value should be wrapped in a stream state enum.
#[derive(Clone, PartialEq)]
pub struct StreamStateTy<'t, N: TypeIdent> {
    pub value: Box<Ty<'t, N>>,
}
impl<'s, 'v, 't, N: TypeIdent> TypeValue<'s, 'v, 't> for StreamStateTy<'t, N>
where
    's: 'v,
{
    type Value = BamlStreamState<'s, 'v, 't, N>;
}

/// A class field as declared: its type plus the declaration attributes that shape parsing.
///
/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone)]
pub struct AnnotatedField<'t, N: TypeIdent> {
    pub name: Cow<'t, str>,
    pub ty: Ty<'t, N>,
    /// Aliases for the field name.
    /// If any are present, the real name is not used for matching.
    pub aliases: Vec<Cow<'t, str>>,
    /// `@stream.done`: an incomplete value is treated as not supplied, so the field keeps its
    /// default until the value completes.
    pub stream_done: bool,
    /// What the field holds while its object is incomplete and no (partial) value has arrived.
    /// `None` derives it from the type ([`TypeRefDb::partial_default`]); a declaration can pin
    /// it instead: `@stream.must_exist` is `Some(DefaultValue::Never)`, so the class has no
    /// partial parse until the field is present.
    pub default: Option<DefaultValue<N>>,
}
impl<N: TypeIdent> AnnotatedField<'_, N> {
    /// Checks if an input key matches the field exactly (including aliases).
    /// Does not do fuzzy matching.
    pub fn key_matches(&self, key: &str) -> bool {
        if self.aliases.is_empty() {
            self.name == key
        } else {
            self.aliases.iter().any(|a| a == key)
        }
    }
}

#[derive(Clone)]
pub struct AnnotatedEnumVariant<'t> {
    pub name: Cow<'t, str>,
    /// Aliases for the variant name.
    /// If any are present, the real name is not used for matching.
    pub aliases: Vec<Cow<'t, str>>,
}

/// A value derived from a type to stand in for a class field the input has not supplied.
///
/// Where `N` is the type used by the host to identify named types (e.g. class/enum names).
#[derive(Clone, From, PartialEq)]
pub enum DefaultValue<N: TypeIdent> {
    /// The type has no default (`never`), so its container has no value either.
    Never,
    Null,
    #[from(i64)]
    Int(i64),
    Bigint(num_bigint::BigInt),
    #[from(&str, String)]
    String(String),
    #[from(bool)]
    Bool(bool),
    EmptyArray,
    EmptyMap,
    /// A class whose every field has a default.
    Object {
        name: N,
        fields: IndexMap<String, DefaultValue<N>>,
    },
    EnumVariant {
        enum_name: N,
        variant_name: String,
    },
    /// A `StreamState` field that has not started, around the inner type's default.
    StreamStatePending(Box<DefaultValue<N>>),
}

impl<N: TypeIdent> fmt::Debug for DefaultValue<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefaultValue::Never => f.write_str("never"),
            DefaultValue::Null => f.write_str("null"),
            DefaultValue::Int(i) => write!(f, "{i}"),
            DefaultValue::Bigint(b) => write!(f, "{b}n"),
            DefaultValue::String(s) => write!(f, "{s:?}"),
            DefaultValue::Bool(b) => write!(f, "{b}"),
            DefaultValue::EmptyArray => f.write_str("[]"),
            DefaultValue::EmptyMap => f.write_str("{}"),
            DefaultValue::Object { name, fields } => {
                write!(f, "{name} ")?;
                f.debug_map().entries(fields.iter()).finish()
            }
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => write!(f, "{enum_name}.{variant_name}"),
            DefaultValue::StreamStatePending(inner) => write!(f, "Pending({inner:?})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baml_db;

    type Db = TypeRefDb<'static, &'static str>;

    fn field<'a>(
        db: &'a Db,
        class: &'static str,
        field: usize,
    ) -> &'a AnnotatedField<'a, &'static str> {
        let Some(TyResolvedRef::Class(class)) = db.resolve_name(&class) else {
            panic!("{class} is a class in the db");
        };
        &class.fields[field]
    }

    fn partial(db: &Db, class: &'static str, index: usize) -> DefaultValue<&'static str> {
        db.field_default(field(db, class, index))
            .expect("field type resolves")
            .into_owned()
    }

    fn missing(db: &Db, class: &'static str, index: usize) -> DefaultValue<&'static str> {
        db.missing_default(&field(db, class, index).ty)
            .expect("field type resolves")
    }

    #[test]
    fn every_row_of_the_default_table() {
        let db = baml_db! {
            enum One { Only }
            enum Many { A, B }
            class Row {
                nullable: (int | null),
                literal_str: "resume",
                literal_int: 7,
                literal_bool: true,
                text: string,
                list: [int],
                dict: map<string, int>,
                number: int,
                big: bigint,
                real: float,
                flag: bool,
                one: One,
                one_variant: Many.A,
                many: Many,
                non_nullable_union: (string | int),
                state: StreamState<string>,
                never_state: StreamState<int>,
            }
        };
        let expected: [(DefaultValue<&str>, DefaultValue<&str>); 17] = [
            (DefaultValue::Null, DefaultValue::Null),
            (DefaultValue::String("resume".into()), DefaultValue::Never),
            (DefaultValue::Int(7), DefaultValue::Never),
            (DefaultValue::Bool(true), DefaultValue::Never),
            (DefaultValue::String(String::new()), DefaultValue::Never),
            (DefaultValue::EmptyArray, DefaultValue::EmptyArray),
            (DefaultValue::EmptyMap, DefaultValue::EmptyMap),
            (DefaultValue::Never, DefaultValue::Never),
            (DefaultValue::Never, DefaultValue::Never),
            (DefaultValue::Never, DefaultValue::Never),
            (DefaultValue::Never, DefaultValue::Never),
            (
                DefaultValue::EnumVariant {
                    enum_name: "One",
                    variant_name: "Only".into(),
                },
                DefaultValue::Never,
            ),
            (
                DefaultValue::EnumVariant {
                    enum_name: "Many",
                    variant_name: "A".into(),
                },
                DefaultValue::Never,
            ),
            (DefaultValue::Never, DefaultValue::Never),
            (DefaultValue::Never, DefaultValue::Never),
            (
                DefaultValue::StreamStatePending(Box::new(DefaultValue::String(String::new()))),
                DefaultValue::Never,
            ),
            (DefaultValue::Never, DefaultValue::Never),
        ];
        for (index, (partial_default, missing_default)) in expected.into_iter().enumerate() {
            assert_eq!(
                partial(&db, "Row", index),
                partial_default,
                "field {index} partial"
            );
            assert_eq!(
                missing(&db, "Row", index),
                missing_default,
                "field {index} missing"
            );
        }
    }

    #[test]
    fn class_defaults_recurse_and_stop_at_the_first_never() {
        let db = baml_db! {
            class Leaf {
                label: string,
                tags: [string],
            }
            class Branch {
                leaf: Leaf,
                count: (int | null),
            }
            class Broken {
                leaf: Leaf,
                count: int,
            }
            class Holder {
                branch: Branch,
                broken: Broken,
            }
        };
        assert_eq!(
            partial(&db, "Holder", 0),
            DefaultValue::Object {
                name: "Branch",
                fields: IndexMap::from([
                    (
                        "leaf".to_string(),
                        DefaultValue::Object {
                            name: "Leaf",
                            fields: IndexMap::from([
                                ("label".to_string(), DefaultValue::String(String::new())),
                                ("tags".to_string(), DefaultValue::EmptyArray),
                            ]),
                        },
                    ),
                    ("count".to_string(), DefaultValue::Null),
                ]),
            }
        );
        assert_eq!(partial(&db, "Holder", 1), DefaultValue::Never);
        assert_eq!(missing(&db, "Holder", 0), DefaultValue::Never);
    }

    #[test]
    fn declaration_attributes_shape_the_defaults() {
        let db = baml_db! {
            class Item {
                id: string @stream.must_exist,
                body: string @stream.done,
            }
        };
        assert_eq!(partial(&db, "Item", 0), DefaultValue::Never);
        assert_eq!(missing(&db, "Item", 0), DefaultValue::Never);
        // `@stream.done` changes when the default is used, not what it is.
        assert_eq!(partial(&db, "Item", 1), DefaultValue::String(String::new()));
    }

    #[test]
    fn self_reference_is_never_but_indirection_is_fine() {
        let db = baml_db! {
            class Node {
                next: Node,
            }
            class Tree {
                children: [Tree],
                parent: (Tree | null),
            }
            class Holder {
                node: Node,
                tree: Tree,
            }
        };
        assert_eq!(partial(&db, "Holder", 0), DefaultValue::Never);
        assert_eq!(
            partial(&db, "Holder", 1),
            DefaultValue::Object {
                name: "Tree",
                fields: IndexMap::from([
                    ("children".to_string(), DefaultValue::EmptyArray),
                    ("parent".to_string(), DefaultValue::Null),
                ]),
            }
        );
    }

    #[test]
    fn alias_cycles_terminate_and_keep_nullability() {
        let db = baml_db! {
            type MaybeText = (TextRef | null);
            type TextRef = (MaybeText | int);
            type Json = (int | string | null | JsonList);
            type JsonList = [Json];
            class Holder {
                maybe: MaybeText,
                text_ref: TextRef,
                json: Json,
                list: JsonList,
            }
        };
        assert_eq!(partial(&db, "Holder", 0), DefaultValue::Null);
        // `TextRef` is nullable only through `MaybeText`, which is nullable through `TextRef`.
        assert_eq!(partial(&db, "Holder", 1), DefaultValue::Null);
        assert_eq!(partial(&db, "Holder", 2), DefaultValue::Null);
        assert_eq!(partial(&db, "Holder", 3), DefaultValue::EmptyArray);
    }

    #[test]
    fn a_dangling_field_type_is_an_error() {
        let db = baml_db! {
            class Holder {
                missing: Nowhere,
            }
        };
        assert_eq!(
            db.field_default(field(&db, "Holder", 0)).err(),
            Some(&"Nowhere")
        );
        assert_eq!(
            db.missing_default(&field(&db, "Holder", 0).ty).err(),
            Some(&"Nowhere")
        );
    }
}
