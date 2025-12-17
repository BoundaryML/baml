use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{EscapedPythonString, LiteralValue, MediaTypePy, TypePy},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;

pub fn stream_type_to_py(field: &TypeStreaming, _lookup: &impl TypeLookups) -> TypePy {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_py(field, _lookup);
    let should_wrap_stream_state = field.meta().streaming_behavior.state;

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    // Handle checks
    let checks: Vec<_> = field
        .meta()
        .constraints
        .iter()
        .filter_map(|c| {
            if matches!(c.level, ConstraintLevel::Check) {
                c.label.as_ref()
            } else {
                None
            }
        })
        .collect();

    let mut type_py: TypePy = match field {
        T::Primitive(type_value, _) => {
            let t: TypePy = type_value.into();
            t
        }
        T::Enum { name, dynamic, .. } => TypePy::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::Literal(literal_value, _) => TypePy::Literal(vec![match literal_value {
            baml_types::LiteralValue::String(val) => {
                LiteralValue::String(EscapedPythonString::new(val))
            }
            baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
            baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
        }]),
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
        },
        T::List(type_generic, _) => TypePy::List(Box::new(recursive_fn(type_generic))),
        T::Map(type_generic, type_generic1, _) => TypePy::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
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
        },
        T::Tuple(..) => TypePy::Any {
            reason: "tuples are not supported in Py".to_string(),
        },
        T::Arrow(..) => TypePy::Any {
            reason: "arrow types are not supported in Py".to_string(),
        },
        T::Union(union_type_generic, _union_meta) => {
            // Checks for Union are handled inside the match to support OneOfOptional ordering
            match union_type_generic.view() {
                baml_types::ir_type::UnionTypeViewGeneric::Null => TypePy::Any {
                    reason: "Null types are not supported in Py".to_string(),
                },
                baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                    // T | Null
                    // For single optional, we prefer Checked[Optional[T]]
                    let type_py = recursive_fn(type_generic);
                    type_py.make_optional()
                }
                baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                    // T1 | T2
                    let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                    TypePy::Union { variants: options }
                }
                baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                    // T1 | T2 | Null (Streaming Union)
                    // We prefer Optional[Checked[Union[T1, T2]]]
                    let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                    let t = TypePy::Union { variants: options };
                    t.make_optional()
                }
            }
        }
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the resolution phase."
        ),
    };

    if !checks.is_empty() {
        type_py = type_py.make_checked(checks);
    }

    // Wrap in StreamState if needed
    if should_wrap_stream_state {
        type_py.make_stream_state()
    } else {
        type_py
    }
}

pub fn type_to_py(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypePy {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_py(field, _lookup);

    let type_pkg = Package::types();

    let mut type_py = match field {
        T::Primitive(type_value, _) => {
            let t: TypePy = type_value.into();
            t
        }
        T::Enum { name, dynamic, .. } => TypePy::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::Literal(literal_value, _) => TypePy::Literal(vec![match literal_value {
            baml_types::LiteralValue::String(val) => {
                LiteralValue::String(EscapedPythonString::new(val))
            }
            baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
            baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
        }]),
        T::Class { name, dynamic, .. } => TypePy::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::List(type_generic, _) => TypePy::List(Box::new(recursive_fn(type_generic))),
        T::Map(type_generic, type_generic1, _) => TypePy::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
        ),
        T::Tuple(..) => TypePy::Any {
            reason: "tuples are not supported in Py".to_string(),
        },
        T::Arrow(..) => TypePy::Any {
            reason: "arrow types are not supported in Py".to_string(),
        },
        T::RecursiveTypeAlias { name, .. } => TypePy::TypeAlias {
            package: type_pkg.clone(),
            name: name.clone(),
        },
        T::Union(union_type_generic, _union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => {
                TypePy::Literal(vec![LiteralValue::None])
            }
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let type_py = recursive_fn(type_generic);
                type_py.make_optional()
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(&recursive_fn).collect();
                TypePy::Union { variants: options }
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let options: Vec<_> = type_generics.into_iter().map(recursive_fn).collect();
                TypePy::Union { variants: options }.make_optional()
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    // Handle checks
    let checks: Vec<_> = field
        .meta()
        .constraints
        .iter()
        .filter_map(|c| {
            if matches!(c.level, ConstraintLevel::Check) {
                c.label.as_ref()
            } else {
                None
            }
        })
        .collect();

    if !checks.is_empty() {
        type_py = type_py.make_checked(checks);
    }

    type_py
}

impl From<&TypeValue> for TypePy {
    fn from(type_value: &TypeValue) -> Self {
        match type_value {
            TypeValue::String => TypePy::String,
            TypeValue::Int => TypePy::Int,
            TypeValue::Float => TypePy::Float,
            TypeValue::Bool => TypePy::Bool,
            TypeValue::Null => TypePy::Any {
                reason: "Null types are not supported in Py".to_string(),
            },
            TypeValue::Media(baml_media_type) => TypePy::Media(baml_media_type.into()),
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
