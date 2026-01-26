use std::collections::HashSet;

use anyhow::Result;
use baml_types::{
    baml_value::TypeLookups,
    ir_type::{TypeIR, TypeStreaming},
    BamlValue, StreamingMode,
};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::{
    repr::IntermediateRepr, ClassWalker, EnumWalker, IRHelper, IRHelperExtended,
};
use internal_baml_jinja::types::{Class, Enum, Name, OutputFormatContent};

use crate::{
    runtime_context::{RuntimeClassOverride, RuntimeEnumOverride},
    RuntimeContext,
};

pub fn render_output_format(
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    output: &TypeIR,
    mode: StreamingMode,
) -> Result<OutputFormatContent> {
    let (enums, classes, recursive_classes, structural_recursive_aliases) =
        relevant_data_models(ir, output, ctx, mode == StreamingMode::Streaming)?;

    Ok(OutputFormatContent::target(output.clone())
        .enums(enums)
        .classes(classes)
        .recursive_classes(recursive_classes)
        .structural_recursive_aliases(structural_recursive_aliases)
        .build())
}

#[derive(Debug)]
enum OverridableValue<T> {
    Unset,
    SetEmpty,
    Set(T),
}

impl<T> OverridableValue<T> {
    fn value(self) -> Option<T> {
        match self {
            OverridableValue::Set(v) => Some(v),
            _ => None,
        }
    }
}

impl OverridableValue<bool> {
    fn from(v: Option<&BamlValue>) -> Self {
        match v {
            Some(v) => match v.as_bool() {
                Some(v) => OverridableValue::Set(v),
                None => OverridableValue::SetEmpty,
            },
            None => OverridableValue::Unset,
        }
    }
}

impl OverridableValue<String> {
    fn from(v: Option<&BamlValue>) -> Self {
        match v {
            Some(v) => match v.as_str() {
                Some(v) => OverridableValue::Set(v.to_string()),
                None => OverridableValue::SetEmpty,
            },
            None => OverridableValue::Unset,
        }
    }
}

fn find_new_class_field(
    class_name: &str,
    field_name: &str,
    class_walker: &Result<ClassWalker<'_>>,
    overrides: &RuntimeClassOverride,
    _ctx: &RuntimeContext,
) -> Result<Option<(Name, TypeIR, Option<String>, bool)>> {
    let Some(field_overrides) = overrides.new_fields.get(field_name) else {
        anyhow::bail!("Class {} does not have a field: {}", class_name, field_name);
    };

    // Ensure the original field does not exist
    if let Ok(class_walker) = class_walker {
        if class_walker.find_field(field_name).is_some() {
            anyhow::bail!(
                "Class {} already has a pre-defined field: {}",
                class_name,
                field_name
            );
        }
    }

    if let Some(true) = field_overrides.1.skip {
        return Ok(None);
    }

    let alias = OverridableValue::<String>::from(field_overrides.1.alias.as_ref());
    let desc = OverridableValue::<String>::from(field_overrides.1.meta.get("description"));

    let name = Name::new_with_alias(field_name.to_string(), alias.value());
    let desc = desc.value();

    Ok(Some((name, field_overrides.0.clone(), desc, false))) // TODO: Field overrides are not "stream.not_null". Should this be configurable?
}

fn find_existing_class_field(
    class_name: &str,
    field_name: &str,
    class_walker: &Result<ClassWalker<'_>>,
    overrides: &Option<&RuntimeClassOverride>,
    ctx: &RuntimeContext,
) -> Result<Option<(Name, TypeIR, Option<String>, bool)>> {
    let Ok(class_walker) = class_walker else {
        anyhow::bail!("Class {} does not exist", class_name);
    };

    let Some(field_walker) = class_walker.find_field(field_name) else {
        anyhow::bail!("Class {} does not have a field: {}", class_name, field_name);
    };

    let field_overrides = overrides.map(|o| o.update_fields.get(field_name)).flatten();

    let mut alias = OverridableValue::Unset;
    let mut desc = OverridableValue::Unset;
    let mut needed = OverridableValue::Unset;
    let mut skip = OverridableValue::Unset;

    if let Some(attrs) = field_overrides {
        match attrs.skip {
            Some(true) => return Ok(None),
            Some(false) => skip = OverridableValue::Set(false),
            None => {}
        }

        alias = OverridableValue::<String>::from(attrs.alias.as_ref());
        desc = OverridableValue::<String>::from(attrs.meta.get("description"));
        needed = OverridableValue::<bool>::from(attrs.meta.get("stream.not_null"));
    }

    let eval_ctx = ctx.eval_ctx(false);

    if field_walker.skip(&eval_ctx)? && !matches!(skip, OverridableValue::Set(false)) {
        return Ok(None);
    }

    if matches!(alias, OverridableValue::Unset) {
        if let Some(_alias) = field_walker.alias(&eval_ctx)? {
            alias = OverridableValue::Set(_alias);
        }
    }

    if matches!(desc, OverridableValue::Unset) {
        if let Some(_desc) = field_walker.description(&eval_ctx)? {
            desc = OverridableValue::Set(_desc);
        }
    }

    let name = Name::new_with_alias(field_name.to_string(), alias.value());
    let desc = desc.value();
    let r#type = field_walker.r#type();
    let needed = needed.value().unwrap_or(false);

    Ok(Some((name, r#type.clone(), desc, needed)))
}

fn find_enum_value(
    enum_name: &str,
    value_name: &str,
    enum_walker: &Result<EnumWalker<'_>>,
    overrides: &Option<&RuntimeEnumOverride>,
    ctx: &RuntimeContext,
) -> Result<Option<(Name, Option<String>)>> {
    if enum_walker.is_err() && overrides.is_none() {
        anyhow::bail!("Enum {} does not exist", enum_name);
    }

    let value_walker = match enum_walker {
        Ok(e) => e.find_value(value_name),
        Err(_) => None,
    };

    let value_overrides = overrides.map(|o| o.values.get(value_name)).flatten();

    if value_overrides.is_none() && value_walker.is_none() {
        anyhow::bail!("Enum {} does not have a value: {}", enum_name, value_name);
    }

    let mut skip = OverridableValue::Unset;
    let mut alias = OverridableValue::Unset;
    let mut desc = OverridableValue::Unset;
    if let Some(attrs) = value_overrides {
        match attrs.skip {
            Some(true) => return Ok(None),
            Some(false) => skip = OverridableValue::Set(false),
            None => {}
        }

        alias = OverridableValue::<String>::from(attrs.alias.as_ref());
        desc = OverridableValue::<String>::from(attrs.meta.get("description"));
    }

    let eval_ctx = ctx.eval_ctx(false);
    if let Some(value) = value_walker {
        if value.skip(&eval_ctx)? && !matches!(skip, OverridableValue::Set(false)) {
            return Ok(None);
        }
        if matches!(alias, OverridableValue::Unset) {
            if let Some(_alias) = value.alias(&eval_ctx)? {
                alias = OverridableValue::Set(_alias);
            }
        }

        if matches!(desc, OverridableValue::Unset) {
            if let Some(_desc) = value.description(&eval_ctx)? {
                desc = OverridableValue::Set(_desc);
            }
        }
    }

    let name = Name::new_with_alias(value_name.to_string(), alias.value());
    let desc = desc.value();

    Ok(Some((name, desc)))
}

// TODO: This function is "almost" a duplicate of `relevant_dat_models` at
// baml-lib/jsonish/src/tests/mod.rs
//
// Should be refactored.
fn relevant_data_models<'a>(
    ir: &'a IntermediateRepr,
    output: &'a TypeIR,
    ctx: &RuntimeContext,
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
    let mut checked_types = HashSet::new();
    let mut enums = Vec::new();
    let mut classes = Vec::new();
    let mut recursive_classes = IndexSet::new();
    let mut structural_recursive_aliases = IndexMap::new();
    let mut stack: Vec<baml_types::TypeIR> = vec![output.clone()];

    // start.extend(ctx.type_alias_overrides.values().cloned());

    let eval_ctx = ctx.eval_ctx(false);

    while let Some(output) = stack.pop() {
        match &output {
            TypeIR::Enum {
                name: enm,
                dynamic: _,
                meta: ref metadata,
            } => {
                if checked_types.insert(output.to_string()) {
                    let overrides = ctx.enum_overrides.get(enm);
                    let walker = ir.find_enum(enm);
                    let real_values = walker
                        .as_ref()
                        .map(|e| e.walk_values().map(|v| v.name().to_string()))
                        .ok();
                    let override_values = overrides
                        .map(|o| o.values.keys().cloned())
                        .into_iter()
                        .flatten();
                    let values = real_values
                        .into_iter()
                        .flatten()
                        .chain(override_values)
                        .collect::<IndexSet<_>>()
                        .into_iter()
                        .map(|value| {
                            let meta = find_enum_value(enm, &value, &walker, &overrides, ctx)?;
                            Ok(meta)
                        })
                        .filter_map(|v| v.transpose())
                        .collect::<Result<Vec<_>>>()?;

                    let mut alias =
                        OverridableValue::<String>::from(overrides.and_then(|o| o.alias.as_ref()));

                    if matches!(alias, OverridableValue::Unset) {
                        if let Ok(walker) = walker {
                            if let Some(a) = walker.alias(&eval_ctx)? {
                                alias = OverridableValue::Set(a);
                            }
                        }
                    }

                    enums.push(Enum {
                        name: Name::new_with_alias(enm.to_string(), alias.value()),
                        values,
                        constraints: metadata.constraints.clone(),
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
                    stack.push(inner.clone());
                }
            }
            TypeIR::Map(k, ref v, _) => {
                if checked_types.insert(output.to_string()) {
                    let v = if partialize {
                        v.to_streaming_type(ir).to_ir_type()
                    } else {
                        v.as_ref().clone()
                    };
                    if !checked_types.contains(&k.to_string()) {
                        stack.push(k.as_ref().clone());
                    }
                    if !checked_types.contains(&v.to_string()) {
                        stack.push(v);
                    }
                }
            }
            TypeIR::Tuple(ref options, _) => {
                if checked_types.insert(output.to_string()) {
                    for inner in options {
                        if !checked_types.contains(&inner.to_string()) {
                            stack.push(inner.clone());
                        }
                    }
                }
            }
            TypeIR::Union(ref options, _) => {
                if checked_types.insert(output.to_string()) {
                    for inner in options.iter_include_null() {
                        if !checked_types.contains(&inner.to_string()) {
                            stack.push(inner.clone());
                        }
                    }
                }
            }
            TypeIR::Class {
                name: cls,
                mode,
                dynamic: _,
                meta: ref metadata,
            } => {
                if checked_types.insert(output.to_string()) {
                    let overrides = ctx.class_override.get(&cls.to_string());
                    let walker = ir.find_class(&cls.to_string());

                    let real_fields = walker
                        .as_ref()
                        .map(|e| e.walk_fields().map(|v| v.name().to_string()))
                        .ok();
                    let override_fields = overrides
                        .map(|o| o.update_fields.keys().cloned())
                        .into_iter()
                        .flatten();

                    let fields = real_fields
                        .into_iter()
                        .flatten()
                        .chain(override_fields)
                        .collect::<IndexSet<_>>()
                        .into_iter()
                        .map(|field| {
                            let meta =
                                find_existing_class_field(cls, &field, &walker, &overrides, ctx)?;
                            Ok(meta)
                        })
                        .filter_map(Result::transpose);

                    let new_fields = overrides
                        .map(|o| {
                            o.new_fields
                                .keys()
                                .map(|k| find_new_class_field(cls, k, &walker, o, ctx))
                                .filter_map(Result::transpose)
                        })
                        .into_iter()
                        .flatten();

                    let mut alias =
                        OverridableValue::<String>::from(overrides.and_then(|o| o.alias.as_ref()));

                    if matches!(alias, OverridableValue::Unset) {
                        if let Ok(walker) = walker {
                            if let Some(a) = walker.alias(&eval_ctx)? {
                                alias = OverridableValue::Set(a);
                            }
                        }
                    }

                    let mut description = OverridableValue::Unset;

                    // Check for runtime override first (future enhancement)
                    // if let Some(desc_override) = overrides.and_then(|o| o.meta.get("description")) {
                    //     description = OverridableValue::<String>::from(Some(desc_override));
                    // }

                    // Fall back to walker description
                    if matches!(description, OverridableValue::Unset) {
                        if let Ok(walker) = &walker {
                            if let Some(d) = walker.description(&eval_ctx)? {
                                description = OverridableValue::Set(d);
                            }
                        }
                    }

                    let fields = fields
                        .chain(new_fields)
                        .map(|field| {
                            let (name, t, desc, needed) = field?;
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
                            Ok((name, t, desc, needed))
                        })
                        .collect::<Result<Vec<_>>>()?;

                    for (_, t, _, _) in fields.iter().as_ref() {
                        if !checked_types.contains(&t.to_string()) {
                            stack.push(t.clone());
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

                    for cycle in &ctx.recursive_class_overrides {
                        if cycle.contains(cls) {
                            recursive_classes.extend(cycle.iter().map(ToOwned::to_owned));
                        }
                    }

                    classes.push(Class {
                        name: Name::new_with_alias(cls.to_string(), alias.value()),
                        description: description.value(),
                        namespace: if !metadata.streaming_behavior.done && partialize {
                            StreamingMode::Streaming
                        } else {
                            StreamingMode::NonStreaming
                        },
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
                            if structural_recursive_aliases
                                .insert(alias.to_owned(), target.clone())
                                .is_none()
                            {
                                stack.push(target.clone());
                            }
                        }
                    }
                }

                // Overrides.
                for cycle in &ctx.recursive_type_alias_overrides {
                    if cycle.contains_key(name) {
                        for (alias, target) in cycle.iter() {
                            if structural_recursive_aliases
                                .insert(alias.to_owned(), target.clone())
                                .is_none()
                            {
                                stack.push(target.clone());
                            }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use baml_ids::FunctionCallId;
    use internal_baml_core::feature_flags::FeatureFlags;
    use internal_baml_jinja::types::RenderOptions;

    use super::*;
    use crate::BamlRuntime;

    #[test_log::test]
    fn skipped_variants_are_not_rendered() {
        let files = vec![(
            "test-file.baml",
            r#"
          enum Foo {
            Bar
            Baz @skip
          }"#,
        )]
        .into_iter()
        .collect();
        let env_vars = HashMap::new();
        let baml_runtime =
            BamlRuntime::from_file_content(".", &files, env_vars.clone(), FeatureFlags::new())
                .unwrap();
        let ctx_manager = baml_runtime.create_ctx_manager(BamlValue::Null, None);
        let ctx: RuntimeContext = ctx_manager
            .create_ctx(None, None, env_vars.clone(), vec![FunctionCallId::new()])
            .unwrap();

        let field_type = TypeIR::r#enum("Foo");
        let render_output = render_output_format(
            baml_runtime.ir.as_ref(),
            &ctx,
            &field_type,
            StreamingMode::NonStreaming,
        )
        .unwrap();

        let foo_enum = render_output.find_enum("Foo").unwrap();
        assert_eq!(foo_enum.values[0].0.real_name(), "Bar".to_string());
        assert_eq!(foo_enum.values.len(), 1);

        let rendered = render_output
            .render(RenderOptions::default())
            .unwrap()
            .unwrap();
        println!("{rendered}");
        assert_eq!(
            rendered,
            r#"
Answer with any of the categories:
Foo
----
- Bar
        "#
            .trim()
        );
    }

    #[test]
    fn skipped_class_fields_are_not_rendered() {
        let files = vec![(
            "test-file.baml",
            r#"
class Example {
  keep string
  dont_keep string? @skip
}
            "#,
        )]
        .into_iter()
        .collect();
        let env_vars = HashMap::new();
        let baml_runtime =
            BamlRuntime::from_file_content(".", &files, env_vars.clone(), FeatureFlags::new())
                .unwrap();
        let ctx_manager = baml_runtime.create_ctx_manager(BamlValue::Null, None);
        let ctx: RuntimeContext = ctx_manager
            .create_ctx(None, None, env_vars.clone(), vec![FunctionCallId::new()])
            .unwrap();

        let field_type = TypeIR::class("Example");
        let render_output = render_output_format(
            baml_runtime.ir.as_ref(),
            &ctx,
            &field_type,
            StreamingMode::NonStreaming,
        )
        .unwrap();

        let rendered = render_output
            .render(RenderOptions::default())
            .unwrap()
            .unwrap();
        println!("{rendered}");

        assert_eq!(
            rendered,
            r#"
Answer in JSON using this schema:
{
  keep: string,
}
        "#
            .trim()
        )
    }

    #[test]
    fn test_render_output_format_aliases() {
        let files = vec![(
            "test-file.baml",
            r#"
enum Month {
  January
  February
  March
  April
  May
  June
  July
  August
  September
  October
  November
  December
}

class Date {
  day int
  month Month
  year Date?
}

type DateAlias = Date

class Education {
  from_date DateAlias
  to_date DateAlias | "current"
  school string
  description string
}

class Resume {
  education Education[]
}

            "#,
        )]
        .into_iter()
        .collect();
        let env_vars = HashMap::new();
        let baml_runtime =
            BamlRuntime::from_file_content(".", &files, env_vars.clone(), FeatureFlags::new())
                .unwrap();
        let ctx_manager = baml_runtime.create_ctx_manager(BamlValue::Null, None);
        let ctx: RuntimeContext = ctx_manager
            .create_ctx(None, None, env_vars.clone(), vec![FunctionCallId::new()])
            .unwrap();

        let field_type = TypeIR::class("Resume");
        let render_output = render_output_format(
            baml_runtime.ir.as_ref(),
            &ctx,
            &field_type,
            StreamingMode::NonStreaming,
        )
        .unwrap();

        let rendered = render_output
            .render(RenderOptions::default())
            .unwrap()
            .unwrap();
        println!("{rendered}");

        assert_eq!(
            rendered,
            r#"
Month
----
- January
- February
- March
- April
- May
- June
- July
- August
- September
- October
- November
- December

Date {
  day: int,
  month: Month,
  year: Date or null,
}

Answer in JSON using this schema:
{
  education: [
    {
      from_date: Date,
      to_date: Date or "current",
      school: string,
      description: string,
    }
  ],
}
        "#
            .trim()
        )
    }

    #[test]
    fn test_render_output_format() {
        let files = vec![(
            "test-file.baml",
            r#"
enum Month {
  January
  February
  March
  April
  May
  June
  July
  August
  September
  October
  November
  December
}

class Date {
  day int
  month Month
  year int
}

class Education {
  from_date Date
  to_date Date | "current"
  school string
  description string
}

class Resume {
  education Education[]
}

            "#,
        )]
        .into_iter()
        .collect();
        let env_vars = HashMap::new();
        let baml_runtime =
            BamlRuntime::from_file_content(".", &files, env_vars.clone(), FeatureFlags::new())
                .unwrap();
        let ctx_manager = baml_runtime.create_ctx_manager(BamlValue::Null, None);
        let ctx: RuntimeContext = ctx_manager
            .create_ctx(None, None, env_vars.clone(), vec![FunctionCallId::new()])
            .unwrap();

        let field_type = TypeIR::class("Resume");
        let render_output = render_output_format(
            baml_runtime.ir.as_ref(),
            &ctx,
            &field_type,
            StreamingMode::NonStreaming,
        )
        .unwrap();

        let rendered = render_output
            .render(RenderOptions::default())
            .unwrap()
            .unwrap();
        println!("{rendered}");

        assert_eq!(
            rendered,
            r#"
Month
----
- January
- February
- March
- April
- May
- June
- July
- August
- September
- October
- November
- December

Answer in JSON using this schema:
{
  education: [
    {
      from_date: {
        day: int,
        month: Month,
        year: int,
      },
      to_date: {
        day: int,
        month: Month,
        year: int,
      } or "current",
      school: string,
      description: string,
    }
  ],
}
        "#
            .trim()
        )
    }

    #[test]
    fn test_render_output_format_description_and_alias() {
        let files = vec![(
            "test-file.baml",
            r#"
            class Foo {
                bar string @alias("a") @description("d")
            }
            "#,
        )]
        .into_iter()
        .collect();
        let env_vars = HashMap::new();
        let baml_runtime =
            BamlRuntime::from_file_content(".", &files, env_vars.clone(), FeatureFlags::new())
                .unwrap();
        let ctx_manager = baml_runtime.create_ctx_manager(BamlValue::Null, None);
        let ctx: RuntimeContext = ctx_manager
            .create_ctx(None, None, env_vars.clone(), vec![FunctionCallId::new()])
            .expect("Should create context");

        let field_type = TypeIR::class("Foo");
        let render_output = render_output_format(
            baml_runtime.ir.as_ref(),
            &ctx,
            &field_type,
            StreamingMode::NonStreaming,
        )
        .unwrap();

        let foo = render_output
            .find_class(&StreamingMode::NonStreaming, "Foo")
            .unwrap();
        assert_eq!(
            foo.fields[0].0,
            Name::new_with_alias("bar".to_string(), Some("a".to_string()))
        );
        assert_eq!(foo.fields[0].1, TypeIR::r#string());
        assert_eq!(foo.fields[0].2, Some("d".to_string()));
    }
}
