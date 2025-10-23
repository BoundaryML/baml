pub mod common;
use std::{collections::HashSet, path::PathBuf};

use anyhow::Result;
use baml_types::{
    type_meta::base::StreamingBehavior, BamlValueWithMeta, EvaluationContext, JinjaExpression,
    ResponseCheck,
};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::{
    ast::Field,
    internal_baml_diagnostics::SourceFile,
    ir::{
        repr::IntermediateRepr, ClassWalker, EnumWalker, IRHelper, IRHelperExtended, TypeIR,
        TypeValue,
    },
    validate,
};
use internal_baml_jinja::types::{Builder, Class, Enum, Name, OutputFormatContent};

use crate::{
    deserializer::{
        deserialize_flags::{constraint_results, Flag},
        semantic_streaming::validate_streaming_state,
    },
    BamlValueWithFlags, ResponseBamlValue,
};

pub fn load_test_ir(file_content: &str) -> IntermediateRepr {
    let mut schema = validate(
        &PathBuf::from("./baml_src"),
        vec![SourceFile::from((
            PathBuf::from("./baml_src/example.baml"),
            file_content.to_string(),
        ))],
        internal_baml_core::FeatureFlags::new(),
    );
    match schema.diagnostics.to_result() {
        Ok(_) => {}
        Err(e) => {
            panic!("Failed to validate schema: {e}");
        }
    }

    IntermediateRepr::from_parser_database(&schema.db, schema.configuration).unwrap()
}

pub fn render_output_format(
    ir: &IntermediateRepr,
    output: &TypeIR,
    env_values: &EvaluationContext<'_>,
    streaming_mode: baml_types::StreamingMode,
) -> Result<OutputFormatContent> {
    let (enums, classes, recursive_classes, structural_recursive_aliases) = relevant_data_models(
        ir,
        output,
        env_values,
        streaming_mode == baml_types::StreamingMode::Streaming,
    )?;

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
) -> Result<(Name, TypeIR, Option<String>, bool)> {
    let Ok(class_walker) = class_walker else {
        anyhow::bail!("Class {} does not exist", class_name);
    };

    let Some(field_walker) = class_walker.find_field(field_name) else {
        anyhow::bail!("Class {} does not have a field: {}", class_name, field_name);
    };

    let name = Name::new_with_alias(field_name.to_string(), field_walker.alias(env_values)?);
    let desc = field_walker.description(env_values)?;
    let r#type = field_walker.r#type();
    let streaming_needed = field_walker.item.attributes.streaming_behavior().needed;
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
    output: &'a TypeIR,
    env_values: &EvaluationContext<'_>,
    partialize: bool,
) -> Result<(
    Vec<Enum>,
    Vec<Class>,
    IndexSet<String>,
    IndexMap<String, TypeIR>,
)> {
    let output = if partialize {
        output.to_streaming_type(ir).to_ir_type()
    } else {
        output.clone()
    };
    let mut checked_types: HashSet<String> = HashSet::new();
    let mut enums = Vec::new();
    let mut classes: Vec<Class> = Vec::new();
    let mut recursive_classes = IndexSet::new();
    let mut structural_recursive_aliases = IndexMap::new();
    let mut start: Vec<baml_types::TypeIR> = vec![output.clone()];

    while let Some(output) = start.pop() {
        match &output {
            TypeIR::Enum {
                name,
                dynamic,
                meta,
            } => {
                if checked_types.insert(output.to_string()) {
                    let walker = ir.find_enum(name);

                    let real_values = walker
                        .as_ref()
                        .map(|e| e.walk_values().map(|v| v.name().to_string()))
                        .ok();
                    let values = real_values
                        .into_iter()
                        .flatten()
                        .map(|value| find_enum_value(name, &value, &walker, env_values))
                        .filter_map(|v| v.transpose())
                        .collect::<Result<Vec<_>>>()?;

                    enums.push(Enum {
                        name: Name::new_with_alias(name.to_string(), walker?.alias(env_values)?),
                        values,
                        constraints: meta.constraints.clone(),
                    });
                }
            }
            TypeIR::List(inner, _) => {
                let inner = if partialize {
                    &inner.to_streaming_type(ir).to_ir_type()
                } else {
                    inner
                };
                if !checked_types.contains(&inner.to_string()) {
                    start.push(inner.clone());
                }
            }
            TypeIR::Map(k, v, _) => {
                if checked_types.insert(output.to_string()) {
                    if !checked_types.contains(&k.to_string()) {
                        start.push(k.as_ref().clone());
                    }
                    let v = if partialize {
                        &v.to_streaming_type(ir).to_ir_type()
                    } else {
                        v
                    };
                    if !checked_types.contains(&v.to_string()) {
                        start.push(v.clone());
                    }
                }
            }
            TypeIR::Tuple(options, _) => {
                if checked_types.insert(output.to_string()) {
                    for inner in options {
                        if !checked_types.contains(&inner.to_string()) {
                            start.push(inner.clone());
                        }
                    }
                }
            }
            TypeIR::Union(options, _) => {
                if checked_types.insert(output.to_string()) {
                    for inner in options.iter_skip_null() {
                        if !checked_types.contains(&inner.to_string()) {
                            start.push(inner.clone());
                        }
                    }
                }
            }
            TypeIR::Class {
                name,
                mode,
                dynamic,
                meta: metadata,
            } => {
                if checked_types.insert(output.to_string()) {
                    let walker = ir.find_class(name);

                    let real_fields = walker
                        .as_ref()
                        .map(|e| e.walk_fields().map(|v| v.name().to_string()))
                        .ok();

                    let fields = real_fields
                        .into_iter()
                        .flatten()
                        .map(|field| find_existing_class_field(name, &field, &walker, env_values))
                        .map(|field| {
                            let (name, t, prop1, needed) = field?;
                            let t = if partialize {
                                if metadata.streaming_behavior.done {
                                    let mut t = t;
                                    t.meta_mut().streaming_behavior.needed = true;
                                    t
                                } else {
                                    t.to_streaming_type(ir).to_ir_type()
                                }
                            } else {
                                t
                            };
                            Ok((name, t, prop1, needed))
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
                        if cycle.contains(name) {
                            recursive_classes.extend(cycle.iter().map(ToOwned::to_owned));
                        }
                    }

                    classes.push(Class {
                        name: Name::new_with_alias(name.to_string(), walker?.alias(env_values)?),
                        description: None,
                        namespace: *mode,
                        fields,
                        constraints: metadata.constraints.clone(),
                        streaming_behavior: metadata.streaming_behavior.clone(),
                    });
                }
            }
            TypeIR::RecursiveTypeAlias { name, .. } => {
                // TODO: Same O(n) problem as above.
                for cycle in ir.structural_recursive_alias_cycles() {
                    if cycle.contains_key(name) {
                        for (alias, target) in cycle.iter() {
                            structural_recursive_aliases.insert(alias.to_owned(), target.clone());
                        }
                    }
                }
            }
            TypeIR::Literal(_, _) => {}
            TypeIR::Primitive(_, _) => {}
            TypeIR::Arrow(_, _) => {}
            TypeIR::Top(_) => panic!(
                "TypeIR::Top should have been resolved by the compiler before code generation. \
                 This indicates a bug in the type resolution phase."
            ),
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
/// This is largely a duplicate of baml-runtime::internal::llm_client::parsed_value_to_response.
/// It's used in jsonish tests.
pub fn parsed_value_to_response(
    ir: &IntermediateRepr,
    baml_value: BamlValueWithFlags,
    mode: baml_types::StreamingMode,
) -> Result<ResponseBamlValue> {
    let meta_flags: BamlValueWithMeta<Vec<Flag>> = baml_value.clone().into();
    let baml_value_with_meta: BamlValueWithMeta<Vec<(String, JinjaExpression, bool)>> =
        baml_value.clone().into();
    let meta_field_type: BamlValueWithMeta<TypeIR> = baml_value.clone().into();

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

    let baml_value_with_streaming = validate_streaming_state(ir, &baml_value, mode)
        .map_err(|s| anyhow::anyhow!("Parsing failed due to: {s:?}"))?;

    // Combine the baml_value, its types, the parser flags, and the streaming state
    // into a final value.
    // Node that we set the StreamState to `None` unless `allow_partials`.
    let response_value = baml_value_with_streaming
        .zip_meta(&value_with_response_checks)?
        .zip_meta(&meta_flags)?
        .zip_meta(&meta_field_type)?
        .map_meta(|(((x, y), z), ft)| {
            crate::ResponseValueMeta(z.clone(), y.clone(), x.clone(), ft.clone())
        });
    Ok(ResponseBamlValue(response_value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_description_and_alias() {
        let ir = load_test_ir(
            r#"
          class Foo {
            bar string @description("d") @alias("a")
          }
        "#,
        );
        let output = render_output_format(
            &ir,
            &TypeIR::class("Foo"),
            &EvaluationContext::default(),
            baml_types::StreamingMode::NonStreaming,
        )
        .expect("Rendering should work");
        let foo = output
            .classes
            .get(&("Foo".to_string(), baml_types::StreamingMode::NonStreaming))
            .expect("Exists");
        assert_eq!(foo.fields.len(), 1);
        assert_eq!(foo.fields[0].2, Some("d".to_string()));
        assert_eq!(
            foo.fields[0].0,
            Name::new_with_alias("bar".to_string(), Some("a".to_string()))
        );
    }
}
