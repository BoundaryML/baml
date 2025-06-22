use std::collections::HashSet;

use indexmap::IndexSet;
use itertools::Itertools;

use crate::{baml_value::TypeLookups, BamlMediaType, ConstraintLevel};

mod builder;
mod display;
pub mod type_meta;
mod union_type;

// Types, depending on the context, have different metadata attached to them.
// When you define a type in BAML you have the IR rep of the type.
// Sometimes you use them in streaming or nonstreaming contexts.
/// The building block of IR types in BAML.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Eq, Hash)]
pub enum TypeGeneric<T> {
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
        meta: T,
    },
    Tuple(Vec<TypeGeneric<T>>, T),
    Arrow(Box<ArrowGeneric<T>>, T),
    Union(UnionTypeGeneric<T>, T),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
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
pub type Type = TypeGeneric<type_meta::Base>;
pub type FieldType = Type;
pub type TypeStreaming = TypeGeneric<type_meta::Streaming>;

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
    pub fn literal_base_type(&self) -> FieldType {
        match self {
            Self::String(_) => FieldType::string(),
            Self::Int(_) => FieldType::int(),
            Self::Bool(_) => FieldType::bool(),
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
    types: Vec<FieldType>,
}

/// A union type may never hold more than 1 null
/// A view into a union type that classifies its variants
#[derive(Debug)]
pub enum UnionTypeView<'a> {
    /// A union containing only the null type
    Null,
    /// A union containing exactly one non-null type and the null type
    Optional(&'a FieldType),
    /// A union containing multiple non-null types with no optional variants
    OneOf(Vec<&'a FieldType>),
    /// A union containing multiple types where at least one is optional
    OneOfOptional(Vec<&'a FieldType>),
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

impl<'a, T: Default + std::fmt::Debug + Clone> UnionTypeViewGeneric<'a, T> {
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

    pub fn view<'a>(&'a self) -> UnionTypeViewGeneric<'a, T> {
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

    fn view_mut<'a>(&'a mut self) -> UnionTypeViewGenericMut<'a, T> {
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

pub trait HasFieldType {
    fn field_type(&self) -> &FieldType;
}

impl HasFieldType for FieldType {
    fn field_type(&self) -> &FieldType {
        self
    }
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Arrow {
    pub param_types: Vec<FieldType>,
    pub return_type: FieldType,
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
    pub fn flatten(&self) -> Vec<TypeGeneric<T>>
    where
        T: Clone + std::fmt::Debug + Default,
    {
        match self {
            TypeGeneric::Union(inner, _) => inner.view().flatten(),
            _ => vec![self.clone()],
        }
    }

    pub fn find_if<'a>(
        &'a self,
        predicate: &impl Fn(&TypeGeneric<T>) -> bool,
    ) -> Vec<&'a TypeGeneric<T>> {
        if predicate(self) {
            return vec![self];
        }

        match self {
            TypeGeneric::Primitive(..)
            | TypeGeneric::Enum { .. }
            | TypeGeneric::Literal(..)
            | TypeGeneric::Class { .. }
            | TypeGeneric::RecursiveTypeAlias { .. } => vec![],
            TypeGeneric::List(inner, _) => inner.find_if(predicate),
            TypeGeneric::Map(type_generic, type_generic1, _) => {
                let mut res = type_generic.find_if(predicate);
                res.extend(type_generic1.find_if(predicate));
                res
            }
            TypeGeneric::Tuple(type_generics, _) => type_generics
                .iter()
                .flat_map(|t| t.find_if(predicate))
                .collect(),
            TypeGeneric::Union(union_type_generic, _) => union_type_generic
                .iter_skip_null()
                .iter()
                .flat_map(|t| t.find_if(predicate))
                .collect(),
            TypeGeneric::Arrow(arrow_generic, _) => {
                let res = arrow_generic
                    .param_types
                    .iter()
                    .flat_map(|t| t.find_if(predicate));
                let mut returned = arrow_generic.return_type.find_if(predicate);
                returned.extend(res);
                returned
            }
        }
    }

    pub fn set_meta(&mut self, meta: T) {
        match self {
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

    pub fn meta_mut(&mut self) -> &mut T {
        match self {
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

    /// For types that are unions, replace the variants list with
    /// a simplified (flattened) variants list.
    pub fn is_primitive(&self) -> bool {
        match self {
            TypeGeneric::Primitive(_, _) => true,
            TypeGeneric::List(t, _) => t.is_primitive(),
            _ => false,
        }
    }

    pub fn is_optional(&self) -> bool
    where
        T: std::fmt::Debug + Default,
    {
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
                TypeGeneric::Primitive(_, _) | TypeGeneric::Literal(_, _) => {}
            }
        }
        deps
    }
}

impl Type {
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
    pub fn partialize(&self, lookup: &impl TypeLookups) -> TypeStreaming {
        partialize(self, lookup)
    }
}

fn partialize(r#type: &Type, lookup: &impl TypeLookups) -> TypeStreaming {
    // This inner worker function goes from `FieldType` to `FieldType` to be
    // suitable for recursive use. We only wrap the outermost `FieldType` in
    // `StreamingType`.
    fn partialize_helper(r#type: &Type, lookup: &impl TypeLookups) -> TypeStreaming {
        let type_meta::base::StreamingBehavior {
            done,
            needed,
            state,
        } = r#type
            .streaming_behavior()
            .combine(&inherent_streaming_behavior(r#type, lookup));

        // A copy of the metadata to use in the new type.
        let meta = type_meta::Streaming {
            streaming_behavior: type_meta::stream::StreamingBehavior { done, state },
            constraints: r#type.meta().constraints.clone(),
        };

        // Streaming behavior of the type, without regard to the `@stream` annotations.
        // (That annotation will be handled later in this function).
        let mut base_type_streaming = match r#type {
            FieldType::Primitive(type_value, _) => match type_value {
                TypeValue::Null => TypeStreaming::Primitive(TypeValue::Null, meta),
                TypeValue::Int => TypeStreaming::Primitive(TypeValue::Int, meta),
                TypeValue::Float => TypeStreaming::Primitive(TypeValue::Float, meta),
                TypeValue::Bool => TypeStreaming::Primitive(TypeValue::Bool, meta),
                TypeValue::String => TypeStreaming::Primitive(TypeValue::String, meta),
                TypeValue::Media(_) => {
                    TypeStreaming::Primitive(TypeValue::Media(BamlMediaType::Image), meta)
                }
            },
            FieldType::Enum { name, dynamic, .. } => TypeStreaming::Enum {
                name: name.clone(),
                dynamic: *dynamic,
                meta: meta.clone(),
            },
            FieldType::Literal(literal_value, _) => {
                TypeStreaming::Literal(literal_value.clone(), meta)
            }
            FieldType::Class { name, dynamic, .. } => TypeStreaming::Class {
                name: name.clone(),
                mode: if done {
                    StreamingMode::NonStreaming
                } else {
                    StreamingMode::Streaming
                },
                dynamic: *dynamic,
                meta: meta.clone(),
            },
            FieldType::List(item_type, _) => {
                TypeStreaming::List(Box::new(partialize(item_type, lookup)), meta)
            }
            FieldType::Map(key_type, item_type, _) => TypeStreaming::Map(
                {
                    // Keys cannot be null in maps
                    let mut clone = key_type.clone();
                    clone.meta_mut().streaming_behavior.needed = true;
                    Box::new(partialize(&clone, lookup))
                },
                Box::new(partialize(item_type, lookup)),
                meta,
            ),
            FieldType::RecursiveTypeAlias { name, .. } => TypeStreaming::RecursiveTypeAlias {
                name: name.clone(),
                meta: meta.clone(),
            },
            FieldType::Tuple(field_types, _) => TypeStreaming::Tuple(
                field_types.iter().map(|t| partialize(t, lookup)).collect(),
                meta,
            ),
            FieldType::Arrow(arrow, _) => TypeStreaming::Arrow(
                Box::new(ArrowGeneric {
                    param_types: arrow
                        .param_types
                        .iter()
                        .map(|t| partialize(t, lookup))
                        .collect(),
                    return_type: partialize(&arrow.return_type, lookup),
                }),
                meta,
            ),
            FieldType::Union(union_type, _) => {
                let variants = union_type.iter_skip_null();
                let variants = variants.into_iter().cloned().map(|mut t| {
                    t.meta_mut().streaming_behavior.needed = true;
                    partialize(&t, lookup)
                });

                let variants = if !needed {
                    variants
                        .chain(std::iter::once(TypeStreaming::null()))
                        .collect()
                } else {
                    variants.collect()
                };
                TypeStreaming::Union(unsafe { UnionTypeGeneric::new_unsafe(variants) }, meta)
            }
        };
        if needed || base_type_streaming.is_optional() {
            // Needed streaming types, and streaming types that are optional, need
            // no further processing to add optionality.
            base_type_streaming
        } else {
            // Currently base_type_streaming has the interesting metadata.
            // In the union we create to make base_type_streaming optional,
            // we want that inner metadata to be default, our outer union to
            // have the metadata. So we create a new default metadata and swap
            // its memory with that of the inner base_type.
            let meta = base_type_streaming.meta().clone();
            *base_type_streaming.meta_mut() = Default::default();
            let mut optional_value = TypeStreaming::Union(
                unsafe {
                    UnionTypeGeneric::new_unsafe(vec![base_type_streaming, TypeStreaming::null()])
                },
                Default::default(),
            );
            *optional_value.meta_mut() = meta;
            optional_value
        }
    }

    // Types have inherent streaming behavior. For example literals and
    // numbers are inherently @done. These behaviors are applied even
    // without user annotations.
    fn inherent_streaming_behavior(
        field_type: &FieldType,
        lookup: &impl TypeLookups,
    ) -> type_meta::base::StreamingBehavior {
        type StreamingBehavior = type_meta::base::StreamingBehavior;
        match field_type {
            FieldType::Primitive(type_value, _) => match type_value {
                TypeValue::Bool | TypeValue::Float | TypeValue::Int => StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                TypeValue::String | TypeValue::Null | TypeValue::Media(_) => Default::default(),
            },
            FieldType::Enum { .. } | FieldType::Literal(_, _) => StreamingBehavior {
                done: true,
                ..Default::default()
            },
            FieldType::RecursiveTypeAlias { name, .. } => {
                match lookup.expand_recursive_type(name) {
                    Ok(expansion) if expansion.is_optional() => StreamingBehavior {
                        needed: true,
                        ..Default::default()
                    },
                    _ => Default::default(),
                }
            }
            FieldType::Class { .. }
            | FieldType::List(..)
            | FieldType::Map(..)
            | FieldType::Tuple(..)
            | FieldType::Arrow(..)
            | FieldType::Union(..) => Default::default(),
        }
    }
    partialize_helper(r#type, lookup)
}

impl TypeGeneric<type_meta::Base> {
    pub fn streaming_behavior(&self) -> &type_meta::base::StreamingBehavior {
        &self.meta().streaming_behavior
    }

    pub fn simplify(&self) -> Self {
        match self {
            TypeGeneric::Union(inner, union_meta) => {
                let view = inner.view();
                let flattened = view.flatten();
                let unique = flattened.into_iter().unique().collect::<Vec<_>>();
                let has_null = unique.contains(&TypeGeneric::null());
                // if the union contains null, we'll detect that here.
                let mut variants: Vec<TypeGeneric<type_meta::Base>> = unique
                    .into_iter()
                    .filter(|t| t != &TypeGeneric::null())
                    .collect::<Vec<_>>();

                // here metadata simplification of both variants and the union itself happens
                // unions will never have checks and asserts in their own metadata, always distributed and do not keep
                // Union(A|B)(@check(A, {..})) => Union(A@check(A, {..})|B@check(B, {..}))
                let (to_move, to_keep): (Vec<_>, Vec<_>) =
                    union_meta.constraints.clone().into_iter().partition(|c| {
                        // move these
                        matches!(c.level, ConstraintLevel::Check | ConstraintLevel::Assert)
                    });

                let type_meta::base::StreamingBehavior {
                    done,
                    needed,
                    state,
                } = union_meta.streaming_behavior;

                // Add to_move to each variant
                for variant in variants.iter_mut() {
                    variant.meta_mut().constraints.extend(to_move.clone());
                    if done {
                        variant.meta_mut().streaming_behavior.done = true;
                    }
                    if needed {
                        variant.meta_mut().streaming_behavior.needed = true;
                    }
                }

                let mut new_meta = type_meta::Base::default();
                new_meta.constraints.extend(to_keep);

                if needed {
                    new_meta.streaming_behavior.needed = true;
                }
                new_meta.streaming_behavior.state = state;
                new_meta.streaming_behavior.done = done;

                let simplified: TypeGeneric<type_meta::Base> = match variants.len() {
                    0 => return TypeGeneric::null(),
                    1 => {
                        if has_null {
                            // Return an optional of a single variant.
                            TypeGeneric::Union(
                                unsafe { UnionTypeGeneric::new_unsafe(vec![variants[0].clone()]) },
                                new_meta,
                            )
                        } else {
                            // Return the single variant.
                            variants[0].clone()
                        }
                    }
                    _ => {
                        if has_null {
                            variants.push(TypeGeneric::null());
                        }
                        TypeGeneric::Union(
                            unsafe { UnionTypeGeneric::new_unsafe(variants) },
                            new_meta,
                        )
                    }
                };

                simplified
            }
            _ => self.clone(),
        }
    }
}

pub trait ToUnionName<T> {
    fn to_union_name(&self) -> String;
    fn find_union_types(&self) -> IndexSet<&TypeGeneric<T>>;
}

impl<Meta: std::hash::Hash + std::cmp::Eq> ToUnionName<Meta> for TypeGeneric<Meta> {
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
            T::Primitive(_, _)
            | T::Enum { .. }
            | T::Literal(_, _)
            | T::Class { .. }
            | T::RecursiveTypeAlias { .. }
            | T::Arrow(_, _) => IndexSet::new(),
            T::Tuple(inner, _) => inner.iter().flat_map(|t| t.find_union_types()).collect(),
        }
    }

    fn to_union_name(&self) -> String {
        use TypeGeneric as T;
        match self {
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
                LiteralValue::Int(val) => format!("int_{}", val),
                LiteralValue::Bool(val) => format!("bool_{}", val),
            },
            T::Class { name, .. } => name.to_string(),
            T::List(field_type, _) => {
                format!("List__{}", field_type.to_union_name())
            }
            T::Map(field_type, field_type1, _) => {
                format!(
                    "Map__{}_{}",
                    field_type.to_union_name(),
                    field_type1.to_union_name()
                )
            }
            T::Union(field_types, _) => match field_types.view() {
                UnionTypeViewGeneric::Null => "null".to_string(),
                UnionTypeViewGeneric::Optional(field_type) => field_type.to_union_name(),
                UnionTypeViewGeneric::OneOf(field_types)
                | UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    format!(
                        "Union__{}",
                        field_types
                            .iter()
                            .map(|t| t.to_union_name())
                            .sorted()
                            .collect::<Vec<_>>()
                            .join("__")
                    )
                }
            },
            T::Tuple(field_types, _) => format!(
                "Tuple__{}",
                field_types
                    .iter()
                    .map(|v| v.to_union_name())
                    .collect::<Vec<_>>()
                    .join("__")
            ),
            T::RecursiveTypeAlias { name, .. } => name.to_string(),
            T::Arrow(_, _) => "function".to_string(),
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
    use crate::Constraint;

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

    fn make_union(types: Vec<FieldType>, meta: type_meta::Base) -> FieldType {
        FieldType::Union(unsafe { UnionTypeGeneric::new_unsafe(types) }, meta)
    }

    #[test]
    fn simplify_base_case() {
        assert_eq!(FieldType::null().simplify(), FieldType::null());
    }

    #[test]
    fn simplify_int() {
        let int = FieldType::int();
        assert_eq!(int.simplify(), int);
    }

    #[test]
    fn simplify_optional_int() {
        let optional_int = FieldType::optional(FieldType::int());
        assert_eq!(optional_int.simplify(), optional_int);
    }

    #[test]
    fn simplify_nested_unions() {
        // ((int | null) | string)
        let inner_union = FieldType::union(vec![FieldType::int(), FieldType::null()]);
        let outer_union = FieldType::union(vec![inner_union, FieldType::string()]);
        // union(union(int, null), string)
        assert_eq!(
            outer_union.simplify(),
            FieldType::union(vec![
                FieldType::int(),
                FieldType::string(),
                FieldType::null()
            ])
        );
    }

    #[test]
    fn simplify_repeated_variants() {
        let union = FieldType::union(vec![
            FieldType::int(),
            FieldType::int(),
            FieldType::string(),
            FieldType::string(),
        ]);
        assert_eq!(
            union.simplify(),
            FieldType::union(vec![FieldType::int(), FieldType::string()])
        );
    }

    #[test]
    fn simplify_nested_with_repeats() {
        let inner_union = FieldType::union(vec![FieldType::int(), FieldType::null()]);
        let union = FieldType::union(vec![FieldType::int(), inner_union, FieldType::string()]);
        assert_eq!(
            union.simplify(),
            FieldType::union(vec![
                FieldType::int(),
                FieldType::string(),
                FieldType::null()
            ])
        );
    }

    struct TestLookup;

    impl TypeLookups for TestLookup {
        fn expand_recursive_type(&self, _name: &str) -> anyhow::Result<&FieldType> {
            anyhow::bail!("nothing found");
        }
    }

    #[test]
    fn simplify_union_constraints() {
        struct TestCase {
            name: &'static str,
            input: FieldType,
            expected: FieldType,
        }

        let constraint = Constraint::new_check("check all fields are positive", "{{ this }} > 0");
        let streaming_behavior = type_meta::base::StreamingBehavior::default();

        let cases = vec![
            TestCase {
                name: "(A|B)(@check(A, {..})) => (A@check(A, {..})|B@check(B, {..}))",
                input: make_union(
                    vec![FieldType::int(), FieldType::float()],
                    type_meta::Base {
                        constraints: vec![constraint.clone()],
                        streaming_behavior: streaming_behavior.clone(),
                    },
                ),
                expected: make_union(
                    vec![
                        FieldType::int_with_meta(type_meta::Base {
                            constraints: vec![constraint.clone()],
                            streaming_behavior: Default::default(),
                        }),
                        FieldType::float_with_meta(type_meta::Base {
                            constraints: vec![constraint.clone()],
                            streaming_behavior: Default::default(),
                        }),
                    ],
                    type_meta::Base {
                        constraints: vec![],
                        streaming_behavior: Default::default(),
                    },
                ),
            },
            TestCase {
                name: "(A|B)@stream.done => (A@stream.done|B@stream.done)@stream.done",
                input: make_union(
                    vec![FieldType::int(), FieldType::float()],
                    type_meta::Base {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            done: true,
                            ..Default::default()
                        },
                    },
                ),
                expected: make_union(
                    vec![
                        FieldType::int_with_meta(type_meta::Base {
                            constraints: vec![],
                            streaming_behavior: type_meta::base::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                        }),
                        FieldType::float_with_meta(type_meta::Base {
                            constraints: vec![],
                            streaming_behavior: type_meta::base::StreamingBehavior {
                                done: true,
                                ..Default::default()
                            },
                        }),
                    ],
                    type_meta::Base {
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
                input: make_union(vec![FieldType::int(), FieldType::string()], type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        needed: true,
                        ..Default::default()
                    },
                }),
                expected: make_union(vec![
                    FieldType::int_with_meta(type_meta::Base {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            needed: true,
                            ..Default::default()
                        },
                    }),
                    FieldType::string_with_meta(type_meta::Base {
                        constraints: vec![],
                        streaming_behavior: type_meta::base::StreamingBehavior {
                            needed: true,
                            ..Default::default()
                        },
                    }),
                ], type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        needed: true,
                        ..Default::default()
                    },
                }),
            },
            TestCase {
                name: "(A|B)@stream.with_state => (A|B)@stream.with_state",
                input: make_union(vec![FieldType::int(), FieldType::string()], type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }),
                expected: make_union(vec![
                    FieldType::int(),
                    FieldType::string(),
                ], type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }),
            },
            TestCase{
                name: "(A@stream_with_state | B@stream_with_state) => (A@stream_with_state | B@stream_with_state)",
                input: make_union(vec![FieldType::int_with_meta(type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }), FieldType::string_with_meta(type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                })], Default::default()),
                expected: make_union(vec![FieldType::int_with_meta(type_meta::Base {
                    constraints: vec![],
                    streaming_behavior: type_meta::base::StreamingBehavior {
                        state: true,
                        ..Default::default()
                    },
                }), FieldType::string_with_meta(type_meta::Base {
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

    #[test]
    fn flatten_base_case() {
        let null = FieldType::null();
        assert_eq!(null.flatten(), vec![null])
    }

    #[test]
    fn flatten_int() {
        let int = FieldType::int();
        assert_eq!(int.flatten(), vec![int])
    }

    #[test]
    fn flatten_optional_int() {
        let optional_int = FieldType::optional(FieldType::int());
        assert_eq!(
            optional_int.flatten(),
            vec![FieldType::int(), FieldType::null()]
        )
    }

    #[test]
    // null => null
    fn partialize_base_case() {
        let null = FieldType::null();
        assert_eq!(
            partialize(&null, &TestLookup),
            TypeStreaming::Primitive(TypeValue::Null, Default::default())
        );
    }

    #[test]
    fn partialize_primitive_with_streaming() {
        // int@stream.with_state => stream.int | null @stream.with_state @stream.not_null
        let int = FieldType::int_with_meta(type_meta::Base {
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
                        type_meta::stream::TypeMetaStreaming::default(),
                    ),
                    TypeStreaming::null(),
                ])
            },
            type_meta::stream::TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    state: true,
                    done: true,
                },
                ..Default::default()
            },
        );
        assert_eq!(int.partialize(&TestLookup), expected);
    }

    #[test]
    fn parialize_primitive_needed_field_with_streaming() {
        let int = FieldType::int_with_meta(type_meta::Base {
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
        assert_eq!(int.partialize(&TestLookup), expected);
    }

    #[test]
    // Foo => stream.Foo | null
    fn partialize_bare_class() {
        let class = FieldType::class("MyClass");
        assert_eq!(
            partialize(&class, &TestLookup),
            make_optional(TypeStreaming::Class {
                name: "MyClass".to_string(),
                dynamic: false,
                mode: StreamingMode::Streaming,
                meta: Default::default(),
            })
        );
    }

    #[test]
    // Foo @stream.done => Foo
    fn partialize_class_with_done() {
        let mut class = FieldType::class("MyClass");
        let mut expected = make_optional(TypeStreaming::Class {
            name: "MyClass".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: Default::default(),
        });
        expected.meta_mut().streaming_behavior.done = true;
        class.meta_mut().streaming_behavior.done = true;
        assert_eq!(partialize(&class, &TestLookup), expected);
    }

    #[test]
    fn partialize_simple_union() {
        let union = FieldType::union(vec![FieldType::int(), FieldType::string()]);
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
        let actual = partialize(&union, &TestLookup);

        assert_eq!(actual, expected);
    }

    #[test]
    fn partialize_primitive_types() {
        // Test Float
        let float = FieldType::float();
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![TypeStreaming::float(), TypeStreaming::null()])
            },
            type_meta::stream::TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(float.partialize(&TestLookup), expected);

        // Test Bool
        let bool_type = FieldType::bool();
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![TypeStreaming::bool(), TypeStreaming::null()])
            },
            type_meta::stream::TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(bool_type.partialize(&TestLookup), expected);
    }

    #[test]
    fn partialize_enum() {
        let enum_type = FieldType::r#enum("MyEnum");
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Enum {
                        name: "MyEnum".to_string(),
                        dynamic: false,
                        meta: Default::default(),
                    },
                    TypeStreaming::null(),
                ])
            },
            type_meta::stream::TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(enum_type.partialize(&TestLookup), expected);
    }

    #[test]
    fn partialize_literal() {
        let literal = FieldType::literal("test");
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::Literal(
                        LiteralValue::String("test".to_string()),
                        Default::default(),
                    ),
                    TypeStreaming::null(),
                ])
            },
            type_meta::stream::TypeMetaStreaming {
                streaming_behavior: type_meta::stream::StreamingBehavior {
                    done: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(literal.partialize(&TestLookup), expected);
    }

    #[test]
    fn partialize_recursive_type_alias() {
        let alias = FieldType::recursive_type_alias("MyAlias");
        let expected = TypeStreaming::Union(
            unsafe {
                UnionTypeGeneric::new_unsafe(vec![
                    TypeStreaming::RecursiveTypeAlias {
                        name: "MyAlias".to_string(),
                        meta: Default::default(),
                    },
                    TypeStreaming::null(),
                ])
            },
            Default::default(),
        );
        assert_eq!(alias.partialize(&TestLookup), expected);
    }
}
