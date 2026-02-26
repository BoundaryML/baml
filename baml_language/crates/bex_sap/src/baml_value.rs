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
pub enum BamlValue<'t, N: TypeIdent> {
    String(BamlString),
    Int(BamlInt),
    Float(BamlFloat),
    Bool(BamlBool),
    Null(BamlNull),
    Media(BamlMedia),
    Array(BamlArray<'t, N>),
    Map(BamlMap<'t, N>),
    Enum(BamlEnum<'t, N>),
    Class(BamlClass<'t, N>),
    StreamState(BamlStreamState<'t, N>),
}

#[derive(Clone, From)]
pub enum BamlPrimitive {
    String(BamlString),
    Int(BamlInt),
    Float(BamlFloat),
    Bool(BamlBool),
    Null(BamlNull),
    Media(BamlMedia),
}
impl<'t, N: TypeIdent> From<BamlPrimitive> for BamlValue<'t, N> {
    fn from(value: BamlPrimitive) -> Self {
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
pub struct BamlString {
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BamlInt {
    pub value: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BamlFloat {
    pub value: f64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BamlBool {
    pub value: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BamlNull;
#[derive(Debug, Clone)]
pub struct BamlMedia;
#[derive(Clone)]
pub struct BamlArray<'t, N: TypeIdent> {
    pub value: Vec<BamlValueWithFlags<'t, N>>,
}
#[derive(Clone)]
pub struct BamlMap<'t, N: TypeIdent> {
    pub value: IndexMap<String, BamlValueWithFlags<'t, N>>,
}
#[derive(Clone)]
pub struct BamlEnum<'t, N: TypeIdent + 't> {
    pub name: &'t N,
    pub value: String,
}
#[derive(Clone)]
pub struct BamlClass<'t, N: TypeIdent> {
    pub name: &'t N,
    pub value: IndexMap<String, BamlValueWithFlags<'t, N>>,
}
#[derive(Clone)]
pub enum BamlStreamState<'t, N: TypeIdent> {
    Incomplete(Box<BamlValueWithFlags<'t, N>>),
    Complete(Box<BamlValueWithFlags<'t, N>>),
}

/// A BAML value with associated metadata. Can be used to represent various kinds of metadata.
///
/// ## Generics
/// - `T`: The type of metadata.
/// - `N`: the type used by the host to identify a type reference (i.e. enum or class name).
pub type BamlValueWithMeta<'t, T, N: TypeIdent> = ValueWithMeta<BamlValue<'t, N>, T>;
