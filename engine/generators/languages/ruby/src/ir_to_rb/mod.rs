use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    type_meta::{self, stream::TypeMetaStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{MediaTypeRb, TypeMetaRb, TypeRb, TypeWrapper},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;

pub(crate) fn stream_type_to_rb(field: &TypeStreaming, _lookup: &impl TypeLookups) -> TypeRb {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_rb(field, _lookup);
    let meta = stream_meta_to_rb(field.meta());

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    let type_rb: TypeRb = match field {
        T::Primitive(type_value, _) => {
            let t: TypeRb = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeRb::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeRb::String(Some(val.clone()), meta),
            baml_types::LiteralValue::Int(val) => TypeRb::Int(Some(*val), meta),
            baml_types::LiteralValue::Bool(val) => TypeRb::Bool(Some(*val), meta),
        },
        T::Class {
            name,
            dynamic,
            meta: cls_meta,
            ..
        } => TypeRb::Class {
            package: match cls_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypeRb::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeRb::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::RecursiveTypeAlias {
            name,
            meta: alias_meta,
            ..
        } => TypeRb::TypeAlias {
            package: match alias_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
            meta,
        },
        T::Tuple(..) => TypeRb::Any {
            reason: "tuples are not supported in Rb".to_string(),
            meta,
        },
        T::Arrow(..) => TypeRb::Any {
            reason: "arrow types are not supported in Rb".to_string(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => {
            match union_type_generic.view() {
                baml_types::ir_type::UnionTypeViewGeneric::Null => TypeRb::Any {
                    reason: "Null types are not supported in Rb".to_string(),
                    meta,
                },
                baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                    let mut type_rb = recursive_fn(type_generic);
                    // get all checks
                    let checks = union_meta
                        .constraints
                        .iter()
                        .filter_map(|c| {
                            if matches!(c.level, ConstraintLevel::Check) {
                                c.label.as_ref()
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    if !checks.is_empty() {
                        type_rb.meta_mut().map(|m| m.make_checked(checks));
                    }
                    type_rb.meta_mut().map(|m| m.make_optional());
                    if union_meta.streaming_behavior.state {
                        type_rb.meta_mut().map(|m| m.set_stream_state());
                    }
                    type_rb
                }
                baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                    let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                    TypeRb::Union {
                        variants: options,
                        meta,
                    }
                }
                baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                    let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                    let mut meta = meta;
                    meta.make_optional();
                    TypeRb::Union {
                        variants: options,
                        meta,
                    }
                }
            }
        }
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_rb
}

pub(crate) fn type_to_rb(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypeRb {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_rb(field, _lookup);
    let meta = meta_to_rb(field.meta());

    let type_pkg = Package::types();

    let type_rb = match field {
        T::Primitive(type_value, _) => {
            let t: TypeRb = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeRb::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeRb::String(Some(val.clone()), meta),
            baml_types::LiteralValue::Int(val) => TypeRb::Int(Some(*val), meta),
            baml_types::LiteralValue::Bool(val) => TypeRb::Bool(Some(*val), meta),
        },
        T::Class { name, dynamic, .. } => TypeRb::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypeRb::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeRb::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::Tuple(..) => TypeRb::Any {
            reason: "tuples are not supported in Rb".to_string(),
            meta,
        },
        T::Arrow(..) => TypeRb::Any {
            reason: "arrow types are not supported in Rb".to_string(),
            meta,
        },
        T::RecursiveTypeAlias { name, .. } => TypeRb::TypeAlias {
            package: type_pkg.clone(),
            name: name.clone(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeRb::Any {
                reason: "Null types are not supported in Rb".to_string(),
                meta,
            },
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_rb = recursive_fn(type_generic);
                type_rb.meta_mut().map(|m| m.make_optional());
                let checks = union_meta
                    .constraints
                    .iter()
                    .filter_map(|c| {
                        if matches!(c.level, ConstraintLevel::Check) {
                            c.label.as_ref()
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                if !checks.is_empty() {
                    type_rb.meta_mut().map(|m| m.make_checked(checks));
                }
                type_rb
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                TypeRb::Union {
                    variants: options,
                    meta,
                }
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                let mut meta = meta;
                meta.make_optional();
                TypeRb::Union {
                    variants: options,
                    meta,
                }
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_rb
}

// convert ir metadata to rb metadata
fn meta_to_rb(meta: &type_meta::NonStreaming) -> TypeMetaRb {
    let checks = meta
        .constraints
        .iter()
        .filter_map(|c| {
            if matches!(c.level, ConstraintLevel::Check) {
                c.label.as_ref()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let wrapper = TypeWrapper::default();
    let wrapper = if !checks.is_empty() {
        wrapper.wrap_with_checked(checks)
    } else {
        wrapper
    };

    // optionality is handled by unions
    TypeMetaRb {
        type_wrapper: wrapper,
        wrap_stream_state: false,
    }
}

fn stream_meta_to_rb(meta: &TypeMetaStreaming) -> TypeMetaRb {
    let checks = meta
        .constraints
        .iter()
        .filter_map(|c| {
            if matches!(c.level, ConstraintLevel::Check) {
                c.label.as_ref()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let wrapper = TypeWrapper::default();
    let wrapper = if !checks.is_empty() {
        wrapper.wrap_with_checked(checks)
    } else {
        wrapper
    };

    TypeMetaRb {
        type_wrapper: wrapper,
        wrap_stream_state: meta.streaming_behavior.state,
    }
}

impl From<&TypeValue> for TypeRb {
    fn from(type_value: &TypeValue) -> Self {
        let meta = TypeMetaRb::default();
        match type_value {
            TypeValue::String => TypeRb::String(None, meta),
            TypeValue::Int => TypeRb::Int(None, meta),
            TypeValue::Float => TypeRb::Float(meta),
            TypeValue::Bool => TypeRb::Bool(None, meta),
            TypeValue::Null => TypeRb::Any {
                reason: "Null types are not supported in Rb".to_string(),
                meta,
            },
            TypeValue::Media(baml_media_type) => TypeRb::Media(baml_media_type.into(), meta),
        }
    }
}

impl From<&BamlMediaType> for MediaTypeRb {
    fn from(baml_media_type: &BamlMediaType) -> Self {
        match baml_media_type {
            BamlMediaType::Image => MediaTypeRb::Image,
            BamlMediaType::Audio => MediaTypeRb::Audio,
            BamlMediaType::Pdf => MediaTypeRb::Pdf,
            BamlMediaType::Video => MediaTypeRb::Video,
        }
    }
}

#[cfg(test)]
mod tests {}
