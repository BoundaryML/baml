use crate::BamlMediaType;
use crate::Constraint;
use crate::ConstraintLevel;
use indexmap::IndexSet;
use itertools::Itertools;
use std::collections::HashSet;

mod builder;

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
    RecursiveTypeAlias(String, T),
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
}

/// A convenience type alias for BAML types in the IR.
pub type Type = TypeGeneric<TypeMeta>;
pub type FieldType = Type;
pub type TypeStreaming = TypeGeneric<TypeMetaStreaming>;

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

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeMeta {
    pub constraints: Vec<Constraint>,
    pub streaming_behavior: StreamingBehavior,
}

impl Default for TypeMeta {
    fn default() -> Self {
        TypeMeta {
            constraints: Vec::new(),
            streaming_behavior: StreamingBehavior::default(),
        }
    }
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeMetaStreaming {
    pub constraints: Vec<Constraint>,
    pub streaming_behavior: StreamingBehavior,
}

impl Default for TypeMetaStreaming {
    fn default() -> Self {
        TypeMetaStreaming {
            constraints: Vec::new(),
            streaming_behavior: StreamingBehavior::default(),
        }
    }
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

// impl UnionType {
//     // disallow construction so people have to use:
//     // FieldType::union(vec![...]) which does a simplify() default
//     pub(crate) fn new(types: Vec<FieldType>) -> Self {
//         if types.len() <= 1 {
//             panic!(
//                 "FATAL, please report this bug: Union type must have at least 2 types. Got {:?}",
//                 types
//             );
//         }
//         Self { types }
//     }
//
//     pub fn is_optional(&self) -> bool {
//         self.types.iter().any(|t| t.is_optional())
//     }
//
//     pub fn add_type(&mut self, t: FieldType) {
//         self.types.push(t);
//     }
//
//     pub fn view(self) -> UnionTypeView> {
//         let non_null_types = self
//             .types
//             .iter()
//             .filter(|t| !t.is_null())
//             .collect::<Vec<_>>();
//         match non_null_types.len() {
//             0 => UnionTypeView::Null,
//             1 => UnionTypeView::Optional(&non_null_types[0]),
//             _ => {
//                 if non_null_types.len() == self.types.len() {
//                     UnionTypeView::OneOf(non_null_types)
//                 } else {
//                     UnionTypeView::OneOfOptional(non_null_types)
//                 }
//             }
//         }
//     }
//
//     // Hello
//     pub fn view_as_iter(&self, include_null: bool) -> (Vec<FieldType>, bool) {
//         match self.view() {
//             UnionTypeView::Null => (
//                 if include_null {
//                     vec![TypeGeneric::null()]
//                 } else {
//                     vec![]
//                 },
//                 true,
//             ),
//             UnionTypeView::Optional(field_type) => {
//                 if include_null {
//                     (vec![field_type.clone(), TypeGeneric::null()], true)
//                 } else {
//                     (vec![field_type.clone()], true)
//                 }
//             }
//             UnionTypeView::OneOf(items) => (items, false),
//             UnionTypeView::OneOfOptional(items) => {
//                 let null = TypeGeneric::null();
//                 if include_null {
//                     (
//                         items.into_iter().chain(std::iter::once(&null)).collect(),
//                         true,
//                     )
//                 } else {
//                     (items, true)
//                 }
//             }
//         }
//     }
// }

impl<T: std::fmt::Debug + Default> UnionTypeGeneric<T> {
    // disallow construction so people have to use:
    // FieldType::union(vec![...]) which does a simplify() default
    pub(crate) fn new(types: Vec<TypeGeneric<T>>) -> Self {
        if types.len() <= 1 {
            panic!(
                "FATAL, please report this bug: Union type must have at least 2 types. Got {:?}",
                types
            );
        }
        Self { types }
    }

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

    pub fn view_as_iter(&self) -> (Vec<&TypeGeneric<T>>, bool) {
        match self.view() {
            UnionTypeViewGeneric::Null => (vec![], true),
            UnionTypeViewGeneric::Optional(field_type) => (vec![field_type], true),
            UnionTypeViewGeneric::OneOf(items) => (items, false),
            UnionTypeViewGeneric::OneOfOptional(items) => (items, true),
        }
    }
}

pub trait HasFieldType {
    fn field_type<'a>(&'a self) -> &'a FieldType;
}

impl HasFieldType for FieldType {
    fn field_type<'a>(&'a self) -> &'a FieldType {
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

impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut metadata_display_fmt = String::new();

        for constraint in &self.meta().constraints {
            // " @check( the_name, {{..}} )"
            let constraint_level = match constraint.level {
                ConstraintLevel::Assert => "assert",
                ConstraintLevel::Check => "check",
            };
            let constraint_name = match &constraint.label {
                None => "".to_string(),
                Some(label) => format!("{}, ", label),
            };
            metadata_display_fmt.push_str(&format!(
                " @{constraint_level}({constraint_name}, {{{{..}}}} )"
            ));
        }
        let StreamingBehavior {
            done,
            needed,
            state,
            ..
        } = self.streaming_behavior();
        if *done {
            metadata_display_fmt.push_str(" @stream.done")
        }
        if *needed {
            metadata_display_fmt.push_str(" @stream.not_null")
        }
        if *state {
            metadata_display_fmt.push_str(" @stream.with_state")
        }

        let _res = match self {
            FieldType::Enum { name, .. }
            | FieldType::Class { name, .. }
            | FieldType::RecursiveTypeAlias(name, _) => write!(f, "{name}"),
            FieldType::Primitive(t, _) => write!(f, "{t}"),
            FieldType::Literal(v, _) => write!(f, "{v}"),
            FieldType::Union(choices, _) => {
                let view = choices.view();
                let res = match view {
                    UnionTypeViewGeneric::Null => "null".to_string(),
                    UnionTypeViewGeneric::Optional(field_type) => {
                        format!("{}?", field_type.to_string())
                    }
                    UnionTypeViewGeneric::OneOf(field_types) => field_types
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(" | "),
                    UnionTypeViewGeneric::OneOfOptional(field_types) => {
                        let not_null_choices_str = field_types
                            .iter()
                            .map(|t| t.to_string())
                            .collect::<Vec<_>>()
                            .join(" | ");
                        format!("({})?", not_null_choices_str)
                    }
                };
                write!(f, "{res}")
            }
            FieldType::Tuple(choices, _) => {
                write!(
                    f,
                    "({})",
                    choices
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            FieldType::Map(k, v, _) => write!(f, "map<{k}, {v}>"),
            FieldType::List(t, _) => write!(f, "{t}[]"),
            FieldType::Arrow(arrow, _) => write!(
                f,
                "({}) -> {}",
                arrow
                    .param_types
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                arrow.return_type.to_string()
            ),
        }?;

        write!(f, "{}", metadata_display_fmt)
    }
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

    pub fn set_meta(&mut self, meta: T) {
        match self {
            TypeGeneric::Class { meta: m, .. } => *m = meta,
            TypeGeneric::Arrow(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Primitive(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Enum { meta: m, .. } => *m = meta,
            TypeGeneric::Literal(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::List(_, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::Map(_, _, type_metadata_ir) => *type_metadata_ir = meta,
            TypeGeneric::RecursiveTypeAlias(_, type_metadata_ir) => *type_metadata_ir = meta,
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
            TypeGeneric::RecursiveTypeAlias(_, type_metadata_ir) => type_metadata_ir,
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
            TypeGeneric::RecursiveTypeAlias(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::Tuple(_, type_metadata_ir) => type_metadata_ir,
            TypeGeneric::Union(_, type_metadata_ir) => type_metadata_ir,
        }
    }

    /// For types that are unions, replace the variants list with
    /// a simplified (flattened) variants list.
    pub fn simplify(&self) -> TypeGeneric<T>
    where
        T: Clone + Default + std::fmt::Debug + Eq + std::hash::Hash,
    {
        match self {
            TypeGeneric::Union(inner, _) => {
                let view = inner.view();
                let flattened = view.flatten();
                let unique = flattened.into_iter().unique().collect::<Vec<_>>();
                let has_null = unique.contains(&TypeGeneric::null());
                // if the union contains null, we'll detect that here.
                let mut variants: Vec<TypeGeneric<T>> = unique
                    .into_iter()
                    .filter(|t| t != &TypeGeneric::null())
                    .collect::<Vec<_>>();

                let simplified: TypeGeneric<T> = match variants.len() {
                    0 => return TypeGeneric::null(),
                    1 => {
                        if has_null {
                            // Return an optional of a single variant.
                            TypeGeneric::Union(
                                UnionTypeGeneric {
                                    types: vec![variants[0].clone(), TypeGeneric::null()],
                                },
                                T::default(),
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
                        TypeGeneric::Union(UnionTypeGeneric { types: variants }, T::default())
                    }
                };

                simplified
            }
            _ => self.clone(),
        }
    }

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
        match self {
            TypeGeneric::Primitive(TypeValue::Null, _) => true,
            _ => false,
        }
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
                TypeGeneric::RecursiveTypeAlias(name, _) => {
                    deps.insert(name.clone());
                }
                TypeGeneric::Primitive(_, _) | TypeGeneric::Literal(_, _) => {}
            }
        }
        deps
    }
}

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
pub fn partialize(r#type: &Type) -> TypeStreaming {
    // This inner worker function goes from `FieldType` to `FieldType` to be
    // suitable for recursive use. We only wrap the outermost `FieldType` in
    // `StreamingType`.
    fn partialize_helper(r#type: &Type) -> TypeStreaming {
        let StreamingBehavior {
            done,
            needed,
            state,
            ..
        } = r#type
            .streaming_behavior()
            .combine(&inherent_streaming_behavior(&r#type));

        // A copy of the metadata to use in the new type.
        let meta = TypeMetaStreaming {
            streaming_behavior: StreamingBehavior {
                done,
                state,
                needed,
            },
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
                TypeStreaming::List(Box::new(partialize(item_type)), meta)
            }
            FieldType::Map(key_type, item_type, _) => TypeStreaming::Map(
                Box::new(partialize(key_type)),
                Box::new(partialize(item_type)),
                meta,
            ),
            FieldType::RecursiveTypeAlias(name, _) => {
                TypeStreaming::RecursiveTypeAlias(name.clone(), meta)
            }
            FieldType::Tuple(field_types, _) => {
                TypeStreaming::Tuple(field_types.iter().map(|t| partialize(t)).collect(), meta)
            }
            FieldType::Arrow(arrow, _) => TypeStreaming::Arrow(
                Box::new(ArrowGeneric {
                    param_types: arrow.param_types.iter().map(|t| partialize(t)).collect(),
                    return_type: partialize(&arrow.return_type),
                }),
                meta,
            ),
            FieldType::Union(union_type, _) => {
                let (variants, _is_optional) = union_type.view_as_iter();
                TypeStreaming::Union(
                    UnionTypeGeneric::new(variants.iter().map(|t| partialize(t)).collect()),
                    meta,
                )
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
            *base_type_streaming.meta_mut() = TypeMetaStreaming::default();
            let mut optional_value = TypeStreaming::optional(base_type_streaming);
            *optional_value.meta_mut() = meta;
            optional_value
        }
    }

    // Types have inherent streaming behavior. For example literals and
    // numbers are inherently @done. These behaviors are applied even
    // without user annotations.
    fn inherent_streaming_behavior(field_type: &FieldType) -> StreamingBehavior {
        match field_type {
            FieldType::Primitive(type_value, _) => match type_value {
                TypeValue::String => StreamingBehavior::default(),
                TypeValue::Int => StreamingBehavior {
                    done: true,
                    needed: false,
                    state: false,
                },
                TypeValue::Float => StreamingBehavior {
                    done: true,
                    needed: false,
                    state: false,
                },
                TypeValue::Bool => StreamingBehavior {
                    done: true,
                    needed: false,
                    state: false,
                },
                TypeValue::Null => StreamingBehavior::default(),
                TypeValue::Media(_) => StreamingBehavior::default(),
            },
            FieldType::Enum { .. } => StreamingBehavior {
                done: true,
                needed: false,
                state: false,
            },
            FieldType::Literal(_, _) => StreamingBehavior {
                done: true,
                needed: false,
                state: false,
            },
            FieldType::Class { .. } => StreamingBehavior::default(),
            FieldType::List(..) => StreamingBehavior::default(),
            FieldType::Map(..) => StreamingBehavior::default(),
            FieldType::RecursiveTypeAlias(..) => StreamingBehavior::default(),
            FieldType::Tuple(..) => StreamingBehavior::default(),
            FieldType::Arrow(..) => StreamingBehavior::default(),
            FieldType::Union(inner, _) => inner
                .types
                .iter()
                .fold(StreamingBehavior::default(), |acc, t| {
                    acc.combine(&inherent_streaming_behavior(t))
                }),
        }
    }
    partialize_helper(r#type)
}

impl TypeGeneric<TypeMeta> {
    pub fn streaming_behavior(&self) -> &StreamingBehavior {
        &self.meta().streaming_behavior
    }
}

pub trait ToUnionName {
    fn to_union_name(&self) -> String;
    fn find_union_types(&self) -> IndexSet<FieldType>;
}

impl ToUnionName for FieldType {
    fn find_union_types(&self) -> IndexSet<FieldType> {
        // TODO: its pretty hard to get type aliases here
        let value = self.simplify();
        match &value {
            FieldType::Union(_, _) => IndexSet::from_iter([value]),
            FieldType::List(inner, _) => inner.find_union_types(),
            FieldType::Map(field_type, field_type1, _) => {
                let mut set = field_type.find_union_types();
                set.extend(field_type1.find_union_types());
                set
            }
            FieldType::Primitive(_, _)
            | FieldType::Enum { .. }
            | FieldType::Literal(_, _)
            | FieldType::Class { .. }
            | FieldType::RecursiveTypeAlias(_, _)
            | FieldType::Arrow(_, _) => IndexSet::new(),
            FieldType::Tuple(inner, _) => inner.iter().flat_map(|t| t.find_union_types()).collect(),
        }
    }

    fn to_union_name(&self) -> String {
        match self {
            FieldType::Primitive(type_value, _) => type_value.to_string(),
            FieldType::Enum { name, .. } => name.to_string(),
            FieldType::Literal(literal_value, _) => match literal_value {
                LiteralValue::String(value) => format!(
                    "string_{}",
                    value
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '_' })
                        .collect::<String>()
                ),
                LiteralValue::Int(val) => format!("int_{}", val.to_string()),
                LiteralValue::Bool(val) => format!("bool_{}", val.to_string()),
            },
            FieldType::Class { name, .. } => name.to_string(),
            FieldType::List(field_type, _) => {
                format!("List__{}", field_type.to_union_name())
            }
            FieldType::Map(field_type, field_type1, _) => {
                format!(
                    "Map__{}_{}",
                    field_type.to_union_name(),
                    field_type1.to_union_name()
                )
            }
            FieldType::Union(field_types, _) => match field_types.view() {
                UnionTypeViewGeneric::Null => "null".to_string(),
                UnionTypeViewGeneric::Optional(field_type) => field_type.to_union_name(),
                UnionTypeViewGeneric::OneOf(field_types)
                | UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    format!(
                        "Union__{}",
                        field_types
                            .iter()
                            .map(|t| t.to_union_name())
                            .collect::<Vec<_>>()
                            .join("__")
                    )
                }
            },
            FieldType::Tuple(field_types, _) => format!(
                "Tuple__{}",
                field_types
                    .iter()
                    .map(|v| v.to_union_name())
                    .collect::<Vec<_>>()
                    .join("__")
                    .to_string()
            ),
            FieldType::RecursiveTypeAlias(name, _) => name.to_string(),
            FieldType::Arrow(_, _) => "function".to_string(),
        }
    }
}

// pub struct StreamingBehaviorG<T> {
//     pub needed: bool,
//     pub done: T,
//     pub state: bool,
// }
//
// type StreamingBehavior = StreamingBehaviorG<bool>;
// type StreamingBehaviorStreaming = StreamingBehaviorG<()>;

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

// TODO: Do we need this?
impl TypeMeta {
    pub fn combine(&self, other: &TypeMeta) -> TypeMeta {
        let mut constraints = self.constraints.clone();
        constraints.extend(other.constraints.clone());
        TypeMeta {
            streaming_behavior: self.streaming_behavior.combine(&other.streaming_behavior),
            constraints,
        }
    }
}

/// Metadata on a type that determines how it behaves under streaming conditions.
#[derive(Clone, Debug, PartialEq, serde::Serialize, Eq, Hash)]
pub struct StreamingBehavior {
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

impl StreamingBehavior {
    pub fn combine(&self, other: &StreamingBehavior) -> StreamingBehavior {
        StreamingBehavior {
            done: self.done || other.done,
            state: self.state || other.state,
            needed: self.needed || other.needed,
        }
    }
}

impl Default for StreamingBehavior {
    fn default() -> Self {
        StreamingBehavior {
            done: false,
            state: false,
            needed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn const_null_equals_synthetic_null() {
        let optional_int = FieldType::optional(FieldType::int());
        let union = FieldType::Union(
            UnionTypeGeneric::new(vec![FieldType::int(), FieldType::null()]),
            TypeMeta::default(),
        );
        assert_eq!(optional_int, union)
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
            partialize(&null),
            TypeStreaming::Primitive(TypeValue::Null, TypeMetaStreaming::default())
        );
    }

    #[test]
    // Foo => stream.Foo | null
    fn partialize_bare_class() {
        let class = FieldType::class("MyClass");
        assert_eq!(
            partialize(&class),
            TypeGeneric::optional(TypeStreaming::Class {
                name: "MyClass".to_string(),
                dynamic: false,
                mode: StreamingMode::Streaming,
                meta: TypeMetaStreaming::default(),
            })
        );
    }

    #[test]
    // Foo @stream.done => Foo
    fn partialize_class_with_done() {
        let mut class = FieldType::class("MyClass");
        let mut expected = TypeGeneric::optional(TypeStreaming::Class {
            name: "MyClass".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: TypeMetaStreaming::default(),
        });
        expected.meta_mut().streaming_behavior.done = true;
        class.meta_mut().streaming_behavior.done = true;
        assert_eq!(partialize(&class), expected);
    }

    #[test]
    fn partialize_simple_union() {
        let union = FieldType::union(vec![FieldType::int(), FieldType::string()]);
        let expected = TypeStreaming::optional(TypeStreaming::union(vec![
            TypeStreaming::int(),
            TypeStreaming::string(),
        ]));
        assert_eq!(partialize(&union), expected);
    }
}
