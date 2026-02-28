use std::borrow::Cow;

use derive_more::From;
use indexmap::IndexMap;

use crate::{deserializer::types::BamlValueWithFlags, sap_model::TypeIdent};

pub struct ValueWithMeta<T, M> {
    pub value: T,
    pub meta: M,
}

impl<T: Clone, M: Clone> Clone for ValueWithMeta<T, M> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            meta: self.meta.clone(),
        }
    }
}

impl<T, M> ValueWithMeta<T, M> {
    pub const fn new(value: T, meta: M) -> Self {
        Self { value, meta }
    }
    pub const fn as_ref(&self) -> ValueWithMeta<&T, &M> {
        ValueWithMeta {
            value: &self.value,
            meta: &self.meta,
        }
    }
    pub fn map_value<U, F: FnOnce(T) -> U>(self, f: F) -> ValueWithMeta<U, M> {
        ValueWithMeta {
            value: f(self.value),
            meta: self.meta,
        }
    }
    pub fn map_meta<U, F: FnOnce(M) -> U>(self, f: F) -> ValueWithMeta<T, U> {
        ValueWithMeta {
            value: self.value,
            meta: f(self.meta),
        }
    }
}

#[derive(Clone, From)]
pub enum BamlValue<'s, 'v, 't, N: TypeIdent> {
    String(BamlString<'s>),
    Int(BamlInt),
    Float(BamlFloat),
    Bool(BamlBool),
    Null(BamlNull),
    Media(BamlMedia),
    Array(BamlArray<'s, 'v, 't, N>),
    Map(BamlMap<'s, 'v, 't, N>),
    Enum(BamlEnum<'t, N>),
    Class(BamlClass<'s, 'v, 't, N>),
    StreamState(BamlStreamState<'s, 'v, 't, N>),
}

#[derive(Clone, From)]
pub enum BamlPrimitive<'s> {
    String(BamlString<'s>),
    Int(BamlInt),
    Float(BamlFloat),
    Bool(BamlBool),
    Null(BamlNull),
    Media(BamlMedia),
}
impl<'s, N: TypeIdent> From<BamlPrimitive<'s>> for BamlValue<'s, '_, '_, N> {
    fn from(value: BamlPrimitive<'s>) -> Self {
        match value {
            BamlPrimitive::String(s) => BamlValue::String(s),
            BamlPrimitive::Int(i) => BamlValue::Int(i),
            BamlPrimitive::Float(f) => BamlValue::Float(f),
            BamlPrimitive::Bool(b) => BamlValue::Bool(b),
            BamlPrimitive::Null(n) => BamlValue::Null(n),
            BamlPrimitive::Media(m) => BamlValue::Media(m),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BamlString<'s> {
    pub value: Cow<'s, str>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BamlInt {
    pub value: i64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BamlFloat {
    pub value: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BamlBool {
    pub value: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BamlNull;
#[derive(Debug, Clone)]
pub struct BamlMedia;
#[derive(Clone)]
pub struct BamlArray<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    pub value: Vec<BamlValueWithFlags<'s, 'v, 't, N>>,
}
#[derive(Clone)]
pub struct BamlMap<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    pub value: IndexMap<Cow<'s, str>, BamlValueWithFlags<'s, 'v, 't, N>>,
}
#[derive(Clone, Copy)]
pub struct BamlEnum<'t, N: TypeIdent + 't> {
    pub name: &'t N,
    pub value: &'t str,
}
#[derive(Clone)]
pub struct BamlClass<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    pub name: &'t N,
    pub value: IndexMap<&'t str, BamlValueWithFlags<'s, 'v, 't, N>>,
}
#[derive(Clone)]
pub enum BamlStreamState<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    Incomplete(Box<BamlValueWithFlags<'s, 'v, 't, N>>),
    Complete(Box<BamlValueWithFlags<'s, 'v, 't, N>>),
}

/// A BAML value with associated metadata. Can be used to represent various kinds of metadata.
///
/// ## Generics
/// - `M`: The type of metadata.
/// - `N`: the type used by the host to identify a type reference (i.e. enum or class name).
pub type BamlValueWithMeta<'s, 'v, 't, M, N> = ValueWithMeta<BamlValue<'s, 'v, 't, N>, M>;
