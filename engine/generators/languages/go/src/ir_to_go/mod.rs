use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeNonStreaming, TypeStreaming},
    BamlMediaType, ConstraintLevel, TypeValue,
};

use crate::{
    package::Package,
    r#type::{MediaTypeGo, TypeGo},
};

pub mod classes;
pub mod enums;
pub mod functions;
pub mod type_aliases;
pub mod unions;

pub(crate) fn stream_type_to_go(field: &TypeStreaming, lookup: &impl TypeLookups) -> TypeGo {
    use TypeStreaming as T;
    let recursive_fn = |field| stream_type_to_go(field, lookup);

    // Check if this field has check constraints
    let field_has_checks = field
        .meta()
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    // Check if this field needs StreamState wrapping
    let field_has_stream_state = field.meta().streaming_behavior.state;

    let types_pkg: Package = Package::types();
    let stream_pkg: Package = Package::stream_types();

    let type_go: TypeGo = match field {
        T::Primitive(type_value, _) => type_value.into(),
        T::Enum { name, dynamic, .. } => TypeGo::Enum {
            package: types_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeGo::String(Some(val.clone())),
            baml_types::LiteralValue::Int(val) => TypeGo::Int(Some(*val)),
            baml_types::LiteralValue::Bool(val) => TypeGo::Bool(Some(*val)),
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
        },
        T::List(type_generic, _) => TypeGo::List(Box::new(recursive_fn(type_generic))),
        T::Map(type_generic, type_generic1, _) => TypeGo::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
        ),
        T::RecursiveTypeAlias {
            name,
            meta: alias_meta,
            ..
        } => {
            if lookup.expand_recursive_type(name).is_err() {
                TypeGo::Any {
                    reason: format!("Recursive type alias {name} is not supported in Go"),
                }
            } else {
                TypeGo::TypeAlias {
                    package: match alias_meta.streaming_behavior.done {
                        true => types_pkg.clone(),
                        false => stream_pkg.clone(),
                    },
                    name: name.clone(),
                }
            }
        }
        T::Tuple(..) => TypeGo::Any {
            reason: "tuples are not supported in Go".to_string(),
        },
        T::Arrow(..) => TypeGo::Any {
            reason: "arrow types are not supported in Go".to_string(),
        },
        T::Union(union_type_generic, union_meta) => {
            let has_union_checks = union_meta
                .constraints
                .iter()
                .any(|c| matches!(c.level, ConstraintLevel::Check));
            let has_union_stream_state = union_meta.streaming_behavior.state;

            match union_type_generic.view() {
                baml_types::ir_type::UnionTypeViewGeneric::Null => TypeGo::Null,
                baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                    let mut type_go = recursive_fn(type_generic);
                    // Order: inner type -> Optional -> Checked -> StreamState
                    type_go = type_go.make_optional();
                    if has_union_checks {
                        type_go = type_go.make_checked();
                    }
                    if has_union_stream_state {
                        type_go = type_go.make_stream_state();
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
                    let mut union_type = TypeGo::Union {
                        package: match field.mode(&baml_types::StreamingMode::Streaming, lookup, 1)
                        {
                            Ok(baml_types::StreamingMode::NonStreaming) => types_pkg.clone(),
                            Ok(baml_types::StreamingMode::Streaming) => stream_pkg.clone(),
                            Err(e) => {
                                return TypeGo::Any {
                                    reason: format!("Failed to get mode for field type: {e}"),
                                }
                            }
                        },
                        name: format!("Union{num_options}{name}"),
                    };
                    // Order: Union -> Checked -> StreamState
                    if has_union_checks {
                        union_type = union_type.make_checked();
                    }
                    if has_union_stream_state {
                        union_type = union_type.make_stream_state();
                    }
                    union_type
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
                    let mut union_type = TypeGo::Union {
                        package: match field.mode(&baml_types::StreamingMode::Streaming, lookup, 1)
                        {
                            Ok(baml_types::StreamingMode::NonStreaming) => types_pkg.clone(),
                            Ok(baml_types::StreamingMode::Streaming) => stream_pkg.clone(),
                            Err(e) => {
                                return TypeGo::Any {
                                    reason: format!("Failed to get mode for field type: {e}"),
                                }
                            }
                        },
                        name: format!("Union{num_options}{name}"),
                    };
                    // Order: Union -> Checked -> Optional -> StreamState
                    if has_union_checks {
                        union_type = union_type.make_checked();
                    }
                    union_type = union_type.make_optional();
                    if has_union_stream_state {
                        union_type = union_type.make_stream_state();
                    }
                    union_type
                }
            }
        }
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    // For non-union types, apply wrappers based on field metadata
    // Union types handle their own wrapping in their match arms
    if matches!(field, T::Union(..)) {
        return type_go;
    }

    // Order: base type -> Checked -> StreamState
    let type_go = if field_has_checks {
        type_go.make_checked()
    } else {
        type_go
    };

    if field_has_stream_state {
        type_go.make_stream_state()
    } else {
        type_go
    }
}

pub(crate) fn type_to_go(field: &TypeNonStreaming, _lookup: &impl TypeLookups) -> TypeGo {
    use TypeNonStreaming as T;
    let recursive_fn = |field| type_to_go(field, _lookup);

    // Check if this field has check constraints
    let field_has_checks = field
        .meta()
        .constraints
        .iter()
        .any(|c| matches!(c.level, ConstraintLevel::Check));

    let type_pkg = Package::types();

    let type_go = match field {
        T::Primitive(type_value, _) => type_value.into(),
        T::Enum { name, dynamic, .. } => TypeGo::Enum {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::Literal(literal_value, _) => match literal_value {
            baml_types::LiteralValue::String(val) => TypeGo::String(Some(val.clone())),
            baml_types::LiteralValue::Int(val) => TypeGo::Int(Some(*val)),
            baml_types::LiteralValue::Bool(val) => TypeGo::Bool(Some(*val)),
        },
        T::Class { name, dynamic, .. } => TypeGo::Class {
            package: type_pkg.clone(),
            name: name.clone(),
            dynamic: *dynamic,
        },
        T::List(type_generic, _) => TypeGo::List(Box::new(recursive_fn(type_generic))),
        T::Map(type_generic, type_generic1, _) => TypeGo::Map(
            Box::new(recursive_fn(type_generic)),
            Box::new(recursive_fn(type_generic1)),
        ),
        T::Tuple(..) => TypeGo::Any {
            reason: "tuples are not supported in Go".to_string(),
        },
        T::Arrow(..) => TypeGo::Any {
            reason: "arrow types are not supported in Go".to_string(),
        },
        T::RecursiveTypeAlias { name, .. } => {
            if _lookup.expand_recursive_type(name).is_err() {
                TypeGo::Any {
                    reason: format!("Recursive type alias {name} is not supported in Go"),
                }
            } else {
                TypeGo::TypeAlias {
                    package: type_pkg.clone(),
                    name: name.clone(),
                }
            }
        }
        T::Union(union_type_generic, union_meta) => {
            let has_union_checks = union_meta
                .constraints
                .iter()
                .any(|c| matches!(c.level, ConstraintLevel::Check));

            match union_type_generic.view() {
                baml_types::ir_type::UnionTypeViewGeneric::Null => TypeGo::Null,
                baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                    let mut type_go = recursive_fn(type_generic);
                    // Order: inner type -> Optional -> Checked
                    type_go = type_go.make_optional();
                    if has_union_checks {
                        type_go = type_go.make_checked();
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
                    let mut union_type = TypeGo::Union {
                        package: type_pkg.clone(),
                        name: format!("Union{num_options}{name}"),
                    };
                    if has_union_checks {
                        union_type = union_type.make_checked();
                    }
                    union_type
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
                    let mut union_type = TypeGo::Union {
                        package: type_pkg.clone(),
                        name: format!("Union{num_options}{name}"),
                    };
                    // Order: Union -> Checked -> Optional
                    if has_union_checks {
                        union_type = union_type.make_checked();
                    }
                    union_type = union_type.make_optional();
                    union_type
                }
            }
        }
        T::Top(_) => panic!(
            "TypeGeneric::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    // For non-union types, wrap with Checked if the field has check constraints
    // Union types handle their own check wrapping in their match arms
    if field_has_checks && !matches!(field, T::Union(..)) {
        type_go.make_checked()
    } else {
        type_go
    }
}

impl From<&TypeValue> for TypeGo {
    fn from(type_value: &TypeValue) -> Self {
        match type_value {
            TypeValue::String => TypeGo::String(None),
            TypeValue::Int => TypeGo::Int(None),
            TypeValue::Float => TypeGo::Float,
            TypeValue::Bool => TypeGo::Bool(None),
            TypeValue::Null => TypeGo::Null,
            TypeValue::Media(baml_media_type) => TypeGo::Media(baml_media_type.into()),
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
