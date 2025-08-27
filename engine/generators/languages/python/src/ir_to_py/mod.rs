use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    type_meta::{self, stream::TypeMetaStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{EscapedPythonString, LiteralValue, MediaTypePy, TypeMetaPy, TypePy, TypeWrapper},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;

pub(crate) fn stream_type_to_py(field: &TypeStreaming, _lookup: &impl TypeLookups) -> TypePy {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_py(field, _lookup);
    let meta = stream_meta_to_py(field.meta());

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    let type_py: TypePy = match field {
        T::Primitive(type_value, _) => {
            let t: TypePy = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypePy::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(_) => TypePy::String(meta),
            baml_types::LiteralValue::Int(_) => TypePy::Int(meta),
            baml_types::LiteralValue::Bool(_) => TypePy::Bool(meta),
        },
        T::Class {
            name,
            dynamic,
            meta: cls_meta,
            ..
        } => TypePy::Class {
            package: match cls_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypePy::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypePy::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::RecursiveTypeAlias {
            name,
            meta: alias_meta,
            ..
        } => TypePy::TypeAlias {
            package: match alias_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
            meta,
        },
        T::Tuple(..) => TypePy::Any {
            reason: "tuples are not supported in Py".to_string(),
            meta,
        },
        T::Arrow(..) => TypePy::Any {
            reason: "arrow types are not supported in Py".to_string(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => {
            match union_type_generic.view() {
                baml_types::ir_type::UnionTypeViewGeneric::Null => TypePy::Any {
                    reason: "Null types are not supported in Py".to_string(),
                    meta,
                },
                baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                    let mut type_py = recursive_fn(type_generic);
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
                        type_py.meta_mut().map(|m| m.make_checked(checks));
                    }
                    type_py.meta_mut().map(|m| m.make_optional());
                    if union_meta.streaming_behavior.state {
                        type_py.meta_mut().map(|m| m.set_stream_state());
                    }
                    type_py
                }
                baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                    let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                    TypePy::Union {
                        variants: options,
                        meta,
                    }
                }
                baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                    let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                    let mut meta = meta;
                    meta.make_optional();
                    TypePy::Union {
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

    type_py
}

pub(crate) fn type_to_py(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypePy {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_py(field, _lookup);
    let meta = meta_to_py(field.meta());

    let type_pkg = Package::types();

    let type_py = match field {
        T::Primitive(type_value, _) => {
            let t: TypePy = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypePy::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => TypePy::Literal(
            vec![match literal_value {
                baml_types::LiteralValue::String(val) => {
                    LiteralValue::String(EscapedPythonString::new(val))
                }
                baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
                baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
            }],
            meta,
        ),
        T::Class { name, dynamic, .. } => TypePy::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypePy::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypePy::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::Tuple(..) => TypePy::Any {
            reason: "tuples are not supported in Py".to_string(),
            meta,
        },
        T::Arrow(..) => TypePy::Any {
            reason: "arrow types are not supported in Py".to_string(),
            meta,
        },
        T::RecursiveTypeAlias { name, .. } => TypePy::TypeAlias {
            package: type_pkg.clone(),
            name: name.clone(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => {
                TypePy::Literal(vec![LiteralValue::None], meta)
            }
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_py = recursive_fn(type_generic);
                type_py.meta_mut().map(|m| m.make_optional());
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
                    type_py.meta_mut().map(|m| m.make_checked(checks));
                }
                type_py
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                TypePy::Union {
                    variants: options,
                    meta,
                }
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                let mut meta = meta;
                meta.make_optional();
                TypePy::Union {
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

    type_py
}

// convert ir metadata to py metadata
fn meta_to_py(meta: &type_meta::NonStreaming) -> TypeMetaPy {
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
    TypeMetaPy {
        type_wrapper: wrapper,
        wrap_stream_state: false,
    }
}

fn stream_meta_to_py(meta: &TypeMetaStreaming) -> TypeMetaPy {
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

    TypeMetaPy {
        type_wrapper: wrapper,
        wrap_stream_state: meta.streaming_behavior.state,
    }
}

impl From<&TypeValue> for TypePy {
    fn from(type_value: &TypeValue) -> Self {
        let meta = TypeMetaPy::default();
        match type_value {
            TypeValue::String => TypePy::String(meta),
            TypeValue::Int => TypePy::Int(meta),
            TypeValue::Float => TypePy::Float(meta),
            TypeValue::Bool => TypePy::Bool(meta),
            TypeValue::Null => TypePy::Any {
                reason: "Null types are not supported in Py".to_string(),
                meta,
            },
            TypeValue::Media(baml_media_type) => TypePy::Media(baml_media_type.into(), meta),
        }
    }
}

impl From<&BamlMediaType> for MediaTypePy {
    fn from(baml_media_type: &BamlMediaType) -> Self {
        match baml_media_type {
            BamlMediaType::Image => MediaTypePy::Image,
            BamlMediaType::Audio => MediaTypePy::Audio,
            BamlMediaType::Pdf => MediaTypePy::Pdf,
            BamlMediaType::Video => MediaTypePy::Video,
        }
    }
}

#[cfg(test)]
mod tests {}
