use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    type_meta::{self, stream::TypeMetaStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{LiteralValue, MediaTypeTS, TypeTS},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;

pub(crate) fn stream_type_to_ts(field: &TypeStreaming, _lookup: &impl TypeLookups) -> TypeTS {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_ts(field, _lookup);
    let (check_names, wrap_stream_state) = stream_meta_to_ts_with_checks(field.meta());

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    let type_ts: TypeTS = match field {
        T::Primitive(type_value, _) => type_value.into(),
        T::Enum { name, dynamic, .. } => TypeTS::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::Literal(literal_value, _) => {
            let val = match literal_value {
                baml_types::LiteralValue::String(val) => LiteralValue::String(val.to_string()),
                baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
                baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
            };
            TypeTS::Literal(val)
        }
        T::Class {
            name,
            dynamic,
            meta: cls_meta,
            ..
        } => TypeTS::Class {
            package: match cls_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::List(type_generic, _) => TypeTS::List(Box::new(recursive_fn(type_generic))),
        T::Map(type_generic, type_generic1, _) => TypeTS::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
        ),
        T::RecursiveTypeAlias {
            name,
            meta: alias_meta,
            ..
        } => TypeTS::TypeAlias {
            package: match alias_meta.streaming_behavior.done {
                true => types_pkg.clone(),
                false => stream_pkg.clone(),
            },
            name: name.clone(),
        },
        T::Tuple(..) => TypeTS::Any {
            reason: "tuples are not supported in TypeScript".to_string(),
        },
        T::Arrow(..) => TypeTS::Any {
            reason: "arrow types are not supported in TypeScript".to_string(),
        },
        T::Union(union_type_generic, _union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeTS::Any {
                reason: "Null types are not supported in TypeScript".to_string(),
            },
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_ts = recursive_fn(type_generic);
                type_ts = type_ts.make_optional();
                // Note: stream state wrapping is now handled at the end of the function
                // based on the top-level meta's streaming_behavior.state
                type_ts
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => TypeTS::Union {
                variants: type_generics.into_iter().map(&recursive_fn).collect(),
            },
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let union = TypeTS::Union {
                    variants: type_generics.into_iter().map(&recursive_fn).collect(),
                };
                union.make_optional()
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    // Apply checked wrapper if there are checks on this type
    let type_ts = if let Some(names) = check_names {
        type_ts.make_checked(names)
    } else {
        type_ts
    };

    // Apply stream state wrapper if needed
    if wrap_stream_state {
        type_ts.make_stream_state()
    } else {
        type_ts
    }
}

pub(crate) fn type_to_ts(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypeTS {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_ts(field, _lookup);
    let check_names = meta_to_check_names(field.meta());

    let type_pkg = Package::types();

    let type_ts = match field {
        T::Primitive(type_value, _) => match type_value {
            TypeValue::String => TypeTS::String,
            TypeValue::Int => TypeTS::Int,
            TypeValue::Float => TypeTS::Float,
            TypeValue::Bool => TypeTS::Bool,
            TypeValue::Null => TypeTS::Any {
                reason: "Null types are not supported in TypeScript".to_string(),
            },
            TypeValue::Media(baml_media_type) => TypeTS::Media(baml_media_type.into()),
        },
        T::Enum { name, dynamic, .. } => TypeTS::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::Literal(literal_value, _) => TypeTS::Literal(match literal_value {
            baml_types::LiteralValue::String(val) => LiteralValue::String(val.to_string()),
            baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
            baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
        }),
        T::Class { name, dynamic, .. } => TypeTS::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::List(type_generic, _) => TypeTS::List(Box::new(recursive_fn(type_generic))),
        T::Map(type_generic, type_generic1, _) => TypeTS::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
        ),
        T::Tuple(..) => TypeTS::Any {
            reason: "tuples are not supported in TypeScript".to_string(),
        },
        T::Arrow(..) => TypeTS::Any {
            reason: "arrow types are not supported in TypeScript".to_string(),
        },
        T::RecursiveTypeAlias { name, .. } => TypeTS::TypeAlias {
            package: type_pkg.clone(),
            name: name.clone(),
        },
        T::Union(union_type_generic, _union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeTS::Any {
                reason: "Null types are not supported in TypeScript".to_string(),
            },
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_ts = recursive_fn(type_generic);
                type_ts = type_ts.make_optional();
                type_ts
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => TypeTS::Union {
                variants: type_generics.into_iter().map(&recursive_fn).collect(),
            },
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let union = TypeTS::Union {
                    variants: type_generics.into_iter().map(&recursive_fn).collect(),
                };
                union.make_optional()
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    // Apply checked wrapper if there are checks on this type
    if let Some(names) = check_names {
        type_ts.make_checked(names)
    } else {
        type_ts
    }
}

// Extract check names from ir metadata
fn meta_to_check_names(meta: &type_meta::NonStreaming) -> Option<Vec<Option<String>>> {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    if has_checks {
        Some(
            meta.constraints
                .iter()
                .map(|c| c.label.as_ref().map(|l| l.to_string()))
                .collect(),
        )
    } else {
        None
    }
}

// Extract check names and stream state flag from streaming ir metadata
fn stream_meta_to_ts_with_checks(meta: &TypeMetaStreaming) -> (Option<Vec<Option<String>>>, bool) {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let check_names = if has_checks {
        Some(
            meta.constraints
                .iter()
                .map(|c| c.label.as_ref().map(|l| l.to_string()))
                .collect(),
        )
    } else {
        None
    };

    (check_names, meta.streaming_behavior.state)
}

impl From<&TypeValue> for TypeTS {
    fn from(type_value: &TypeValue) -> Self {
        match type_value {
            TypeValue::String => TypeTS::String,
            TypeValue::Int => TypeTS::Int,
            TypeValue::Float => TypeTS::Float,
            TypeValue::Bool => TypeTS::Bool,
            TypeValue::Null => TypeTS::Any {
                reason: "Null types are not supported in TypeScript".to_string(),
            },
            TypeValue::Media(baml_media_type) => TypeTS::Media(baml_media_type.into()),
        }
    }
}

impl From<&BamlMediaType> for MediaTypeTS {
    fn from(baml_media_type: &BamlMediaType) -> Self {
        match baml_media_type {
            BamlMediaType::Image => MediaTypeTS::Image,
            BamlMediaType::Audio => MediaTypeTS::Audio,
            BamlMediaType::Pdf => MediaTypeTS::Pdf,
            BamlMediaType::Video => MediaTypeTS::Video,
        }
    }
}

#[cfg(test)]
mod tests {}
