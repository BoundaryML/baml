use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    type_meta::{self, stream::TypeMetaStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{MediaTypeGo, TypeGo, TypeMetaGo, TypeWrapper},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;
pub mod unions;

pub(crate) fn stream_type_to_go(field: &TypeStreaming, lookup: &impl TypeLookups) -> TypeGo {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_go(field, lookup);
    let meta = stream_meta_to_go(field.meta());

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    let type_go: TypeGo = match field {
        T::Primitive(type_value, _) => {
            let t: TypeGo = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeGo::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeGo::String(Some(val.clone()), meta),
            baml_types::LiteralValue::Int(val) => TypeGo::Int(Some(*val), meta),
            baml_types::LiteralValue::Bool(val) => TypeGo::Bool(Some(*val), meta),
        },
        T::Class {
            name,
            dynamic,
            meta: cls_meta,
            ..
        } => TypeGo::Class {
            package: match cls_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypeGo::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeGo::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::RecursiveTypeAlias {
            name,
            meta: alias_meta,
            ..
        } => {
            if lookup.expand_recursive_type(name).is_err() {
                TypeGo::Any {
                    reason: format!("Recursive type alias {name} is not supported in Go"),
                    meta,
                }
            } else {
                TypeGo::TypeAlias {
                    package: match alias_meta.streaming_behavior.done {
                        true => types_pkg.clone(),
                        false => stream_pkg.clone(),
                    },
                    name: name.clone(),
                    meta,
                }
            }
        }
        T::Tuple(..) => TypeGo::Any {
            reason: "tuples are not supported in Go".to_string(),
            meta,
        },
        T::Arrow(..) => TypeGo::Any {
            reason: "arrow types are not supported in Go".to_string(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeGo::Any {
                reason: "Null types are not supported in Go".to_string(),
                meta,
            },
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_go = recursive_fn(type_generic);
                if union_meta
                    .constraints
                    .iter()
                    .any(|c| matches!(c.level, ConstraintLevel::Check))
                {
                    type_go.meta_mut().make_checked();
                }
                type_go.meta_mut().make_optional();
                if union_meta.streaming_behavior.state {
                    type_go.meta_mut().set_stream_state();
                }
                type_go
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                let num_options = options.len();
                let mut name = options
                    .iter()
                    .map(|t| t.default_name_within_union())
                    .collect::<Vec<_>>();
                name.sort();
                let name = name.join("Or");
                TypeGo::Union {
                    package: match field.mode(&baml_types::StreamingMode::Streaming, lookup) {
                        Ok(baml_types::StreamingMode::NonStreaming) => types_pkg.clone(),
                        Ok(baml_types::StreamingMode::Streaming) => stream_pkg.clone(),
                        Err(e) => {
                            return TypeGo::Any {
                                reason: format!("Failed to get mode for field type: {e}"),
                                meta,
                            }
                        }
                    },
                    name: format!("Union{num_options}{name}"),
                    meta,
                }
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                let num_options = options.len();
                let mut name = options
                    .iter()
                    .map(|t| t.default_name_within_union())
                    .collect::<Vec<_>>();
                name.sort();
                let name = name.join("Or");
                let mut meta = meta;
                meta.make_optional();
                TypeGo::Union {
                    package: match field.mode(&baml_types::StreamingMode::Streaming, lookup) {
                        Ok(baml_types::StreamingMode::NonStreaming) => types_pkg.clone(),
                        Ok(baml_types::StreamingMode::Streaming) => stream_pkg.clone(),
                        Err(e) => {
                            return TypeGo::Any {
                                reason: format!("Failed to get mode for field type: {e}"),
                                meta,
                            }
                        }
                    },
                    name: format!("Union{num_options}{name}"),
                    meta,
                }
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_go
}

pub(crate) fn type_to_go(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypeGo {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_go(field, _lookup);
    let meta = meta_to_go(field.meta());

    let type_pkg = Package::types();

    let type_go = match field {
        T::Primitive(type_value, _) => {
            let t: TypeGo = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeGo::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeGo::String(Some(val.clone()), meta),
            baml_types::LiteralValue::Int(val) => TypeGo::Int(Some(*val), meta),
            baml_types::LiteralValue::Bool(val) => TypeGo::Bool(Some(*val), meta),
        },
        T::Class { name, dynamic, .. } => TypeGo::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypeGo::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeGo::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::Tuple(..) => TypeGo::Any {
            reason: "tuples are not supported in Go".to_string(),
            meta,
        },
        T::Arrow(..) => TypeGo::Any {
            reason: "arrow types are not supported in Go".to_string(),
            meta,
        },
        T::RecursiveTypeAlias { name, .. } => {
            if _lookup.expand_recursive_type(name).is_err() {
                TypeGo::Any {
                    reason: format!("Recursive type alias {name} is not supported in Go"),
                    meta,
                }
            } else {
                TypeGo::TypeAlias {
                    package: type_pkg.clone(),
                    name: name.clone(),
                    meta,
                }
            }
        }
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeGo::Any {
                reason: "Null types are not supported in Go".to_string(),
                meta,
            },
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_go = recursive_fn(type_generic);
                type_go.meta_mut().make_optional();
                if union_meta
                    .constraints
                    .iter()
                    .any(|c| matches!(c.level, ConstraintLevel::Check))
                {
                    type_go.meta_mut().make_checked();
                }
                type_go
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                let num_options = options.len();
                let mut name = options
                    .iter()
                    .map(|t| t.default_name_within_union())
                    .collect::<Vec<_>>();
                name.sort();
                let name = name.join("Or");
                TypeGo::Union {
                    package: type_pkg.clone(),
                    name: format!("Union{num_options}{name}"),
                    meta,
                }
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                let num_options = options.len();

                let mut name = options
                    .iter()
                    .map(|t| t.default_name_within_union())
                    .collect::<Vec<_>>();
                name.sort();
                let name = name.join("Or");

                let mut meta = meta;
                meta.make_optional();
                TypeGo::Union {
                    package: type_pkg.clone(),
                    name: format!("Union{num_options}{name}"),
                    meta,
                }
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_go
}

// convert ir metadata to go metadata
fn meta_to_go(meta: &type_meta::NonStreaming) -> TypeMetaGo {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let wrapper = TypeWrapper::default();
    let wrapper = if has_checks {
        wrapper.wrap_with_checked()
    } else {
        wrapper
    };

    // optionality is handled by unions
    TypeMetaGo {
        type_wrapper: wrapper,
        wrap_stream_state: false,
    }
}

fn stream_meta_to_go(meta: &TypeMetaStreaming) -> TypeMetaGo {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let wrapper = TypeWrapper::default();
    let wrapper = if has_checks {
        wrapper.wrap_with_checked()
    } else {
        wrapper
    };

    TypeMetaGo {
        type_wrapper: wrapper,
        wrap_stream_state: meta.streaming_behavior.state,
    }
}

impl From<&TypeValue> for TypeGo {
    fn from(type_value: &TypeValue) -> Self {
        let meta = TypeMetaGo::default();
        match type_value {
            TypeValue::String => TypeGo::String(None, meta),
            TypeValue::Int => TypeGo::Int(None, meta),
            TypeValue::Float => TypeGo::Float(meta),
            TypeValue::Bool => TypeGo::Bool(None, meta),
            TypeValue::Null => TypeGo::Any {
                reason: "Null types are not supported in Go".to_string(),
                meta: {
                    let mut meta = meta;
                    meta.make_optional();
                    meta
                },
            },
            TypeValue::Media(baml_media_type) => TypeGo::Media(baml_media_type.into(), meta),
        }
    }
}

impl From<&BamlMediaType> for MediaTypeGo {
    fn from(baml_media_type: &BamlMediaType) -> Self {
        match baml_media_type {
            BamlMediaType::Image => MediaTypeGo::Image,
            BamlMediaType::Audio => MediaTypeGo::Audio,
            BamlMediaType::Pdf => MediaTypeGo::Pdf,
            BamlMediaType::Video => MediaTypeGo::Video,
        }
    }
}

#[cfg(test)]
mod tests {}
