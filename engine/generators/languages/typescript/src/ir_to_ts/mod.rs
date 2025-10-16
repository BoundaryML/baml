use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    type_meta::{self, stream::TypeMetaStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{LiteralValue, MediaTypeTS, TypeMetaTS, TypeTS, TypeWrapper},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;

pub(crate) fn stream_type_to_ts(field: &TypeStreaming, _lookup: &impl TypeLookups) -> TypeTS {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_ts(field, _lookup);
    let meta = stream_meta_to_ts(field.meta());

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    let type_ts: TypeTS = match field {
        T::Primitive(type_value, _) => {
            let t: TypeTS = type_value.into();
            t.with_meta(meta)
        }
        T::Enum { name, dynamic, .. } => TypeTS::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => {
            let val = match literal_value {
                baml_types::LiteralValue::String(val) => LiteralValue::String(val.to_string()),
                baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
                baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
            };
            TypeTS::Literal(val, meta)
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
            meta,
        },
        T::List(type_generic, _) => TypeTS::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeTS::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
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
            meta,
        },
        T::Tuple(..) => TypeTS::Any {
            reason: "tuples are not supported in Go".to_string(),
            meta,
        },
        T::Arrow(..) => TypeTS::Any {
            reason: "arrow types are not supported in Go".to_string(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeTS::Any {
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
                    type_go.meta_mut().make_checked(
                        union_meta
                            .constraints
                            .iter()
                            .map(|c| c.label.clone())
                            .collect(),
                    );
                }
                type_go.meta_mut().make_optional();
                if union_meta.streaming_behavior.state {
                    type_go.meta_mut().set_stream_state();
                }
                type_go
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => TypeTS::Union {
                variants: type_generics.into_iter().map(&recursive_fn).collect(),
                meta,
            },
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let mut meta = meta;
                meta.make_optional();
                TypeTS::Union {
                    variants: type_generics.into_iter().map(&recursive_fn).collect(),
                    meta,
                }
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_ts
}

pub(crate) fn type_to_ts(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypeTS {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_ts(field, _lookup);
    let meta = meta_to_ts(field.meta());

    let type_pkg = Package::types();

    let type_ts = match field {
        T::Primitive(type_value, _) => match type_value {
            TypeValue::String => TypeTS::String(meta),
            TypeValue::Int => TypeTS::Int(meta),
            TypeValue::Float => TypeTS::Float(meta),
            TypeValue::Bool => TypeTS::Bool(meta),
            TypeValue::Null => TypeTS::Any {
                reason: "Null types are not supported in Typescript".to_string(),
                meta,
            },
            TypeValue::Media(baml_media_type) => TypeTS::Media(baml_media_type.into(), meta),
        },
        T::Enum { name, dynamic, .. } => TypeTS::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::Literal(literal_value, _) => TypeTS::Literal(
            match literal_value {
                baml_types::LiteralValue::String(val) => LiteralValue::String(val.to_string()),
                baml_types::LiteralValue::Int(val) => LiteralValue::Int(*val),
                baml_types::LiteralValue::Bool(val) => LiteralValue::Bool(*val),
            },
            meta,
        ),
        T::Class { name, dynamic, .. } => TypeTS::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
            meta,
        },
        T::List(type_generic, _) => TypeTS::List(Box::new(recursive_fn(type_generic)), meta),
        T::Map(type_generic, type_generic1, _) => TypeTS::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
            meta,
        ),
        T::Tuple(..) => TypeTS::Any {
            reason: "tuples are not supported in Typescript".to_string(),
            meta,
        },
        T::Arrow(..) => TypeTS::Any {
            reason: "arrow types are not supported in Typescript".to_string(),
            meta,
        },
        T::RecursiveTypeAlias { name, .. } => TypeTS::TypeAlias {
            package: type_pkg.clone(),
            name: name.clone(),
            meta,
        },
        T::Union(union_type_generic, union_meta) => match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null => TypeTS::Any {
                reason: "Null types are not supported in Typescript".to_string(),
                meta,
            },
            baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                let mut type_ts = recursive_fn(type_generic);
                type_ts.meta_mut().make_optional();
                if union_meta
                    .constraints
                    .iter()
                    .any(|c| matches!(c.level, ConstraintLevel::Check))
                {
                    type_ts.meta_mut().make_checked(
                        union_meta
                            .constraints
                            .iter()
                            .map(|c| c.label.clone())
                            .collect(),
                    );
                }
                type_ts
            }
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => TypeTS::Union {
                variants: type_generics.into_iter().map(&recursive_fn).collect(),
                meta,
            },
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                let mut meta = meta;
                meta.make_optional();
                TypeTS::Union {
                    variants: type_generics.into_iter().map(&recursive_fn).collect(),
                    meta,
                }
            }
        },
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_ts
}

// convert ir metadata to go metadata
fn meta_to_ts(meta: &type_meta::NonStreaming) -> TypeMetaTS {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let wrapper = TypeWrapper::default();
    let wrapper = if has_checks {
        let names = meta
            .constraints
            .iter()
            .map(|c| c.label.as_ref().map(|l| l.to_string()))
            .collect();
        wrapper.wrap_with_checked(names)
    } else {
        wrapper
    };

    // optionality is handled by unions
    TypeMetaTS {
        type_wrapper: wrapper,
        wrap_stream_state: false,
    }
}

fn stream_meta_to_ts(meta: &TypeMetaStreaming) -> TypeMetaTS {
    let has_checks = meta
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let wrapper = TypeWrapper::default();
    let wrapper = if has_checks {
        wrapper.wrap_with_checked(
            meta.constraints
                .iter()
                .map(|c| c.label.as_ref().map(|l| l.to_string()))
                .collect(),
        )
    } else {
        wrapper
    };

    TypeMetaTS {
        type_wrapper: wrapper,
        wrap_stream_state: meta.streaming_behavior.state,
    }
}

impl From<&TypeValue> for TypeTS {
    fn from(type_value: &TypeValue) -> Self {
        let meta = TypeMetaTS::default();
        match type_value {
            TypeValue::String => TypeTS::String(meta),
            TypeValue::Int => TypeTS::Int(meta),
            TypeValue::Float => TypeTS::Float(meta),
            TypeValue::Bool => TypeTS::Bool(meta),
            TypeValue::Null => TypeTS::Any {
                reason: "Null types are not supported in Typescript".to_string(),
                meta,
            },
            TypeValue::Media(baml_media_type) => TypeTS::Media(baml_media_type.into(), meta),
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
