use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    type_meta::{self, stream::TypeMetaStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{MediaTypeRust, TypeMetaRust, TypeRust, TypeWrapper},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;
pub mod unions;

pub(crate) fn stream_type_to_rust(field: &TypeStreaming, lookup: &impl TypeLookups) -> TypeRust {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_rust(field, lookup);
    let meta = stream_meta_to_rust(field.meta());

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_state();

    let type_rust: TypeRust = match field {
        T::Primitive(type_value, _) => {
            let t: TypeRust = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeRust::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeRust::String(Some(val.clone()), meta),
            baml_types::LiteralValue::Int(val) => TypeRust::Int(Some(*val), meta),
            baml_types::LiteralValue::Bool(val) => TypeRust::Bool(Some(*val), meta),
        },
        T::Class {
            name,
            dynamic,
            meta: cls_meta,
            ..
        } => TypeRust::Class {
            package: types_pkg.clone(), // Use types package for both streaming and non-streaming for now
            name: name.clone(),
            dynamic: *dynamic,
            needs_box: false,
            meta,
        },
        T::List(type_generic, _) => TypeRust::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeRust::Map(
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
                TypeRust::Any {
                    reason: format!("Recursive type alias {name} is not supported in Rust"),
                    meta,
                }
            } else {
                TypeRust::TypeAlias {
                    package: match alias_meta.streaming_behavior.done {
                        true => types_pkg.clone(),
                        false => stream_pkg.clone(),
                    },
                    name: name.clone(),
                    needs_box: false,
                    meta,
                }
            }
        }
        T::Tuple(..) => TypeRust::Any {
            reason: "tuples are not supported in Rust".to_string(),
            meta,
        },
        T::Arrow(..) => TypeRust::Any {
            reason: "arrow types are not supported in Rust".to_string(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeRust::Null(meta),
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_rust = recursive_fn(type_generic);
                if union_meta
                    .constraints
                    .iter()
                    .any(|c| matches!(c.level, ConstraintLevel::Check))
                {
                    let checks = union_meta
                        .constraints
                        .iter()
                        .filter_map(|c| c.label.as_ref().map(|l| l.to_string()))
                        .collect();
                    type_rust.meta_mut().make_checked(checks);
                }
                type_rust.meta_mut().make_optional();
                if union_meta.streaming_behavior.state {
                    type_rust.meta_mut().set_stream_state();
                }
                type_rust
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
                TypeRust::Union {
                    package: match field.mode(&baml_types::StreamingMode::Streaming, lookup) {
                        Ok(baml_types::StreamingMode::NonStreaming) => types_pkg.clone(),
                        Ok(baml_types::StreamingMode::Streaming) => stream_pkg.clone(),
                        Err(e) => {
                            return TypeRust::Any {
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
                TypeRust::Union {
                    package: match field.mode(&baml_types::StreamingMode::Streaming, lookup) {
                        Ok(baml_types::StreamingMode::NonStreaming) => types_pkg.clone(),
                        Ok(baml_types::StreamingMode::Streaming) => stream_pkg.clone(),
                        Err(e) => {
                            return TypeRust::Any {
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
        // TODO(Cecilia): actually deal with this
        T::Top(_) => TypeRust::Any {
            reason: "top types are not supported in Rust".to_string(),
            meta,
        },
    };

    type_rust
}

pub(crate) fn type_to_rust(field: &TypeNonStreaming, lookup: &impl TypeLookups) -> TypeRust {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_rust(field, lookup);
    let meta = meta_to_rust(field.meta());

    let type_pkg = Package::types();

    let type_rust = match field {
        T::Primitive(type_value, _) => {
            let t: TypeRust = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeRust::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeRust::String(Some(val.clone()), meta),
            baml_types::LiteralValue::Int(val) => TypeRust::Int(Some(*val), meta),
            baml_types::LiteralValue::Bool(val) => TypeRust::Bool(Some(*val), meta),
        },
        T::Class { name, dynamic, .. } => TypeRust::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            needs_box: false,
            meta,
        },
        T::List(type_generic, _) => TypeRust::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeRust::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::Tuple(..) => TypeRust::Any {
            reason: "tuples are not supported in Rust".to_string(),
            meta,
        },
        T::Arrow(..) => TypeRust::Any {
            reason: "arrow types are not supported in Rust".to_string(),
            meta,
        },
        T::RecursiveTypeAlias { name, .. } => {
            if lookup.expand_recursive_type(name).is_err() {
                TypeRust::Any {
                    reason: format!("Recursive type alias {name} is not supported in Rust"),
                    meta,
                }
            } else {
                TypeRust::TypeAlias {
                    package: type_pkg.clone(),
                    name: name.clone(),
                    needs_box: false,
                    meta,
                }
            }
        }
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeRust::Null(meta),
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_rust = recursive_fn(type_generic);
                type_rust.meta_mut().make_optional();
                if union_meta
                    .constraints
                    .iter()
                    .any(|c| matches!(c.level, ConstraintLevel::Check))
                {
                    let checks = union_meta
                        .constraints
                        .iter()
                        .filter_map(|c| c.label.as_ref().map(|l| l.to_string()))
                        .collect();
                    type_rust.meta_mut().make_checked(checks);
                }
                type_rust
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
                TypeRust::Union {
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
                TypeRust::Union {
                    package: type_pkg.clone(),
                    name: format!("Union{num_options}{name}"),
                    meta,
                }
            }
        },
        T::Top(_) => TypeRust::Any {
            reason: "top types are not supported in Rust".to_string(),
            meta,
        },
    };

    type_rust
}

// convert ir metadata to rust metadata
fn meta_to_rust(meta: &type_meta::NonStreaming) -> TypeMetaRust {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let wrapper = TypeWrapper::default();
    let wrapper = if has_checks {
        let checks = meta
            .constraints
            .iter()
            .filter_map(|c| c.label.as_ref().map(|l| l.to_string()))
            .collect();
        wrapper.wrap_with_checked(checks)
    } else {
        wrapper
    };

    // optionality is handled by unions
    TypeMetaRust {
        type_wrapper: wrapper,
        wrap_stream_state: false,
    }
}

fn stream_meta_to_rust(meta: &TypeMetaStreaming) -> TypeMetaRust {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let wrapper = TypeWrapper::default();
    let wrapper = if has_checks {
        let checks = meta
            .constraints
            .iter()
            .filter_map(|c| c.label.as_ref().map(|l| l.to_string()))
            .collect();
        wrapper.wrap_with_checked(checks)
    } else {
        wrapper
    };

    TypeMetaRust {
        type_wrapper: wrapper,
        wrap_stream_state: meta.streaming_behavior.state,
    }
}

impl From<&TypeValue> for TypeRust {
    fn from(type_value: &TypeValue) -> Self {
        let meta = TypeMetaRust::default();
        match type_value {
            TypeValue::String => TypeRust::String(None, meta),
            TypeValue::Int => TypeRust::Int(None, meta),
            TypeValue::Float => TypeRust::Float(meta),
            TypeValue::Bool => TypeRust::Bool(None, meta),
            TypeValue::Null => TypeRust::Null(meta),
            TypeValue::Media(baml_media_type) => TypeRust::Media(baml_media_type.into(), meta),
        }
    }
}

impl From<&BamlMediaType> for MediaTypeRust {
    fn from(baml_media_type: &BamlMediaType) -> Self {
        match baml_media_type {
            BamlMediaType::Image => MediaTypeRust::Image,
            BamlMediaType::Audio => MediaTypeRust::Audio,
            BamlMediaType::Pdf => MediaTypeRust::Pdf,
            BamlMediaType::Video => MediaTypeRust::Video,
        }
    }
}

#[cfg(test)]
mod tests {}
