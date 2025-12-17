use std::collections::HashSet;

use indexmap::IndexSet;
use itertools::Itertools;

use crate::{
    baml_value::{TypeLookups, TypeLookupsMeta},
    type_meta::MayHaveMeta,
    BamlMediaType, ConstraintLevel,
};

mod builder;
mod converters;
mod display;
mod simplify;
pub mod type_meta;
mod union_type;
pub use display::MetaSuffix;
pub use union_type::UnionConstructor;

// Types, depending on the context, have different metadata attached to them.
// When you define a type in BAML you have the IR rep of the type.
// Sometimes you use them in streaming or nonstreaming contexts.
/// The building block of IR types in BAML.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Eq, Hash)]
pub enum TypeGeneric<T> {
    // Type that can be casted to any other. Used for generics that accept anything, e.g std::fetch_value.
    Top(T),
    Primitive(TypeValue, T),
    Enum {
        name: String,
        dynamic: bool,
        meta: T,
    },
    Literal(LiteralValue, T),
    Class {
        name: String,
        mode: StreamingMode,
        dynamic: bool,
        meta: T,
    },
    List(Box<TypeGeneric<T>>, T),
    Map(Box<TypeGeneric<T>>, Box<TypeGeneric<T>>, T),
    RecursiveTypeAlias {
        name: String,
        mode: StreamingMode,
        meta: T,
    },
    Tuple(Vec<TypeGeneric<T>>, T),
    Arrow(Box<ArrowGeneric<T>>, T),
    Union(UnionTypeGeneric<T>, T),
}

impl TypeValue {
    pub fn basename(&self) -> &'static str {
        match self {
            TypeValue::String => "string",
            TypeValue::Int => "int",
            TypeValue::Float => "float",
            TypeValue::Bool => "bool",
            TypeValue::Null => "null",
            TypeValue::Media(_) => "media",
        }
    }
}

impl<T> TypeGeneric<T> {
    pub fn basename(&self) -> &'static str {
        match self {
            TypeGeneric::Top(_) => "ANY",
            TypeGeneric::Primitive(type_value, _) => type_value.basename(),
            TypeGeneric::Enum { .. } => "enum",
            TypeGeneric::Literal(lit, _) => match lit {
                LiteralValue::String(_) => "string",
                LiteralValue::Int(_) => "int",
                LiteralValue::Bool(_) => "bool",
            },
            TypeGeneric::Class { .. } => "class",
            TypeGeneric::List(_, _) => "list",
            TypeGeneric::Map(_, _, _) => "map",
            TypeGeneric::RecursiveTypeAlias { .. } => "type alias",
            TypeGeneric::Tuple(_, _) => "tuple",
            TypeGeneric::Arrow(_, _) => "function",
            TypeGeneric::Union(_, _) => "union",
        }
    }
}

macro_rules! impl_as_variant {
    ($method_name:ident, $variant:pat, $err_msg:literal) => {
        pub fn $method_name<U: TypeLookupsMeta<T>>(
            self,
            lookup: &U,
        ) -> anyhow::Result<TypeGeneric<T>> {
            match self {
                $variant => Ok(self),
                TypeGeneric::RecursiveTypeAlias { name, .. } => {
                    let expanded_type = TypeLookupsMeta::<T>::expand_recursive_type(lookup, &name)?;
                    expanded_type.$method_name::<U>(lookup)
                }
                _ => anyhow::bail!(concat!("Expected a ", $err_msg, ", got: {}"), self),
            }
        }
    };
}

impl<T: MetaSuffix> TypeGeneric<T> {
    impl_as_variant!(resolve_map, TypeGeneric::Map(..), "map type");
    impl_as_variant!(resolve_list, TypeGeneric::List(..), "list type");
    impl_as_variant!(resolve_enum, TypeGeneric::Enum { .. }, "enum type");
    impl_as_variant!(resolve_class, TypeGeneric::Class { .. }, "class type");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, strum::Display)]
pub enum StreamingMode {
    NonStreaming,
    Streaming,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnionTypeGeneric<T> {
    types: Vec<TypeGeneric<T>>,
    null_type: Box<TypeGeneric<T>>,
}

/// A convenience type alias for BAML types in the IR.
pub type TypeIR = TypeGeneric<type_meta::IR>;
pub type TypeNonStreaming = TypeGeneric<type_meta::NonStreaming>;
pub type TypeStreaming = TypeGeneric<type_meta::Streaming>;

/// Wrapper type that implements Display. Not implementing display directly for TypeIR because
/// we may want multiple display modes.
pub struct TypeIRDiagnosticRepr<'a>(&'a TypeIR);

impl TypeIR {
    pub fn diagnostic_repr(&self) -> TypeIRDiagnosticRepr<'_> {
        TypeIRDiagnosticRepr(self)
    }
}

impl std::fmt::Display for TypeIRDiagnosticRepr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn repr_list<'a>(
            f: &mut std::fmt::Formatter<'_>,
            iter: impl IntoIterator<Item = &'a TypeIR>,
            sep: &'static str,
        ) -> std::fmt::Result {
            let mut iter = iter.into_iter().map(TypeIRDiagnosticRepr);

            if let Some(first) = iter.next() {
                write!(f, "{first}")?;
                for next in iter {
                    write!(f, "{sep}{next}")?;
                }
            }

            Ok(())
        }

        fn repr_tuple<'a>(
            f: &mut std::fmt::Formatter<'_>,
            iter: impl IntoIterator<Item = &'a TypeIR>,
        ) -> std::fmt::Result {
            f.write_str("(")?;

            repr_list(f, iter, ", ")?;

            f.write_str(")")
        }

        match self.0 {
            TypeGeneric::Top(_) => f.write_str("ANY"),
            TypeGeneric::Primitive(type_value, _) => f.write_str(type_value.basename()),
            TypeGeneric::Enum { name, dynamic, .. } => write!(
                f,
                "enum `{name}`{}",
                if *dynamic { " (dynamic)" } else { "" }
            ),
            TypeGeneric::Literal(literal_value, _) => f.write_str(match literal_value {
                LiteralValue::String(_) => "string",
                LiteralValue::Int(_) => "int",
                LiteralValue::Bool(_) => "bool",
            }),
            TypeGeneric::Class { name, dynamic, .. } => {
                write!(
                    f,
                    "class `{name}`{}",
                    if *dynamic { " (dynamic)" } else { "" }
                )
            }
            TypeGeneric::List(type_generic, _) => {
                write!(f, "{}[]", type_generic.diagnostic_repr())
            }
            TypeGeneric::Map(key, value, _) => write!(
                f,
                "map<{}, {}>",
                key.diagnostic_repr(),
                value.diagnostic_repr()
            ),
            TypeGeneric::RecursiveTypeAlias { name, .. } => f.write_str(name),
            TypeGeneric::Tuple(type_generics, _) => repr_tuple(f, type_generics),
            TypeGeneric::Arrow(arrow_generic, _) => {
                f.write_str("fn")?;
                repr_tuple(f, &arrow_generic.param_types)?;
                write!(f, " -> {}", arrow_generic.return_type.diagnostic_repr())
            }
            TypeGeneric::Union(union_type_generic, _) => {
                repr_list(f, &union_type_generic.types, " | ")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, Eq, Hash)]
pub enum TypeValue {
    String,
    Int,
    Float,
    Bool,
    // Char,
    Null,
    Media(BamlMediaType),
}

impl std::str::FromStr for TypeValue {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "string" => TypeValue::String,
            "int" => TypeValue::Int,
            "float" => TypeValue::Float,
            "bool" => TypeValue::Bool,
            "null" => TypeValue::Null,
            "image" => TypeValue::Media(BamlMediaType::Image),
            "audio" => TypeValue::Media(BamlMediaType::Audio),
            "pdf" => TypeValue::Media(BamlMediaType::Pdf),
            "video" => TypeValue::Media(BamlMediaType::Video),
            _ => return Err(()),
        })
    }
}

impl std::fmt::Display for TypeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeValue::String => write!(f, "string"),
            TypeValue::Int => write!(f, "int"),
            TypeValue::Float => write!(f, "float"),
            TypeValue::Bool => write!(f, "bool"),
            TypeValue::Null => write!(f, "null"),
            TypeValue::Media(BamlMediaType::Image) => write!(f, "image"),
            TypeValue::Media(BamlMediaType::Audio) => write!(f, "audio"),
            TypeValue::Media(BamlMediaType::Pdf) => write!(f, "pdf"),
            TypeValue::Media(BamlMediaType::Video) => write!(f, "video"),
        }
    }
}

/// Subset of [`crate::BamlValue`] allowed for literal type definitions.
#[derive(serde::Serialize, Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}

impl From<i64> for LiteralValue {
    fn from(value: i64) -> Self {
        LiteralValue::Int(value)
    }
}

impl From<bool> for LiteralValue {
    fn from(value: bool) -> Self {
        LiteralValue::Bool(value)
    }
}

impl From<&str> for LiteralValue {
    fn from(value: &str) -> Self {
        LiteralValue::String(value.to_string())
    }
}

impl LiteralValue {
    pub fn literal_base_type(&self) -> TypeIR {
        match self {
            Self::String(_) => TypeIR::string(),
            Self::Int(_) => TypeIR::int(),
            Self::Bool(_) => TypeIR::bool(),
        }
    }
}

impl std::fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralValue::String(str) => write!(f, "\"{str}\""),
            LiteralValue::Int(int) => write!(f, "{int}"),
            LiteralValue::Bool(bool) => write!(f, "{bool}"),
        }
    }
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnionType {
    types: Vec<TypeIR>,
}

/// A union type may never hold more than 1 null
/// A view into a union type that classifies its variants
#[derive(Debug)]
pub enum UnionTypeView<'a> {
    /// A union containing only the null type
    Null,
    /// A union containing exactly one non-null type and the null type
    Optional(&'a TypeIR),
    /// A union containing multiple non-null types with no optional variants
    OneOf(Vec<&'a TypeIR>),
    /// A union containing multiple types where at least one is optional
    OneOfOptional(Vec<&'a TypeIR>),
}

/// A union type may never hold more than 1 null
/// A view into a union type that classifies its variants
#[derive(Debug)]
pub enum UnionTypeViewGeneric<'a, T> {
    /// A union containing only the null type
    Null,
    /// A union containing exactly one non-null type and the null type
    Optional(&'a TypeGeneric<T>),
    /// A union containing multiple non-null types with no optional variants
    OneOf(Vec<&'a TypeGeneric<T>>),
    /// A union containing multiple types where at least one is optional
    OneOfOptional(Vec<&'a TypeGeneric<T>>),
}

#[derive(Debug)]
enum UnionTypeViewGenericMut<'a, T> {
    /// A union containing only the null type
    Null,
    /// A union containing exactly one non-null type and the null type
    Optional(&'a mut TypeGeneric<T>),
    /// A union containing multiple non-null types with no optional variants
    OneOf(Vec<&'a mut TypeGeneric<T>>),
    /// A union containing multiple types where at least one is optional
    OneOfOptional(Vec<&'a mut TypeGeneric<T>>),
}

impl<T: Default + std::fmt::Debug + Clone + type_meta::MayHaveMeta> UnionTypeViewGeneric<'_, T> {
    /// A helper-function for the `FieldType::flatten`.
    /// See `FieldType::flatten` for context.
    fn flatten(&self) -> Vec<TypeGeneric<T>> {
        match self {
            UnionTypeViewGeneric::Null => vec![(TypeGeneric::null()).clone()],
            UnionTypeViewGeneric::Optional(field_type) => field_type
                .flatten()
                .into_iter()
                .chain(std::iter::once((TypeGeneric::null()).clone()))
                .collect(),
            UnionTypeViewGeneric::OneOf(field_types) => {
                field_types.iter().flat_map(|t| t.flatten()).collect()
            }
            UnionTypeViewGeneric::OneOfOptional(field_types) => field_types
                .iter()
                .flat_map(|t| t.flatten())
                .chain(std::iter::once((TypeGeneric::null()).clone()))
                .collect(),
        }
    }
}

impl<T> UnionTypeGeneric<T> {
    pub fn is_optional(&self) -> bool {
        match self.view() {
            UnionTypeViewGeneric::Null => true,
            UnionTypeViewGeneric::Optional(..) => true,
            UnionTypeViewGeneric::OneOf(..) => false,
            UnionTypeViewGeneric::OneOfOptional(..) => true,
        }
    }

    pub fn add_type(&mut self, t: TypeGeneric<T>) {
        self.types.push(t);
    }

    pub fn view(&self) -> UnionTypeViewGeneric<'_, T> {
        let non_null_types = self
            .types
            .iter()
            .filter(|t| !t.is_null())
            .collect::<Vec<_>>();
        match non_null_types.len() {
            0 => UnionTypeViewGeneric::Null,
            1 => UnionTypeViewGeneric::Optional(non_null_types[0]),
            _ => {
                if non_null_types.len() == self.types.len() {
                    UnionTypeViewGeneric::OneOf(non_null_types)
                } else {
                    UnionTypeViewGeneric::OneOfOptional(non_null_types)
                }
            }
        }
    }

    fn view_mut(&mut self) -> UnionTypeViewGenericMut<'_, T> {
        let num_types = self.types.len();
        let non_null_types = self
            .types
            .iter_mut()
            .filter(|t| !t.is_null())
            .collect::<Vec<_>>();
        match non_null_types.len() {
            0 => UnionTypeViewGenericMut::Null,
            1 => {
                let mut non_null_types = non_null_types;
                UnionTypeViewGenericMut::Optional(
                    non_null_types
                        .pop()
                        .expect("Expected exactly one non-null type"),
                )
            }
            _ => {
                if non_null_types.len() == num_types {
                    UnionTypeViewGenericMut::OneOf(non_null_types)
                } else {
                    UnionTypeViewGenericMut::OneOfOptional(non_null_types)
                }
            }
        }
    }

    pub fn iter_skip_null_mut(&mut self) -> Vec<&mut TypeGeneric<T>> {
        match self.view_mut() {
            UnionTypeViewGenericMut::Null => vec![],
            UnionTypeViewGenericMut::Optional(field_type) => vec![field_type],
            UnionTypeViewGenericMut::OneOf(items) => items,
            UnionTypeViewGenericMut::OneOfOptional(items) => items,
        }
    }

    pub fn iter_skip_null(&self) -> Vec<&TypeGeneric<T>> {
        match self.view() {
            UnionTypeViewGeneric::Null => vec![],
            UnionTypeViewGeneric::Optional(field_type) => vec![field_type],
            UnionTypeViewGeneric::OneOf(items) => items,
            UnionTypeViewGeneric::OneOfOptional(items) => items,
        }
    }

    pub fn iter_include_null(&self) -> Vec<&TypeGeneric<T>> {
        let mut iter = self.iter_skip_null();
        if self.is_optional() {
            iter.push(&self.null_type);
        }
        iter
    }
}

pub struct SelectedTypeIndexResult<'a, T> {
    // If None, then value is a null
    pub index: usize,
    // Null should be in the options list if its allowed
    pub options: Vec<&'a TypeGeneric<T>>,
}

impl<T: std::cmp::Eq + std::hash::Hash> UnionTypeGeneric<T>
where
    TypeGeneric<T>: std::fmt::Display,
{
    pub fn selected_type_index(
        &self,
        type_to_find: &TypeGeneric<T>,
        lookup: &impl TypeLookupsMeta<T>,
    ) -> Result<SelectedTypeIndexResult<'_, T>, anyhow::Error> {
        let options = self.iter_include_null();

        for (i, t) in options.iter().enumerate() {
            if match t {
                TypeGeneric::RecursiveTypeAlias { name, .. } => {
                    &TypeLookupsMeta::expand_recursive_type(lookup, name.as_str())? == type_to_find
                }
                _ => *t == type_to_find,
            } {
                return Ok(SelectedTypeIndexResult { index: i, options });
            }
        }

        Err(anyhow::anyhow!("Failed to find {type_to_find} in union"))
    }
}

pub trait HasType<T> {
    fn field_type(&self) -> &TypeGeneric<T>;
}

impl HasType<type_meta::IR> for TypeIR {
    fn field_type(&self) -> &TypeIR {
        self
    }
}

impl HasType<type_meta::NonStreaming> for TypeNonStreaming {
    fn field_type(&self) -> &TypeGeneric<type_meta::NonStreaming> {
        self
    }
}

impl HasType<type_meta::Streaming> for TypeStreaming {
    fn field_type(&self) -> &TypeGeneric<type_meta::Streaming> {
        self
    }
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Arrow {
    pub param_types: Vec<TypeIR>,
    pub return_type: TypeIR,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrowGeneric<T> {
    pub param_types: Vec<TypeGeneric<T>>,
    pub return_type: TypeGeneric<T>,
}

impl<T> TypeGeneric<T> {
    /// Consolidate all `Null` types appear in a (potentially deeply) nested
    /// Union, and remove the tree structure of nested unions.
    ///
    /// e.g. (( ((int | null) | int) | (map<string,string> | null ))) =>
    ///         int | int | map<string,string> | null
    ///
    /// Note: Unions with @check constraints are NOT flattened - they are
    /// preserved as a unit to avoid losing the check metadata.
    pub fn flatten(&self) -> Vec<TypeGeneric<T>>
    where
        T: Clone + std::fmt::Debug + Default + type_meta::MayHaveMeta,
    {
        match self {
            TypeGeneric::Union(inner, meta) => {
                // Don't flatten unions that have @check constraints - they are
                // "wrapped" types that should be preserved as a unit
                if meta.has_checks() {
                    vec![self.clone()]
                } else {
                    inner.view().flatten()
                }
            }
            _ => vec![self.clone()],
        }
    }

    pub fn find_if<'a>(
        &'a self,
        predicate: &impl Fn(&TypeGeneric<T>) -> bool,
        ignore_map_keys: bool,
    ) -> Vec<&'a TypeGeneric<T>> {
        let mut result = vec![];
        if predicate(self) {
            result.push(self);
        }

        match self {
            TypeGeneric::Top(_)
            | TypeGeneric::Primitive(..)
            | TypeGeneric::Enum { .. }
            | TypeGeneric::Literal(..)
            | TypeGeneric::Class { .. }
            | TypeGeneric::RecursiveTypeAlias { .. } => {}
            TypeGeneric::List(inner, _) => {
                result.extend(inner.find_if(predicate, ignore_map_keys));
            }
            TypeGeneric::Map(key_type, value_type, _) => {
                let mut res = value_type.find_if(predicate, ignore_map_keys);
                if !ignore_map_keys {
                    res.extend(key_type.find_if(predicate, ignore_map_keys));
                }
                result.extend(res);
            }
            TypeGeneric::Tuple(type_generics, _) => {
                for t in type_generics
                    .iter()
                    .flat_map(|t| t.find_if(predicate, ignore_map_keys))
                {
                    result.push(t);
                }
            }
            TypeGeneric::Union(union_type_generic, _) => {
                for t in union_type_generic
                    .iter_skip_null()
                    .iter()
                    .flat_map(|t| t.find_if(predicate, ignore_map_keys))
                {
                    result.push(t);
                }
            }
            TypeGeneric::Arrow(arrow_generic, _) => {
                let res = arrow_generic
                    .param_types
                    .iter()
                    .flat_map(|t| t.find_if(predicate, ignore_map_keys));
                let mut returned = arrow_generic
                    .return_type
                    .find_if(predicate, ignore_map_keys);
                returned.extend(res);
                result.extend(returned);
            }
        };

        result
    }

    pub fn set_meta(&mut self, meta: T) {
        match self {
            TypeGeneric::Top(m) => *m = meta,
            TypeGeneric::Class { meta: m, .. } => *m = meta,
            TypeGeneric::Arrow(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Primitive(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Enum { meta: m, .. } => *m = meta,
            TypeGeneric::Literal(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::List(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Map(_, _, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::RecursiveTypeAlias { meta: m, .. } => *m = meta,
            TypeGeneric::Tuple(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Union(_, type_metadata_ir) => *type_metadata_ir = meta,
        }
    }

    pub fn meta(&self) -> &T {
        match self {
            TypeGeneric::Top(meta) => meta,
            TypeGeneric::Class { meta, .. } => meta,
            TypeGeneric::Arrow(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::Primitive(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::Enum { meta, .. } => meta,
            TypeGeneric::Literal(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::List(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::Map(_, _, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::RecursiveTypeAlias { meta, .. } => meta,
            TypeGeneric::Tuple(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::Union(_, type_metadata_ir) => type_metadata_ir,
        }
    }

    pub fn map_meta<F, U>(&self, f: F) -> TypeGeneric<U>
    where
        F: Fn(&T) -> U + Copy,
    {
        match self {
            TypeGeneric::Top(meta) => TypeGeneric::Top(f(meta)),
            TypeGeneric::Class {
                meta,
                name,
                mode,
                dynamic,
                ..
            } => TypeGeneric::Class {
                meta: f(meta),
                name: name.clone(),
                mode: *mode,
                dynamic: *dynamic,
            },
            TypeGeneric::Arrow(arrow, type_metadata_ir) => TypeGeneric::Arrow(
                Box::new(ArrowGeneric {
                    param_types: arrow.param_types.iter().map(|t| t.map_meta(f)).collect(),
                    return_type: arrow.return_type.map_meta(f),
                }),
                f(type_metadata_ir),
            ),
            TypeGeneric::Primitive(value, type_metadata_ir) => {
                TypeGeneric::Primitive(*value, f(type_metadata_ir))
            }
            TypeGeneric::Enum {
                meta,
                name,
                dynamic,
            } => TypeGeneric::Enum {
                meta: f(meta),
                name: name.clone(),
                dynamic: *dynamic,
            },
            TypeGeneric::Literal(literal_value, type_metadata_ir) => {
                TypeGeneric::Literal(literal_value.clone(), f(type_metadata_ir))
            }
            TypeGeneric::List(inner, type_metadata_ir) => {
                TypeGeneric::List(Box::new(inner.map_meta(f)), f(type_metadata_ir))
            }
            TypeGeneric::Map(field_type, field_type1, type_metadata_ir) => TypeGeneric::Map(
                Box::new(field_type.map_meta(f)),
                Box::new(field_type1.map_meta(f)),
                f(type_metadata_ir),
            ),
            TypeGeneric::RecursiveTypeAlias { meta, name, mode } => {
                TypeGeneric::RecursiveTypeAlias {
                    meta: f(meta),
                    mode: *mode,
                    name: name.clone(),
                }
            }
            TypeGeneric::Tuple(inner, type_metadata_ir) => TypeGeneric::Tuple(
                inner.iter().map(|t| t.map_meta(f)).collect(),
                f(type_metadata_ir),
            ),
            TypeGeneric::Union(inner, type_metadata_ir) => TypeGeneric::Union(
                UnionTypeGeneric {
                    types: inner.types.iter().map(|t| t.map_meta(f)).collect(),
                    null_type: Box::new(inner.null_type.map_meta(f)),
                },
                f(type_metadata_ir),
            ),
        }
    }

    pub fn meta_mut(&mut self) -> &mut T {
        match self {
            TypeGeneric::Top(meta)
            | TypeGeneric::Class { meta, .. }
            | TypeGeneric::Arrow(_, meta)
            | TypeGeneric::Primitive(_, meta)
            | TypeGeneric::Enum { meta, .. }
            | TypeGeneric::Literal(_, meta)
            | TypeGeneric::List(_, meta)
            | TypeGeneric::Map(_, _, meta)
            | TypeGeneric::RecursiveTypeAlias { meta, .. }
            | TypeGeneric::Tuple(_, meta)
            | TypeGeneric::Union(_, meta) => meta,
        }
    }

    /// For types that are unions, replace the variants list with
    /// a simplified (flattened) variants list.
    pub fn is_primitive(&self) -> bool {
        match self {
            TypeGeneric::Primitive(_, _) => true,
            TypeGeneric::List(t, _) => t.is_primitive(),
            _ => false,
        }
    }

    pub fn is_literal(&self) -> bool {
        matches!(self, TypeGeneric::Literal(_, _))
    }

    pub fn is_optional(&self) -> bool {
        match self {
            TypeGeneric::Primitive(TypeValue::Null, _) => true,
            TypeGeneric::Union(choices, _) => choices.is_optional(),
            _ => false,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, TypeGeneric::Primitive(TypeValue::Null, _))
    }

    /// The immediate (non transitive) dependencies of a given type?
    pub fn dependencies(&self) -> HashSet<String>
    where
        T: Clone + std::fmt::Debug + Default,
    {
        let mut deps = HashSet::new();
        let mut queue = vec![self];
        while let Some(current) = queue.pop() {
            match current {
                TypeGeneric::Class { name, .. } => {
                    deps.insert(name.clone());
                }
                TypeGeneric::Enum { name, .. } => {
                    deps.insert(name.clone());
                }
                TypeGeneric::List(inner, _) => {
                    queue.push(inner);
                }
                TypeGeneric::Map(field_type, field_type1, _) => {
                    queue.push(field_type);
                    queue.push(field_type1);
                }
                TypeGeneric::Union(inner, _) => match inner.view() {
                    UnionTypeViewGeneric::Null => {}
                    UnionTypeViewGeneric::Optional(field_type) => queue.push(field_type),
                    UnionTypeViewGeneric::OneOf(field_types) => {
                        queue.extend(field_types.into_iter())
                    }
                    UnionTypeViewGeneric::OneOfOptional(field_types) => {
                        queue.extend(field_types.into_iter())
                    }
                },
                TypeGeneric::Tuple(inner, _) => {
                    queue.extend(inner.iter());
                }
                TypeGeneric::Arrow(arrow, _) => {
                    queue.extend(arrow.param_types.iter());
                    queue.push(&arrow.return_type);
                }
                TypeGeneric::RecursiveTypeAlias { name, .. } => {
                    deps.insert(name.clone());
                }
                TypeGeneric::Top(_) | TypeGeneric::Primitive(_, _) | TypeGeneric::Literal(_, _) => {
                }
            }
        }
        deps
    }
}

impl TypeIR {
    /// Convert a `FieldType` (a type specified in BAML source code) into
    /// a `StreamingType` (a type that can be used for streaming).
    ///
    /// The streaming-behavior related metadata is applied to the types, and the
    /// annotations are also recorded in the streaming metadata (because not all
    /// streaming behavior is reflected in the Base-type).
    ///
    /// The types of transformations done here:
    ///   - Replacing classes with their streaming-module counterparts.
    ///   - Overriding nullability based on @not_null annotations
    ///   - Overriding nullability based on the field's type
    ///   - Preserving the original type according to the @done annotation
    ///
    /// We do not explicitly represent @stream.with_state or @stream.checked.
    /// Downstream consumers of `StreamingType` must check these properties
    /// in the metadata.
    pub fn to_streaming_type(&self, lookup: &impl TypeLookups) -> TypeStreaming {
        converters::streaming::from_type_ir(self, lookup)
    }

    pub fn to_non_streaming_type(&self, lookup: &impl TypeLookups) -> TypeNonStreaming {
        converters::non_streaming::from_type_ir(self, lookup)
    }
}

fn merge_modes<Mode: Iterator<Item = anyhow::Result<StreamingMode>>>(
    modes: Mode,
) -> anyhow::Result<StreamingMode> {
    // return first error
    // if any are streaming, return streaming
    // else return non-streaming
    for mode in modes.into_iter() {
        match mode {
            Ok(StreamingMode::Streaming) => return Ok(StreamingMode::Streaming),
            Ok(StreamingMode::NonStreaming) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(StreamingMode::NonStreaming)
}

impl<T: type_meta::MayHaveMeta> TypeGeneric<T> {
    pub fn mode(
        &self,
        mode: &StreamingMode,
        _lookup: &impl TypeLookups,
        union_depth: usize,
    ) -> anyhow::Result<StreamingMode> {
        if *mode == StreamingMode::NonStreaming {
            return Ok(StreamingMode::NonStreaming);
        }

        if union_depth > 1 && self.meta().has_stream_state() {
            return Ok(StreamingMode::Streaming);
        }

        match self {
            TypeGeneric::Class { mode, .. } => Ok(*mode),
            TypeGeneric::Top(_)
            | TypeGeneric::Arrow(_, _)
            | TypeGeneric::Primitive(_, _)
            | TypeGeneric::Enum { .. }
            | TypeGeneric::Literal(_, _) => Ok(StreamingMode::NonStreaming),
            TypeGeneric::List(inner, _) => inner.mode(mode, _lookup, union_depth),
            TypeGeneric::Map(key, value, ..) => {
                let items: Vec<Result<StreamingMode, anyhow::Error>> = vec![
                    key.mode(mode, _lookup, union_depth),
                    value.mode(mode, _lookup, union_depth),
                ];
                merge_modes(items.into_iter())
            }
            TypeGeneric::RecursiveTypeAlias { mode, .. } => Ok(*mode),
            TypeGeneric::Tuple(inner, _) => {
                merge_modes(inner.iter().map(|t| t.mode(mode, _lookup, union_depth)))
            }
            TypeGeneric::Union(union_type_generic, _) => merge_modes(
                union_type_generic
                    .types
                    .iter()
                    .map(|t| t.mode(mode, _lookup, union_depth + 1)),
            ),
        }
    }
}

impl TypeStreaming {
    pub fn to_ir_type(&self) -> TypeIR {
        converters::streaming::to_type_ir(self)
    }
}

impl TypeGeneric<type_meta::IR> {
    pub fn streaming_behavior(&self) -> &type_meta::base::StreamingBehavior {
        &self.meta().streaming_behavior
    }
}

pub trait ToUnionName<T> {
    fn to_union_name(&self, include_metadata: bool) -> String;
    fn find_union_types(&self) -> IndexSet<&TypeGeneric<T>>;
}

impl<Meta: std::hash::Hash + std::cmp::Eq + MayHaveMeta> ToUnionName<Meta> for TypeGeneric<Meta> {
    fn find_union_types(&self) -> IndexSet<&TypeGeneric<Meta>> {
        use TypeGeneric as T;
        // TODO: its pretty hard to get type aliases here
        // let value = self.simplify();
        match self {
            T::Union(_, _) => IndexSet::from_iter([self]),
            T::List(inner, _) => inner.find_union_types(),
            T::Map(field_type, field_type1, _) => {
                let mut set = field_type.find_union_types();
                set.extend(field_type1.find_union_types());
                set
            }
            T::Top(_)
            | T::Primitive(_, _)
            | T::Enum { .. }
            | T::Literal(_, _)
            | T::Class { .. }
            | T::RecursiveTypeAlias { .. }
            | T::Arrow(_, _) => IndexSet::new(),
            T::Tuple(inner, _) => inner.iter().flat_map(|t| t.find_union_types()).collect(),
        }
    }

    fn to_union_name(&self, include_metadata: bool) -> String {
        use TypeGeneric as T;

        let result = match self {
            T::Top(_) => "ANY".to_string(),
            T::Primitive(type_value, _) => type_value.to_string(),
            T::Enum { name, .. } => name.to_string(),
            T::Literal(literal_value, _) => match literal_value {
                LiteralValue::String(value) => format!(
                    "string_{}",
                    value
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '_' })
                        .collect::<String>()
                ),
                LiteralValue::Int(val) => format!("int_{val}"),
                LiteralValue::Bool(val) => format!("bool_{val}"),
            },
            T::Class { name, .. } => name.to_string(),
            T::List(field_type, _) => {
                format!("List__{}", field_type.to_union_name(include_metadata))
            }
            T::Map(field_type, field_type1, _) => {
                format!(
                    "Map__{}_{}",
                    field_type.to_union_name(include_metadata),
                    field_type1.to_union_name(include_metadata)
                )
            }
            T::Union(field_types, _) => {
                let format_union_name = |options: Vec<&TypeGeneric<Meta>>| -> String {
                    options
                        .iter()
                        .map(|t| t.to_union_name(include_metadata))
                        .sorted()
                        .collect::<Vec<_>>()
                        .join("__")
                };
                let wrap_optional = |name: String| -> String {
                    if include_metadata {
                        format!("Optional__{}", name)
                    } else {
                        name
                    }
                };

                match field_types.view() {
                    UnionTypeViewGeneric::Null => "null".to_string(),
                    UnionTypeViewGeneric::Optional(field_type) => {
                        wrap_optional(field_type.to_union_name(include_metadata))
                    }
                    UnionTypeViewGeneric::OneOf(field_types) => format_union_name(field_types),
                    UnionTypeViewGeneric::OneOfOptional(field_types) => {
                        wrap_optional(format_union_name(field_types))
                    }
                }
            }
            T::Tuple(field_types, _) => format!(
                "Tuple__{}",
                field_types
                    .iter()
                    .map(|v| v.to_union_name(include_metadata))
                    .collect::<Vec<_>>()
                    .join("__")
            ),
            T::RecursiveTypeAlias { name, .. } => name.to_string(),
            T::Arrow(_, _) => "function".to_string(),
        };

        if include_metadata {
            let result = if self.meta().has_stream_state() {
                format!("StreamState__{}", result)
            } else {
                result
            };

            let result = if self.meta().has_checks() {
                format!("Checked__{}", result)
            } else {
                result
            };

            result
        } else {
            result
        }
    }
}

/// Metadata on a type that determines how it behaves under streaming conditions.
#[derive(Clone, Debug, PartialEq, serde::Serialize, Eq, Hash)]
pub struct TypeMetaIR {
    /// A type with the `not_null` property will not be visible in a stream until
    /// we are certain that it is not null (as in the value has at least begun)
    pub needed: bool,

    /// A type with the `done` property will not be visible in a stream until
    /// we are certain that it is completely available (i.e. the parser did
    /// not finalize it through any early termination, enough tokens were available
    /// from the LLM response to be certain that it is done).
    pub done: bool,

    /// A type with the `state` property will be represented in client code as
    /// a struct: `{value: T, streaming_state: "incomplete" | "complete"}`.
    pub state: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ir_type::union_type::UnionConstructor, type_meta::stream::TypeMetaStreaming, Constraint,
    };

    fn make_optional(inner: TypeStreaming) -> TypeStreaming {
        if let TypeStreaming::Union(items, meta) = inner {
            if items.is_optional() {
                return TypeStreaming::Union(items, meta);
            }
            let options = items
                .iter_skip_null()
                .into_iter()
                .cloned()
                .chain(std::iter::once(TypeStreaming::null()))
                .collect::<Vec<_>>();
            return TypeStreaming::Union(unsafe { UnionTypeGeneric::new_unsafe(options) }, meta);
        }
        TypeStreaming::Union(
            unsafe { UnionTypeGeneric::new_unsafe(vec![inner, TypeStreaming::null()]) },
            Default::default(),
        )
    }

    fn make_union<T: std::fmt::Debug + Default>(
        types: Vec<TypeGeneric<T>>,
        meta: T,
    ) -> TypeGeneric<T> {
        TypeGeneric::Union(unsafe { UnionTypeGeneric::new_unsafe(types) }, meta)
    }

    #[test]
    fn simplify_base_case() {
        assert_eq!(TypeIR::null().simplify(), TypeIR::null());
    }

    #[test]
    fn simplify_int() {
        let int = TypeIR::int();
        assert_eq!(int.simplify(), int);
    }

    #[test]
    fn simplify_optional_int() {
        let optional_int = TypeIR::optional(TypeIR::int());
        assert_eq!(optional_int.simplify(), optional_int);
    }

    #[test]
    fn simplify_nested_unions() {
        // ((int | null) | string)
        let inner_union = TypeIR::union(vec![TypeIR::int(), TypeIR::null()]);
        let outer_union = TypeIR::union(vec![inner_union, TypeIR::string()]);
        // union(union(int, null), string)
        assert_eq!(
            outer_union.simplify(),
            TypeIR::union(vec![TypeIR::int(), TypeIR::string(), TypeIR::null()])
        );
    }

    #[test]
    fn simplify_repeated_variants() {
        let union = TypeIR::union(vec![
            TypeIR::int(),
            TypeIR::int(),
            TypeIR::string(),
            TypeIR::string(),
        ]);
        assert_eq!(
            union.simplify(),
            TypeIR::union(vec![TypeIR::int(), TypeIR::string()])
        );
    }

    #[test]
    fn simplify_nested_with_repeats() {
        let inner_union = TypeIR::union(vec![TypeIR::int(), TypeIR::null()]);
        let union = TypeIR::union(vec![TypeIR::int(), inner_union, TypeIR::string()]);
        assert_eq!(
            union.simplify(),
            TypeIR::union(vec![TypeIR::int(), TypeIR::string(), TypeIR::null()])
        );
    }

    struct TestLookup;

    impl TypeLookups for TestLookup {
        fn expand_recursive_type(&self, _name: &str) -> anyhow::Result<&TypeIR> {
            anyhow::bail!("nothing found");
        }
    }

    #[test]
    fn simplify_union_constraints_streaming() {
        struct TestCase {
            name: &'static str,
            input: TypeIR,
            expected: TypeIR,
        }

        let constraint = Constraint::new_check("check all fields are positive", "{{ this }} > 0");
        let streaming_behavior = type_meta::base::StreamingBehavior::default();

        let cases = vec![
            TestCase {
                name: "(A|B)(@check(A, {..})) => (A@check(A, {..})|B@check(B, {..}))",
                input: make_union(
                    vec![TypeIR::int(), TypeIR::float()],
                    type_meta::IR {
                        constraints: vec![constraint.clone()],
                        streaming_behavior: streaming_behavior.clone(),
                    },
                ),
                expected: make_union(
                    vec![
                        TypeIR::int_with_meta(type_meta::IR {
                            constraints: vec![constraint.clone()],
                            streaming_behavior: Default::default(),
                        }),
                        TypeIR::float_with_meta(type_meta::IR {
                            constraints: vec![constraint.clone()],
                            streaming_behavior: Default::default(),
                        }),
                    ],
                    type_meta::IR {
                        constraints: vec![],
                        streaming_behavior: Default::default(),
                    },
                ),
            },
            TestCase {
                name: "(A|B)@stream.done => (A@stream.done|B@stream.done)@stream.done",
                input: make_union(
                    vec![TypeIR::int(), TypeIR::float()],
                    type_meta::IR {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            done: true,
                            ..Default::default()
                        },
                    },
                ),
                expected: make_union(
                    vec![
                        TypeIR::int_with_meta(type_meta::IR {
                            constraints: vec![],
                            streaming_behavior: type_meta::base::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                        }),
                        TypeIR::float_with_meta(type_meta::IR {
                            constraints: vec![],
                            streaming_behavior: type_meta::base::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                        }),
                    ],
                    type_meta::IR {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            done: true,
                            ..Default::default()
                        },
                    },
                ),
            },
            TestCase {
                name: "(A|B)@stream.not_null => (A@stream.not_null|B@stream.not_null)@stream.not_null",
                input: make_union(vec![TypeIR::int(), TypeIR::string()], type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        needed: true,
                        ..Default::default()
                    },
                }),
                expected: make_union(vec![
                    TypeIR::int_with_meta(type_meta::IR {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            needed: true,
                            ..Default::default()
                        },
                    }),
                    TypeIR::string_with_meta(type_meta::IR {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            needed: true,
                            ..Default::default()
                        },
                    }),
                ], type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        needed: true,
                        ..Default::default()
                    },
                }),
            },
            TestCase {
                name: "(A|B)@stream.with_state => (A|B)@stream.with_state",
                input: make_union(vec![TypeIR::int(), TypeIR::string()], type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }),
                expected: make_union(vec![
                    TypeIR::int(),
                    TypeIR::string(),
                ], type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }),
            },
            TestCase{
                name: "(A@stream_with_state | B@stream_with_state) => (A@stream_with_state | B@stream_with_state)",
                input: make_union(vec![TypeIR::int_with_meta(type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }), TypeIR::string_with_meta(type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                })], Default::default()),
                expected: make_union(vec![TypeIR::int_with_meta(type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }), TypeIR::string_with_meta(type_meta::IR {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                })], Default::default())
            },
        ];

        for case in cases {
            let actual = case.input.simplify();
            assert_eq!(
                actual, case.expected,
                "\n\nFailed test: {}\nInput: {}\nActual: {}\nExpected: {}\n",
                case.name, case.input, actual, case.expected
            );
        }
    }

    // #[test]
    // fn simplify_union_constraints_convertered() {
    //     struct TestCase {
    //         name: &'static str,
    //         input: TypeIR,
    //         expected: TypeNonStreaming,
    //     }

    //     let constraint = Constraint::new_check("check all fields are positive", "{{ this }} > 0");
    //     let streaming_behavior = type_meta::base::StreamingBehavior::default();

    //     let cases = vec![
    //         TestCase {
    //             name: "(A|B)(@check(A, {..})) => (A@check(A, {..})|B@check(B, {..}))",
    //             input: make_union(
    //                 vec![TypeIR::int(), TypeIR::float()],
    //                 type_meta::NonStreaming {
    //                     constraints: vec![constraint.clone()],
    //                 },
    //             ),
    //             expected: make_union(
    //                 vec![
    //                     TypeIR::int_with_meta(type_meta::NonStreaming {
    //                         constraints: vec![constraint.clone()],
    //                     }),
    //                     TypeIR::float_with_meta(type_meta::NonStreaming {
    //                         constraints: vec![constraint.clone()],
    //                     }),
    //                 ],
    //                 type_meta::IR {
    //                     constraints: vec![],
    //                     streaming_behavior: Default::default(),
    //                 },
    //             ),
    //         },
    //         TestCase {
    //             name: "(A|B)@stream.done => (A@stream.done|B@stream.done)@stream.done",
    //             input: make_union(
    //                 vec![TypeIR::int(), TypeIR::float()],
    //                 type_meta::IR {
    //                     constraints: vec![],
    //                     streaming_behavior: type_meta::base::StreamingBehavior {
    //                         done: true,
    //                         ..Default::default()
    //                     },
    //                 },
    //             ),
    //             expected: make_union(
    //                 vec![
    //                     TypeIR::int_with_meta(type_meta::IR {
    //                         constraints: vec![],
    //                         streaming_behavior: type_meta::base::StreamingBehavior {
    //                             done: true,
    //                             ..Default::default()
    //                         },
    //                     }),
    //                     TypeIR::float_with_meta(type_meta::IR {
    //                         constraints: vec![],
    //                         streaming_behavior: type_meta::base::StreamingBehavior {
    //                             done: true,
    //                             ..Default::default()
    //                         },
    //                     }),
    //                 ],
    //                 type_meta::IR {
    //                     constraints: vec![],
    //                     streaming_behavior: type_meta::base::StreamingBehavior {
    //                         done: true,
    //                         ..Default::default()
    //                     },
    //                 },
    //             ),
    //         },
    //         TestCase {
    //             name: "(A|B)@stream.not_null => (A@stream.not_null|B@stream.not_null)@stream.not_null",
    //             input: make_union(vec![TypeIR::int(), TypeIR::string()], type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     needed: true,
    //                     ..Default::default()
    //                 },
    //             }),
    //             expected: make_union(vec![
    //                 TypeIR::int_with_meta(type_meta::IR {
    //                     constraints: vec![],
    //                     streaming_behavior: type_meta::base::StreamingBehavior {
    //                         needed: true,
    //                         ..Default::default()
    //                     },
    //                 }),
    //                 TypeIR::string_with_meta(type_meta::IR {
    //                     constraints: vec![],
    //                     streaming_behavior: type_meta::base::StreamingBehavior {
    //                         needed: true,
    //                         ..Default::default()
    //                     },
    //                 }),
    //             ], type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     needed: true,
    //                     ..Default::default()
    //                 },
    //             }),
    //         },
    //         TestCase {
    //             name: "(A|B)@stream.with_state => (A|B)@stream.with_state",
    //             input: make_union(vec![TypeIR::int(), TypeIR::string()], type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     state: true,
    //                     ..Default::default()
    //                 },
    //             }),
    //             expected: make_union(vec![
    //                 TypeIR::int(),
    //                 TypeIR::string(),
    //             ], type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     state: true,
    //                     ..Default::default()
    //                 },
    //             }),
    //         },
    //         TestCase{
    //             name: "(A@stream_with_state | B@stream_with_state) => (A@stream_with_state | B@stream_with_state)",
    //             input: make_union(vec![TypeIR::int_with_meta(type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     state: true,
    //                     ..Default::default()
    //                 },
    //             }), TypeIR::string_with_meta(type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     state: true,
    //                     ..Default::default()
    //                 },
    //             })], Default::default()),
    //             expected: make_union(vec![TypeIR::int_with_meta(type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     state: true,
    //                     ..Default::default()
    //                 },
    //             }), TypeIR::string_with_meta(type_meta::IR {
    //                 constraints: vec![],
    //                 streaming_behavior: type_meta::base::StreamingBehavior {
    //                     state: true,
    //                     ..Default::default()
    //                 },
    //             })], Default::default())
    //         },
    //     ];

    //     for case in cases {
    //         let actual = case.input.simplify();
    //         assert_eq!(
    //             actual, case.expected,
    //             "\n\nFailed test: {}\nInput: {}\nActual: {}\nExpected: {}\n",
    //             case.name, case.input, actual, case.expected
    //         );
    //     }
    // }

    #[test]
    fn flatten_base_case() {
        let null = TypeIR::null();
        assert_eq!(null.flatten(), vec![null])
    }

    #[test]
    fn flatten_int() {
        let int = TypeIR::int();
        assert_eq!(int.flatten(), vec![int])
    }

    #[test]
    fn flatten_optional_int() {
        let optional_int = TypeIR::optional(TypeIR::int());
        assert_eq!(optional_int.flatten(), vec![TypeIR::int(), TypeIR::null()])
    }

    #[test]
    // null => null
    fn partialize_base_case() {
        let null = TypeIR::null();
        assert_eq!(
            converters::streaming::from_type_ir(&null, &TestLookup),
            TypeStreaming::Primitive(TypeValue::Null, Default::default())
        );
    }

    #[test]
    fn partialize_primitive_with_streaming() {
        // int@stream.with_state => stream.int | null @stream.with_state @stream.not_null
        let int = TypeIR::int_with_meta(type_meta::IR {
            streaming_behavior: type_meta::base::StreamingBehavior {
                state: true,
                needed: false,
                done: false,
            },
            ..Default::default()
        });
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        type_meta::stream::TypeMetaStreaming {
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                state: false,
                                done: true,
                            },
                            ..Default::default()
                        },
                    ),
                    TypeStreaming::null(),
                ])
            },
            type_meta::stream::TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    state: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let actual = int.to_streaming_type(&TestLookup);
        assert_eq!(actual, expected, "{actual} != {expected}");
    }

    #[test]
    fn parialize_primitive_needed_field_with_streaming() {
        let int = TypeIR::int_with_meta(type_meta::IR {
            streaming_behavior: type_meta::base::StreamingBehavior {
                state: true,
                needed: true,
                done: false,
            },
            ..Default::default()
        });
        let expected = TypeStreaming::Primitive(
            TypeValue::Int,
            type_meta::stream::TypeMetaStreaming {
                constraints: vec![],
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    state: true,
                    done: true,
                },
            },
        );
        assert_eq!(int.to_streaming_type(&TestLookup), expected);
    }

    #[test]
    // Foo => stream.Foo | null
    fn partialize_bare_class() {
        let class = TypeIR::class("MyClass");
        assert_eq!(
            converters::streaming::from_type_ir(&class, &TestLookup),
            make_optional(TypeStreaming::Class {
                name: "MyClass".to_string(),
                dynamic: false,
                mode: StreamingMode::Streaming,
                meta: Default::default(),
            })
        );
    }

    #[test]
    fn streaming_type_roundtrip() {
        let class = TypeIR::union(vec![TypeIR::literal("ok"), TypeIR::literal("error")]);
        let streaming_type = class.to_streaming_type(&TestLookup);
        let again_class = streaming_type.to_ir_type();
        let again_streaming_type = again_class.to_streaming_type(&TestLookup);
        assert_eq!(streaming_type, again_streaming_type);
    }

    #[test]
    // Foo @stream.done => Foo
    fn partialize_class_with_done() {
        let mut class = TypeIR::class("MyClass");
        let expected = make_optional(TypeStreaming::Class {
            name: "MyClass".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        });
        class.meta_mut().streaming_behavior.done = true;
        assert_eq!(
            converters::streaming::from_type_ir(&class, &TestLookup),
            expected,
            "{class} != {expected}"
        );
    }

    #[test]
    fn partialize_simple_union() {
        let union = TypeIR::union(vec![TypeIR::int(), TypeIR::string()]);
        let expected = make_optional(TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        type_meta::stream::TypeMetaStreaming::default().done(),
                    ),
                    TypeStreaming::Primitive(
                        TypeValue::String,
                        type_meta::stream::TypeMetaStreaming::default(),
                    ),
                ])
            },
            Default::default(),
        ));
        let actual = converters::streaming::from_type_ir(&union, &TestLookup);
        assert_eq!(actual, expected, "actual: {actual}\nexpected: {expected}");
    }

    #[test]
    fn partialize_primitive_types() {
        // Test Float
        let float = TypeIR::float();
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Float,
                        type_meta::stream::TypeMetaStreaming {
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    ),
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );
        assert_eq!(float.to_streaming_type(&TestLookup), expected);

        // Test Bool
        let bool_type = TypeIR::bool();
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Bool,
                        type_meta::stream::TypeMetaStreaming::default().done(),
                    ),
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );
        assert_eq!(bool_type.to_streaming_type(&TestLookup), expected);
    }

    #[test]
    fn partialize_enum() {
        let enum_type = TypeIR::r#enum("MyEnum");
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Enum {
                        name: "MyEnum".to_string(),
                        dynamic: false,
                        meta: type_meta::stream::TypeMetaStreaming {
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    },
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );
        assert_eq!(enum_type.to_streaming_type(&TestLookup), expected);
    }

    #[test]
    fn partialize_literal() {
        let literal = TypeIR::literal("test");
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Literal(
                        LiteralValue::String("test".to_string()),
                        type_meta::stream::TypeMetaStreaming {
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    ),
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );
        let streaming_type = literal.to_streaming_type(&TestLookup);
        assert_eq!(streaming_type, expected, "{streaming_type} != {expected}");
    }

    #[test]
    fn partialize_recursive_type_alias() {
        let alias = TypeIR::recursive_type_alias("MyAlias");
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::RecursiveTypeAlias {
                        name: "MyAlias".to_string(),
                        mode: StreamingMode::Streaming,
                        meta: Default::default(),
                    },
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );
        assert_eq!(alias.to_streaming_type(&TestLookup), expected);
    }

    #[test]
    fn partialize_mixed_done_union() {
        let mut done_variant = TypeIR::class("FooDone");
        done_variant.meta_mut().streaming_behavior.done = true;

        let streamable_variant = TypeIR::class("MessageToUser");
        let union = TypeIR::Union(
            unsafe { UnionTypeGeneric::new_unsafe(vec![done_variant, streamable_variant]) },
            Default::default(),
        );
        let streaming_type = union.to_streaming_type(&TestLookup);
        let streaming_type_variants: Vec<TypeStreaming> = match streaming_type {
            TypeStreaming::Union(union, _) => union.view().flatten(),
            _ => panic!("Expected union"),
        };
        assert_eq!(streaming_type_variants.len(), 3);

        let mut expected_first_variant = TypeStreaming::class("FooDone");
        expected_first_variant.meta_mut().streaming_behavior.done = true;

        let expected_second_variant = TypeStreaming::Class {
            name: "MessageToUser".to_string(),
            mode: StreamingMode::Streaming,
            dynamic: false,
            meta: Default::default(),
        };

        dbg!(&streaming_type_variants[0]);
        dbg!(&streaming_type_variants[1]);
        dbg!(&streaming_type_variants[2]);
        assert_eq!(streaming_type_variants[0], expected_first_variant);
        assert_eq!(streaming_type_variants[1], expected_second_variant);
    }

    #[test]
    fn partialize_checked_optional_int() {
        // int? @check("foo", "bar")
        // represented as Union(Int, Null) with constraints.
        let constraint = Constraint::new_check("foo", "bar");
        let mut optional_int = TypeIR::union(vec![TypeIR::int(), TypeIR::null()]);
        optional_int.set_meta(type_meta::IR {
            constraints: vec![constraint.clone()],
            streaming_behavior: Default::default(),
        });

        // Expected streaming type:
        // Union(stream(Int), Null) with constraints.
        // The outer null is collapsed into the inner null.
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    // Inner int is marked needed=true because it was inside a union
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        TypeMetaStreaming {
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    ),
                    TypeStreaming::null(),
                ])
            },
            TypeMetaStreaming {
                constraints: vec![constraint],
                streaming_behavior: Default::default(),
            },
        );

        let actual = converters::streaming::from_type_ir(&optional_int, &TestLookup);
        assert_eq!(
            actual, expected,
            "\nActual: {}\nExpected: {}",
            actual, expected
        );
    }

    #[test]
    fn partialize_checked_int() {
        // Case: int @check(..)
        let constraint = Constraint::new_check("foo", "bar");
        let mut input = TypeIR::int();
        input.set_meta(type_meta::IR {
            constraints: vec![constraint.clone()],
            streaming_behavior: Default::default(),
        });

        // Non-streaming: Just the int with the check.
        let expected_non_streaming = TypeNonStreaming::Primitive(
            TypeValue::Int,
            type_meta::NonStreaming {
                constraints: vec![constraint.clone()],
            },
        );

        // Streaming: int becomes Union(int, null).
        // The int keeps the check.
        let expected_streaming = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        type_meta::stream::TypeMetaStreaming {
                            constraints: vec![constraint.clone()],
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                        },
                    ),
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );

        let actual_non_streaming = input.to_non_streaming_type(&TestLookup);
        assert_eq!(actual_non_streaming.to_string(), "int @check(foo, {{..}} )");
        assert_eq!(
            actual_non_streaming, expected_non_streaming,
            "Non-streaming mismatch"
        );

        let actual_streaming = input.to_streaming_type(&TestLookup);
        assert_eq!(
            actual_streaming.to_string(),
            "(int @check(foo, {{..}} ) @stream.done | null)"
        );
        assert_eq!(actual_streaming, expected_streaming, "Streaming mismatch");
    }

    #[test]
    fn partialize_checked_union_on_union() {
        // Case: (int | null) @check(..)
        let constraint = Constraint::new_check("foo", "bar");
        let mut input = TypeIR::union(vec![TypeIR::int(), TypeIR::null()]);
        input.set_meta(type_meta::IR {
            constraints: vec![constraint.clone()],
            streaming_behavior: Default::default(),
        });

        let expected_non_streaming = TypeNonStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeNonStreaming::Primitive(TypeValue::Int, Default::default()),
                    TypeNonStreaming::Primitive(TypeValue::Null, Default::default()),
                ])
            },
            type_meta::NonStreaming {
                constraints: vec![constraint.clone()],
            },
        );

        let expected_streaming = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        type_meta::stream::TypeMetaStreaming::default().done(),
                    ),
                    TypeStreaming::null(),
                ])
            },
            type_meta::stream::TypeMetaStreaming {
                constraints: vec![constraint.clone()],
                streaming_behavior: Default::default(),
            },
        );

        let actual_non_streaming = input.to_non_streaming_type(&TestLookup);
        assert_eq!(
            actual_non_streaming.to_string(),
            "(int | null) @check(foo, {{..}} )"
        );
        assert_eq!(
            actual_non_streaming, expected_non_streaming,
            "Non-streaming mismatch"
        );

        let actual_streaming = input.to_streaming_type(&TestLookup);
        assert_eq!(
            actual_streaming.to_string(),
            "(int @stream.done | null) @check(foo, {{..}} )"
        );
        assert_eq!(actual_streaming, expected_streaming, "Streaming mismatch");
    }

    #[test]
    fn partialize_checked_union_on_variant() {
        // Case: (int @check(..)) | null
        let constraint = Constraint::new_check("foo", "bar");
        let mut int = TypeIR::int();
        int.set_meta(type_meta::IR {
            constraints: vec![constraint.clone()],
            streaming_behavior: Default::default(),
        });
        let input = TypeIR::union(vec![int, TypeIR::null()]);

        let int_non_streaming = TypeNonStreaming::Primitive(
            TypeValue::Int,
            type_meta::NonStreaming {
                constraints: vec![constraint.clone()],
            },
        );
        let expected_non_streaming = TypeNonStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    int_non_streaming,
                    TypeNonStreaming::Primitive(TypeValue::Null, Default::default()),
                ])
            },
            Default::default(),
        );

        let expected_streaming = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        type_meta::stream::TypeMetaStreaming {
                            constraints: vec![constraint.clone()],
                            streaming_behavior: type_meta::stream::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                        },
                    ),
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );

        let actual_non_streaming = input.to_non_streaming_type(&TestLookup);
        assert_eq!(
            actual_non_streaming.to_string(),
            "(int @check(foo, {{..}} ) | null)"
        );
        assert_eq!(
            actual_non_streaming, expected_non_streaming,
            "Non-streaming mismatch"
        );

        let actual_streaming = input.to_streaming_type(&TestLookup);
        assert_eq!(
            actual_streaming.to_string(),
            "(int @check(foo, {{..}} ) @stream.done | null)"
        );
        assert_eq!(actual_streaming, expected_streaming, "Streaming mismatch");
    }

    #[test]
    fn partialize_checked_union_on_null_variant() {
        // Case: int | (null @check(..))
        let constraint = Constraint::new_check("foo", "bar");
        let mut null = TypeIR::null();
        null.set_meta(type_meta::IR {
            constraints: vec![constraint.clone()],
            streaming_behavior: Default::default(),
        });
        let input = TypeIR::union(vec![TypeIR::int(), null]);

        let expected_non_streaming = {
            // NOTE: checks on null are currently lost in non-streaming conversion as well
            let null = TypeNonStreaming::Primitive(TypeValue::Null, Default::default());
            TypeNonStreaming::Union(
                unsafe {
                    UnionTypeGeneric::new_unsafe(vec![
                        TypeNonStreaming::Primitive(TypeValue::Int, Default::default()),
                        null,
                    ])
                },
                Default::default(),
            )
        };

        let expected_streaming = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Primitive(
                        TypeValue::Int,
                        type_meta::stream::TypeMetaStreaming::default().done(),
                    ),
                    // NOTE: checks on null are lost in streaming conversion due to iter_skip_null usage
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );

        let actual_non_streaming = input.to_non_streaming_type(&TestLookup);
        assert_eq!(actual_non_streaming.to_string(), "(int | null)");
        assert_eq!(
            actual_non_streaming, expected_non_streaming,
            "Non-streaming mismatch"
        );

        let actual_streaming = input.to_streaming_type(&TestLookup);
        assert_eq!(actual_streaming.to_string(), "(int @stream.done | null)");
        assert_eq!(actual_streaming, expected_streaming, "Streaming mismatch");
    }
}
