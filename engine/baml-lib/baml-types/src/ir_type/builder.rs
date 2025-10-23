use super::{BamlMediaType, StreamingMode, TypeGeneric, TypeValue};

impl<T: Default> TypeGeneric<T> {
    pub fn top() -> Self {
        TypeGeneric::Top(T::default())
    }

    pub fn string() -> Self {
        TypeGeneric::Primitive(TypeValue::String, T::default())
    }

    pub fn recursive_type_alias<U: AsRef<str>>(name: U) -> Self {
        TypeGeneric::RecursiveTypeAlias {
            name: name.as_ref().to_string(),
            mode: StreamingMode::Streaming,
            meta: T::default(),
        }
    }

    pub fn literal<U: Into<super::LiteralValue>>(value: U) -> Self {
        TypeGeneric::Literal(value.into(), T::default())
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

    #[cfg(test)]
    pub fn int_with_meta(meta: T) -> Self {
        TypeGeneric::Primitive(TypeValue::Int, meta)
    }

    #[cfg(test)]
    pub fn float_with_meta(meta: T) -> Self {
        TypeGeneric::Primitive(TypeValue::Float, meta)
    }

    #[cfg(test)]
    pub fn string_with_meta(meta: T) -> Self {
        TypeGeneric::Primitive(TypeValue::String, meta)
    }

    #[cfg(test)]
    pub fn with_meta(mut self, meta: T) -> Self {
        self.set_meta(meta);
        self
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

    pub fn pdf() -> Self {
        TypeGeneric::Primitive(TypeValue::Media(BamlMediaType::Pdf), T::default())
    }

    pub fn video() -> Self {
        TypeGeneric::Primitive(TypeValue::Media(BamlMediaType::Video), T::default())
    }

    pub fn r#enum(name: &str) -> Self {
        TypeGeneric::Enum {
            name: name.to_string(),
            dynamic: false,
            meta: T::default(),
        }
    }

    pub fn class<U: AsRef<str>>(name: U) -> Self {
        TypeGeneric::Class {
            name: name.as_ref().to_string(),
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

    pub fn tuple(choices: Vec<TypeGeneric<T>>) -> Self {
        TypeGeneric::Tuple(choices, T::default())
    }

    pub fn arrow(param_types: Vec<TypeGeneric<T>>, return_type: TypeGeneric<T>) -> Self {
        TypeGeneric::Arrow(
            Box::new(super::ArrowGeneric {
                param_types,
                return_type,
            }),
            T::default(),
        )
    }
}
