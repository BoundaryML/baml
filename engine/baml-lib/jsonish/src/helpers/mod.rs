pub mod common;
use std::{collections::HashSet, path::PathBuf};

use anyhow::Result;
use baml_types::{EvaluationContext, JinjaExpression};
use baml_types::{BamlValueWithMeta, ResponseCheck, StreamingBehavior};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::{
    ast::Field,
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, ClassWalker, EnumWalker, FieldType, IRHelper, TypeValue},
    validate,
};
use internal_baml_jinja::types::{Builder, Name, OutputFormatContent};
use internal_baml_jinja::types::{Class, Enum};

use crate::deserializer::deserialize_flags::{constraint_results, Flag};
use crate::deserializer::semantic_streaming::validate_streaming_state;
use crate::{BamlValueWithFlags, ResponseBamlValue};

pub fn load_test_ir(file_content: &str) -> IntermediateRepr {
    let mut schema = validate(
        &PathBuf::from("./baml_src"),
        vec![SourceFile::from((
            PathBuf::from("./baml_src/example.baml"),
            file_content.to_string(),
        ))],
    );
    match schema.diagnostics.to_result() {
        Ok(_) => {}
        Err(e) => {
            panic!("Failed to validate schema: {}", e);
        }
    }

    IntermediateRepr::from_parser_database(&schema.db, schema.configuration).unwrap()
}

pub fn render_output_format(
    ir: &IntermediateRepr,
    output: &FieldType,
    env_values: &EvaluationContext<'_>,
) -> Result<OutputFormatContent> {
    let (enums, classes, recursive_classes, structural_recursive_aliases) =
        relevant_data_models(ir, output, env_values)?;

    Ok(OutputFormatContent::target(output.clone())
        .enums(enums)
        .classes(classes)
        .recursive_classes(recursive_classes)
        .structural_recursive_aliases(structural_recursive_aliases)
        .build())
}

fn find_existing_class_field(
    class_name: &str,
    field_name: &str,
    class_walker: &Result<ClassWalker<'_>>,
    env_values: &EvaluationContext<'_>,
) -> Result<(Name, FieldType, Option<String>, bool)> {
    let Ok(class_walker) = class_walker else {
        anyhow::bail!("Class {} does not exist", class_name);
    };

    let Some(field_walker) = class_walker.find_field(field_name) else {
        anyhow::bail!("Class {} does not have a field: {}", class_name, field_name);
    };

    let name = Name::new_with_alias(field_name.to_string(), field_walker.alias(env_values)?);
    let desc = field_walker.description(env_values)?;
    let r#type = field_walker.r#type();
    let streaming_needed = field_walker
        .item
        .attributes
        .get("stream.not_null")
        .is_some();
    Ok((name, r#type.clone(), desc, streaming_needed))
}

fn find_enum_value(
    enum_name: &str,
    value_name: &str,
    enum_walker: &Result<EnumWalker<'_>>,
    env_values: &EvaluationContext<'_>,
) -> Result<Option<(Name, Option<String>)>> {
    if enum_walker.is_err() {
        anyhow::bail!("Enum {} does not exist", enum_name);
    }

    let value_walker = match enum_walker {
        Ok(e) => e.find_value(value_name),
        Err(_) => None,
    };

    let value_walker = match value_walker {
        Some(v) => v,
        None => return Ok(None),
    };

    if value_walker.skip(env_values)? {
        return Ok(None);
    }

    let name = Name::new_with_alias(value_name.to_string(), value_walker.alias(env_values)?);
    let desc = value_walker.description(env_values)?;

    Ok(Some((name, desc)))
}

// TODO: This function is "almost" a duplicate of `relevant_data_models` at
// baml-runtime/src/internal/prompt_renderer/render_output_format.rs
//
// Should be refactored.
//
// TODO: (Greg) Is the use of `String` as a hash key safe? Is there some way to
// get a collision that results in some type not getting put onto the stack?
fn relevant_data_models<'a>(
    ir: &'a IntermediateRepr,
    output: &'a FieldType,
    env_values: &EvaluationContext<'_>,
) -> Result<(
    Vec<Enum>,
    Vec<Class>,
    IndexSet<String>,
    IndexMap<String, FieldType>,
)> {
    let mut checked_types: HashSet<String> = HashSet::new();
    let mut enums = Vec::new();
    let mut classes: Vec<Class> = Vec::new();
    let mut recursive_classes = IndexSet::new();
    let mut structural_recursive_aliases = IndexMap::new();
    let mut start: Vec<baml_types::FieldType> = vec![output.clone()];

    while let Some(output) = start.pop() {
        match ir.distribute_constraints(&output) {
            (FieldType::Enum(enm), constraints) => {
                if checked_types.insert(output.to_string()) {
                    let walker = ir.find_enum(enm);

                    let real_values = walker
                        .as_ref()
                        .map(|e| e.walk_values().map(|v| v.name().to_string()))
                        .ok();
                    let values = real_values
                        .into_iter()
                        .flatten()
                        .map(|value| {
                            let meta = find_enum_value(enm.as_str(), &value, &walker, env_values)?;
                            Ok(meta)
                        })
                        .filter_map(|v| v.transpose())
                        .collect::<Result<Vec<_>>>()?;

                    enums.push(Enum {
                        name: Name::new_with_alias(enm.to_string(), walker?.alias(env_values)?),
                        values,
                        constraints,
                    });
                }
            }
            (FieldType::List(inner), _constraints) | (FieldType::Optional(inner), _constraints) => {
                if !checked_types.contains(&inner.to_string()) {
                    start.push(inner.as_ref().clone());
                }
            }
            (FieldType::Map(k, v), _constraints) => {
                if checked_types.insert(output.to_string()) {
                    if !checked_types.contains(&k.to_string()) {
                        start.push(k.as_ref().clone());
                    }
                    if !checked_types.contains(&v.to_string()) {
                        start.push(v.as_ref().clone());
                    }
                }
            }
            (FieldType::Tuple(options), _constraints)
            | (FieldType::Union(options), _constraints) => {
                if checked_types.insert(output.to_string()) {
                    for inner in options {
                        if !checked_types.contains(&inner.to_string()) {
                            start.push(inner.clone());
                        }
                    }
                }
            }
            (FieldType::Class(cls), constraints) => {
                if checked_types.insert(output.to_string()) {
                    let walker = ir.find_class(cls);

                    let real_fields = walker
                        .as_ref()
                        .map(|e| e.walk_fields().map(|v| v.name().to_string()))
                        .ok();

                    let fields = real_fields.into_iter().flatten().map(|field| {
                        let meta = find_existing_class_field(cls, &field, &walker, env_values)?;
                        Ok(meta)
                    });

                    let fields = fields.collect::<Result<Vec<_>>>()?;

                    for (_, t, _, _) in fields.iter().as_ref() {
                        if !checked_types.contains(&t.to_string()) {
                            start.push(t.clone());
                        }
                    }

                    // TODO: O(n) algorithm. Maybe a Merge-Find Set can optimize
                    // this to O(log n) or something like that
                    // (maybe, IDK though ¯\_(ツ)_/¯)
                    //
                    // Also there's a lot of cloning in this process of going
                    // from Parser DB to IR to Jinja Output Format, not only
                    // with recursive classes but also the rest of models.
                    // There's room for optimization here.
                    //
                    // Also take a look at the TODO on top of this function.
                    for cycle in ir.finite_recursive_cycles() {
                        if cycle.contains(cls) {
                            recursive_classes.extend(cycle.iter().map(ToOwned::to_owned));
                        }
                    }

                    classes.push(Class {
                        name: Name::new_with_alias(cls.to_string(), walker?.alias(env_values)?),
                        fields,
                        constraints,
                        streaming_behavior: StreamingBehavior::default(),
                    });
                }
            }
            (FieldType::RecursiveTypeAlias(name), _) => {
                // TODO: Same O(n) problem as above.
                for cycle in ir.structural_recursive_alias_cycles() {
                    if cycle.contains_key(name) {
                        for (alias, target) in cycle.iter() {
                            structural_recursive_aliases.insert(alias.to_owned(), target.clone());
                        }
                    }
                }
            }
            (FieldType::Literal(_), _) => {}
            (FieldType::Primitive(_), _constraints) => {}
            (_, _) => {
                // TODO: Don't use this wildcard.
                unreachable!("It is guaranteed that a call to distribute_constraints will not return FieldType::Constrained")
            }
        }
    }

    Ok((
        enums,
        classes,
        recursive_classes,
        structural_recursive_aliases,
    ))
}

/// Validate a parsed value, checking asserts and checks.
pub fn parsed_value_to_response(
    ir: &IntermediateRepr,
    baml_value: BamlValueWithFlags,
    field_type: &FieldType,
    allow_partials: bool,
) -> Result<ResponseBamlValue> {

    let meta_flags: BamlValueWithMeta<Vec<Flag>> = baml_value.clone().into();
    let baml_value_with_meta: BamlValueWithMeta<Vec<(String, JinjaExpression, bool)>> =
        baml_value.clone().into();

    let value_with_response_checks: BamlValueWithMeta<Vec<ResponseCheck>> = baml_value_with_meta
        .map_meta(|cs| {
            cs.iter()
                .map(|(label, expr, result)| {
                    let status = (if *result { "succeeded" } else { "failed" }).to_string();
                    ResponseCheck {
                        name: label.clone(),
                        expression: expr.0.clone(),
                        status,
                    }
                })
                .collect()
        });

    let baml_value_with_streaming =
        validate_streaming_state(ir, &baml_value, field_type, allow_partials)
            .map_err(|s| anyhow::anyhow!("{s:?}"))?;

    // Combine the baml_value, its types, the parser flags, and the streaming state
    // into a final value.
    // Node that we set the StreamState to `None` unless `allow_partials`.
    let response_value = baml_value_with_streaming
        .zip_meta(&value_with_response_checks)?
        .zip_meta(&meta_flags)?
        .map_meta(|((x, y), z)| (z.clone(), y.clone(), x.clone() ));
    Ok(ResponseBamlValue(response_value))
}
