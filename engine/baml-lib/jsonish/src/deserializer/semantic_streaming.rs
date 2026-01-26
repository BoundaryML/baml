// This module helps resolve baml values with attached streaming state
// in the context of the streaming behavior associated with their types.

use std::collections::HashSet;

use anyhow::{Context, Error};
use baml_types::{
    BamlMap, BamlValueWithMeta, Completion, CompletionState, ResponseCheck, TypeIR, TypeValue,
};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::{
    ir_helpers::infer_type_with_meta,
    repr::{IntermediateRepr, Walker},
    Field, IRHelper, IRHelperExtended, IRSemanticStreamingHelper,
};
use thiserror;

use crate::{deserializer::coercer::ParsingError, BamlValueWithFlags, Flag};

#[derive(Debug, thiserror::Error)]
pub enum StreamingError {
    #[error("Expected to encounter a class")]
    ExpectedClass,
    #[error("Value was marked Done, but was incomplete in the stream")]
    IncompleteDoneValue,
    #[error("Class instance did not contain fields marked as needed: {fields:?}")]
    MissingNeededFields { fields: Vec<String> },
    #[error("Failed to distribute_type_with_meta: {0}")]
    DistributeTypeWithMetaFailure(#[from] anyhow::Error),
}

/// For a given baml value, traverse its nodes, comparing the completion state
/// of each node against the streaming behavior of the node's type.
pub fn validate_streaming_state(
    ir: &impl IRHelperExtended,
    baml_value: &BamlValueWithFlags,
    mode: baml_types::StreamingMode,
) -> Result<BamlValueWithMeta<Completion>, StreamingError> {
    let baml_value_with_meta_flags: BamlValueWithMeta<Vec<Flag>> = baml_value.clone().into();
    let typed_baml_value: BamlValueWithMeta<(Vec<Flag>, TypeIR)> =
        ir.distribute_type_with_meta(baml_value_with_meta_flags, baml_value.field_type().clone())?;
    let baml_value_with_streaming_state_and_behavior =
        typed_baml_value.map_meta(|(flags, r#type)| (completion_state(flags), r#type));

    let top_level_node = process_node(
        ir,
        baml_value_with_streaming_state_and_behavior,
        0,
        mode == baml_types::StreamingMode::NonStreaming,
    )?;
    Ok(top_level_node)
}

/// Consider a node's type, streaming state, and streaming behavior annotations. Return
/// an error if streaming state doesn't meet the streaming requirements. Also attach
/// the streaming state to the node as metadata, if this was requested by the user
/// vial `@stream.with_state`.
///
/// This function descends into child nodes when the argument is a compound value.
///
/// Params:
///   value: A node in the BamlValue tree.
///   allow_partials: Whether this node may contain partial values. (Once we
///                   see a false, all child nodes will also get false).
fn process_node(
    ir: &impl IRHelperExtended,
    value: BamlValueWithMeta<(CompletionState, &TypeIR)>,
    _depth: usize,
    // TODO(vbv): This is a hack to allow us to skip the done check for
    // non-streaming values.
    skip_done_check: bool,
) -> Result<BamlValueWithMeta<Completion>, StreamingError> {
    let (completion_state, field_type) = value.meta().clone();
    let metadata = field_type.meta();

    let must_be_done = required_done(ir, field_type, &value);
    let allow_partials_in_sub_nodes = !must_be_done;

    let new_meta = Completion {
        state: completion_state.clone(),
        display: metadata.streaming_behavior.state,
        required_done: must_be_done,
    };

    if !skip_done_check && must_be_done && !(completion_state == CompletionState::Complete) {
        return Err(StreamingError::IncompleteDoneValue);
    }

    let new_value = match value {
        BamlValueWithMeta::String(s, _) => Ok(BamlValueWithMeta::String(s, new_meta)),
        BamlValueWithMeta::Media(m, _) => Ok(BamlValueWithMeta::Media(m, new_meta)),
        BamlValueWithMeta::Null(_) => Ok(BamlValueWithMeta::Null(new_meta)),
        BamlValueWithMeta::Int(i, _) => Ok(BamlValueWithMeta::Int(i, new_meta)),
        BamlValueWithMeta::Float(f, _) => Ok(BamlValueWithMeta::Float(f, new_meta)),
        BamlValueWithMeta::Bool(b, _) => Ok(BamlValueWithMeta::Bool(b, new_meta)),
        BamlValueWithMeta::List(items, _) => Ok(BamlValueWithMeta::List(
            items
                .into_iter()
                .filter_map(|item| process_node(ir, item, _depth + 1, skip_done_check).ok())
                .collect(),
            new_meta,
        )),
        BamlValueWithMeta::Class(ref class_name, value_fields, _) => {
            let value_field_names: IndexSet<String> =
                value_fields.keys().map(String::to_owned).collect();
            let needed_fields: HashSet<String> =
                needed_fields(ir, class_name, allow_partials_in_sub_nodes)?;

            // The fields that need to be filled in by Null are initially the
            // fields in the Class type that are not present in the input
            // value.
            let fields_needing_null = fields_needing_null_filler(
                ir,
                class_name,
                value_field_names.iter().cloned().collect(),
            )?;

            // We might later delete fields from 'value_fields`, (e.g. if they
            // were incomplete but required `done`). These deleted fields will
            // need to be replaced with nulls. We initialize a map to hold
            // these nulls here.
            let mut deletion_nulls: BamlMap<String, BamlValueWithMeta<Completion>> = BamlMap::new();

            // Null values used to fill gaps in the input map.
            // Note: fields_needing_null contains fields that are NOT in value_fields,
            // so we must look up their types from the class definition in the IR.
            let class_field_types = ir.class_fields(class_name).unwrap_or_default();
            let filler_nulls = fields_needing_null
                .into_iter()
                .map(|ref null_field_name| {
                    // Get the field type from the class definition, not from value_fields
                    // (which doesn't contain these fields - that's why they need null fillers)
                    let use_state = class_field_types
                        .get(null_field_name)
                        .map(|field_type| field_type.meta().streaming_behavior.state)
                        .unwrap_or(false);
                    let field_stream_state = Completion {
                        state: CompletionState::Pending,
                        display: use_state,
                        required_done: false,
                    };
                    (
                        null_field_name.to_string(),
                        BamlValueWithMeta::Null(field_stream_state),
                    )
                })
                .collect::<IndexMap<String, BamlValueWithMeta<Completion>>>();

            // Fields of the input map, transformed by running the
            // semantic-streaming algorithm, and deleted if appropriate.
            let mut new_fields = value_fields
                .into_iter()
                .filter_map(|(field_name, field_value)| {
                    let with_state = field_value.meta().1.meta().streaming_behavior.state;
                    let completion_state = field_value.meta().0.clone();
                    match process_node(ir, field_value, _depth + 1, skip_done_check) {
                        Ok(res) => Some((field_name, res)),
                        _ => {
                            let state = Completion {
                                state: completion_state,
                                display: with_state,
                                required_done: false,
                            };
                            let null = BamlValueWithMeta::Null(state);
                            deletion_nulls.insert(field_name, null);
                            None
                        }
                    }
                })
                .collect::<IndexMap<String, BamlValueWithMeta<_>>>();

            // Names of fields from the input map that survived semantic streaming.
            let derived_present_nonnull_fields: HashSet<String> = new_fields
                .iter()
                .filter_map(|(field_name, field_value)| {
                    // Check if this field is required to be non-null
                    let is_needed_field = needed_fields.contains(field_name);

                    // Helper to check if a field value should be considered "effectively null"
                    // for the purposes of @stream.not_null validation
                    let is_effectively_null_for_validation =
                        |field_value: &BamlValueWithMeta<Completion>| -> bool {
                            match field_value {
                                BamlValueWithMeta::Null(_) if is_needed_field => {
                                    // For fields marked @stream.not_null, any null value (including
                                    // the null variant of a union) should be considered as missing the requirement
                                    true
                                }
                                BamlValueWithMeta::Class(_, class_fields, _) if is_needed_field => {
                                    // For needed fields, a class is considered "null" if all its fields are null
                                    class_fields
                                        .values()
                                        .all(|field| matches!(field, BamlValueWithMeta::Null(_)))
                                }
                                BamlValueWithMeta::Null(_) => false, // Not a needed field, so null is acceptable
                                _ => false,
                            }
                        };

                    if is_effectively_null_for_validation(field_value) {
                        None
                    } else {
                        Some(field_name.to_string())
                    }
                })
                .collect();
            let missing_needed_fields: Vec<_> = needed_fields
                .difference(&derived_present_nonnull_fields)
                .collect();

            new_fields.extend(filler_nulls);
            new_fields.extend(deletion_nulls);

            let class_definition_fields = type_field_names(ir, field_type);
            new_fields.sort_by(|k1, _v1, k2, _v2| {
                let index1 = class_definition_fields.get_index_of(k1);
                let index2 = class_definition_fields.get_index_of(k2);
                match (index1, index2) {
                    (Some(i1), Some(i2)) => i1.cmp(&i2),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });

            let res = BamlValueWithMeta::Class(class_name.clone(), new_fields, new_meta);
            if missing_needed_fields.is_empty() {
                Ok(res)
            } else {
                Err(StreamingError::MissingNeededFields {
                    fields: missing_needed_fields.into_iter().cloned().collect(),
                })
            }
        }
        BamlValueWithMeta::Enum(name, value, _) => {
            Ok(BamlValueWithMeta::Enum(name, value, new_meta))
        }
        BamlValueWithMeta::Map(kvs, _) => {
            let new_kvs = kvs
                .into_iter()
                .filter_map(|(k, v)| {
                    process_node(ir, v, _depth + 1, skip_done_check)
                        .ok()
                        .map(|v| (k, v))
                })
                .collect();
            Ok(BamlValueWithMeta::Map(new_kvs, new_meta))
        }
    };
    // let space = "    ".repeat(depth);
    // eprintln!("{space}PROCESS NODE\n{space}value\n{space}{value_copy:?}\n{space}new_value\n{space}{new_value:?}\n\n");
    new_value
}

/// Extract the field names from a field_type that is expected to be a `Class`.
/// If it is not a known class, return no field names.
fn type_field_names(ir: &impl IRHelperExtended, field_type: &TypeIR) -> IndexSet<String> {
    match field_type {
        TypeIR::Class {
            name: class_name, ..
        } => ir.class_field_names(class_name).unwrap_or_default(),
        _ => IndexSet::new(),
    }
}

/// Given a type and an input map, if that type is a class, determine what
/// fields in the class need to be filled in by a null. A field needs to be
/// filled by a null if it is not present in the map value.
fn fields_needing_null_filler<'a>(
    ir: &'a impl IRSemanticStreamingHelper,
    class_name: &'a str,
    value_names: HashSet<String>,
) -> Result<HashSet<String>, anyhow::Error> {
    ir.find_class_fields_needing_null_filler(class_name, &value_names)
}

/// For a given type, assume that it is a class, and list the fields of that
/// class that were marked `@stream.not_null`.
///
/// When allow_partials==false, we are in a context where we are done with
/// streaming, so we override the normal implemenation of this function
/// and return an empty set (because we are ignoring the "@stream.not_null" property,
/// which only applies when `allow_partials==true`).
fn needed_fields(
    ir: &impl IRHelperExtended,
    class_name: &str,
    allow_partials: bool,
) -> Result<HashSet<String>, anyhow::Error> {
    if !allow_partials {
        return Ok(HashSet::new());
    }
    ir.class_streaming_needed_fields(class_name)
        .map_err(|_| StreamingError::ExpectedClass)
        .context("needed_fields failed to lookup class")
}

/// Whether a type must be complete before being included as a node
/// in a streamed value.
fn required_done<T>(
    ir: &impl IRHelperExtended,
    field_type: &TypeIR,
    value: &BamlValueWithMeta<T>,
) -> bool {
    let metadata = field_type.meta();
    let type_implies_done = match field_type {
        TypeIR::Primitive(tv, _) => match tv {
            TypeValue::String => false,
            TypeValue::Int => true,
            TypeValue::Float => true,
            TypeValue::Media(_) => true,
            TypeValue::Bool => true,
            TypeValue::Null => true,
        },
        TypeIR::Literal { .. } => true,
        TypeIR::List(_, _) => false,
        TypeIR::Map(_, _, _) => false,
        TypeIR::Enum {
            name: _,
            dynamic: _,
            meta: _,
        } => true,
        TypeIR::Tuple(_, _) => false,
        TypeIR::RecursiveTypeAlias { .. } => false,
        TypeIR::Class { .. } => false,
        TypeIR::Union(options, _) => {
            let view = options.iter_skip_null();
            // Determining whether a union requires done is complicated.
            // If all the variants are required to be done, then the union
            // requires done.
            let all_require_done = view.iter().all(|option| required_done(ir, option, value));
            if all_require_done {
                return true;
            }

            // If none of the variants are required to be done, then the union
            // does not require done.
            let none_require_done = view.iter().all(|option| !required_done(ir, option, value));
            if none_require_done {
                return false;
            }

            // Otherwise, the answer depends on the value we are streaming.
            // Search for the variant that matches the value, and use the
            // required_done property of that variant.
            view.iter().any(|option| {
                let variant_required_done = required_done(ir, option, value);
                let value_unifies_with_variant =
                    infer_type_with_meta(value).is_some_and(|v| ir.is_subtype(&v, option));
                variant_required_done && value_unifies_with_variant
            })
        }
        TypeIR::Arrow(_, _) => false, // TODO: Error? Arrow shouldn't appear here.
        TypeIR::Top(_) => panic!(
            "TypeIR::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    type_implies_done || metadata.streaming_behavior.done
}

fn completion_state(flags: &[Flag]) -> CompletionState {
    if flags.iter().any(|f| matches!(f, Flag::Pending)) {
        CompletionState::Pending
    } else if flags.iter().any(|f| matches!(f, Flag::Incomplete)) {
        CompletionState::Incomplete
    } else {
        CompletionState::Complete
    }
}

#[cfg(test)]
mod tests {
    use baml_types::type_meta::base::TypeMeta;
    use internal_baml_core::ir::repr::make_test_ir;

    use super::*;
    use crate::deserializer::{deserialize_flags::DeserializerConditions, types::ValueWithFlags};

    fn mk_null() -> BamlValueWithFlags {
        BamlValueWithFlags::Null(
            TypeIR::Primitive(TypeValue::Null, TypeMeta::default()),
            DeserializerConditions::default(),
        )
    }

    fn mk_string(s: &str) -> BamlValueWithFlags {
        BamlValueWithFlags::String(ValueWithFlags {
            value: s.to_string(),
            target: TypeIR::Primitive(TypeValue::String, TypeMeta::default()),
            flags: DeserializerConditions::default(),
        })
    }
    fn mk_float(s: f64) -> BamlValueWithFlags {
        BamlValueWithFlags::Float(ValueWithFlags {
            value: s,
            target: TypeIR::Primitive(TypeValue::Float, TypeMeta::default()),
            flags: DeserializerConditions::default(),
        })
    }

    #[test]
    fn recursive_type_alias() {
        let ir = make_test_ir(
            r##"
        type A = A[]
        "##,
        )
        .unwrap();

        fn mk_list(items: Vec<BamlValueWithFlags>) -> BamlValueWithFlags {
            BamlValueWithFlags::List(
                DeserializerConditions::default(),
                TypeIR::recursive_type_alias("A").as_list(),
                items,
            )
        }

        let value = mk_list(vec![
            mk_list(vec![]),
            mk_list(vec![]),
            mk_list(vec![mk_list(vec![]), mk_list(vec![])]),
        ]);

        let res =
            validate_streaming_state(&ir, &value, baml_types::StreamingMode::NonStreaming).unwrap();

        assert_eq!(res.into_iter().count(), 6);
    }

    #[test]
    fn stable_keys() {
        let ir = make_test_ir(
            r##"
        class Address {
          street string
          state string
        }
        class Name {
          first string
          last string?
        }
        class Info {
          name Name
          address Address?
          hair_color string
          height float
        }
        "##,
        )
        .unwrap();

        let value = BamlValueWithFlags::Class(
            "Info".to_string(),
            DeserializerConditions::default(),
            TypeIR::class("Info"),
            vec![
                (
                    "name".to_string(),
                    BamlValueWithFlags::Class(
                        "Name".to_string(),
                        DeserializerConditions::default(),
                        TypeIR::class("Name"),
                        vec![
                            ("first".to_string(), mk_string("Greg")),
                            ("last".to_string(), mk_string("Hale")),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
                ("address".to_string(), mk_null()),
                ("hair_color".to_string(), mk_string("Grey")),
                ("height".to_string(), mk_float(1.75)),
            ]
            .into_iter()
            .collect(),
        );
        let field_type = TypeIR::class("Info");

        let res =
            validate_streaming_state(&ir, &value, baml_types::StreamingMode::NonStreaming).unwrap();

        // The first key should be "Name", matching the order specified in the
        // original value.
        match res {
            BamlValueWithMeta::Class(_name, fields, _meta) => {
                assert_eq!(fields.into_iter().next().unwrap().0.as_str(), "name");
            }
            _ => panic!("Expected Class"),
        }
    }
}
