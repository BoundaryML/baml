use super::{BamlMediaType, StreamingMode, TypeGeneric, TypeValue, UnionTypeGeneric};

impl<T: Default + std::fmt::Debug> TypeGeneric<T> {
    pub fn string() -> Self {
        TypeGeneric::Primitive(TypeValue::String, T::default())
    }

    pub fn literal_string(value: String) -> Self {
        TypeGeneric::Literal(super::LiteralValue::String(value), T::default())
    }

    pub fn literal_int(value: i64) -> Self {
        TypeGeneric::Literal(super::LiteralValue::Int(value), T::default())
    }

    pub fn literal_bool(value: bool) -> Self {
        TypeGeneric::Literal(super::LiteralValue::Bool(value), T::default())
    }

    pub fn int() -> Self {
        TypeGeneric::Primitive(TypeValue::Int, T::default())
    }

    pub fn float() -> Self {
        TypeGeneric::Primitive(TypeValue::Float, T::default())
    }

    pub fn bool() -> Self {
        TypeGeneric::Primitive(TypeValue::Bool, T::default())
    }

    pub fn null() -> Self {
        TypeGeneric::Primitive(TypeValue::Null, T::default())
    }

    pub fn image() -> Self {
        TypeGeneric::Primitive(TypeValue::Media(BamlMediaType::Image), T::default())
    }

    pub fn audio() -> Self {
        TypeGeneric::Primitive(TypeValue::Media(BamlMediaType::Audio), T::default())
    }

    pub fn r#enum(name: &str) -> Self {
        TypeGeneric::Enum {
            name: name.to_string(),
            dynamic: false,
            meta: T::default(),
        }
    }

    pub fn class(name: &str) -> Self {
        TypeGeneric::Class {
            name: name.to_string(),
            dynamic: false,
            mode: StreamingMode::NonStreaming,
            meta: T::default(),
        }
    }

    pub fn list(inner: Self) -> Self {
        TypeGeneric::List(Box::new(inner), T::default())
    }

    pub fn as_list(self) -> Self {
        TypeGeneric::List(Box::new(self), T::default())
    }

    pub fn map(key: TypeGeneric<T>, value: TypeGeneric<T>) -> Self {
        TypeGeneric::Map(Box::new(key), Box::new(value), T::default())
    }

    pub fn union(choices: Vec<TypeGeneric<T>>) -> Self
    where
        T: Clone + Eq + std::hash::Hash + std::fmt::Debug + Default,
    {
        TypeGeneric::Union(UnionTypeGeneric::new(choices), T::default()).simplify()
    }

    pub fn tuple(choices: Vec<TypeGeneric<T>>) -> Self {
        TypeGeneric::Tuple(choices, T::default())
    }

    pub fn optional(inner: TypeGeneric<T>) -> Self
    where
        T: Clone + Eq + std::hash::Hash + std::fmt::Debug + Default,
    {
        TypeGeneric::Union(
            UnionTypeGeneric::new(vec![
                inner,
                TypeGeneric::Primitive(TypeValue::Null, T::default()),
            ]),
            T::default(),
        )
        .simplify()
    }

    pub fn as_optional(self) -> Self
    where
        T: Clone + Eq + std::hash::Hash + std::fmt::Debug + Default,
    {
        TypeGeneric::Union(
            UnionTypeGeneric::new(vec![
                self,
                TypeGeneric::Primitive(TypeValue::Null, T::default()),
            ]),
            T::default(),
        )
        .simplify()
    }
}

// impl FieldType {
//     pub fn string() -> Self {
//         FieldType::Primitive(TypeValue::String, TypeMeta::default())
//     }

//     pub fn literal_string(value: String) -> Self {
//         FieldType::Literal(super::LiteralValue::String(value), TypeMeta::default())
//     }

//     pub fn literal_int(value: i64) -> Self {
//         FieldType::Literal(super::LiteralValue::Int(value), TypeMeta::default())
//     }

//     pub fn literal_bool(value: bool) -> Self {
//         FieldType::Literal(super::LiteralValue::Bool(value), TypeMeta::default())
//     }

//     pub fn int() -> Self {
//         FieldType::Primitive(TypeValue::Int, TypeMeta::default())
//     }

//     pub fn float() -> Self {
//         FieldType::Primitive(TypeValue::Float, TypeMeta::default())
//     }

//     pub fn bool() -> Self {
//         FieldType::Primitive(TypeValue::Bool, TypeMeta::default())
//     }

//     pub fn null() -> Self {
//         FieldType::Primitive(TypeValue::Null, TypeMeta::default())
//     }

//     pub fn image() -> Self {
//         FieldType::Primitive(TypeValue::Media(BamlMediaType::Image), TypeMeta::default())
//     }

//     pub fn audio() -> Self {
//         FieldType::Primitive(TypeValue::Media(BamlMediaType::Audio), TypeMeta::default())
//     }

//     pub fn r#enum(name: &str) -> Self {
//         FieldType::Enum(name.to_string(), TypeMeta::default())
//     }

//     pub fn class(name: &str) -> Self {
//         FieldType::Class(name.to_string(), TypeMeta::default())
//     }

//     pub fn list(inner: FieldType) -> Self {
//         FieldType::List(Box::new(inner), TypeMeta::default())
//     }

//     pub fn as_list(self) -> Self {
//         FieldType::List(Box::new(self), TypeMeta::default())
//     }

//     pub fn map(key: FieldType, value: FieldType) -> Self {
//         FieldType::Map(Box::new(key), Box::new(value), TypeMeta::default())
//     }

//     pub fn union(choices: Vec<FieldType>) -> Self {
//         FieldType::Union(UnionType::new(choices), TypeMeta::default()).simplify()
//     }

//     pub fn tuple(choices: Vec<FieldType>) -> Self {
//         FieldType::Tuple(choices, TypeMeta::default())
//     }

//     pub fn optional(inner: FieldType) -> Self {
//         FieldType::Union(
//             UnionType::new(vec![
//                 inner,
//                 FieldType::Primitive(TypeValue::Null, TypeMeta::default()),
//             ]),
//             TypeMeta::default(),
//         )
//         .simplify()
//     }

//     pub fn as_optional(self) -> Self {
//         FieldType::Union(
//             UnionType::new(vec![
//                 self,
//                 FieldType::Primitive(TypeValue::Null, TypeMeta::default()),
//             ]),
//             TypeMeta::default(),
//         )
//         .simplify()
//     }
// }
