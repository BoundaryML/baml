use std::collections::{HashMap, HashSet};

use anyhow::Result;
use internal_baml_core::ir::{repr::IntermediateRepr, FieldType, IRHelper};

use crate::RuntimeContext;

pub struct OutputFormat {
    pub comment: Option<String>,
    pub output: OutputFormatType,
}

pub enum OutputFormatType {
    Literal(String),
    Primitive(String),
    Enum(String),
    Class(String),
    List(Box<OutputFormatType>),
    Map(Vec<(OutputFormatType, OutputFormat)>),
    Union(Vec<OutputFormat>),
    Tuple(Vec<OutputFormat>),
    Optional(Box<OutputFormatType>),
}

pub fn render_output_format(
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    output: &FieldType,
) -> Result<String> {
    let (enums, classes) = relevant_data_models(ir, output, ctx)?;

    todo!()
}

struct Name {
    name: String,
    rendered_name: Option<String>,
}

impl Name {
    fn new(name: String) -> Self {
        Self {
            name,
            rendered_name: None,
        }
    }

    fn new_with_alias(name: String, alias: Option<String>) -> Self {
        Self {
            name,
            rendered_name: alias,
        }
    }

    fn rendered_name(&self) -> &str {
        self.rendered_name.as_ref().unwrap_or(&self.name)
    }
}

struct Class<'a> {
    name: Name,
    // type and description
    fields: Vec<(Name, &'a FieldType, Option<String>)>,
}

struct Enum {
    name: Name,
    // name and description
    values: Vec<(Name, Option<String>)>,
}

fn find_value<'a>(name: &str, values: &'a [(Name, Option<String>)]) -> Option<&'a Name> {
    values
        .iter()
        .find_map(|(n, _)| if n.name == name { Some(n) } else { None })
}

fn relevant_data_models<'a>(
    ir: &'a IntermediateRepr,
    output: &'a FieldType,
    ctx: &RuntimeContext,
) -> Result<(Vec<Enum>, Vec<Class<'a>>)> {
    let mut checked_types = HashSet::new();
    let mut enums = Vec::new();
    let mut classes = Vec::new();
    let mut start = vec![output];
    checked_types.insert(output.to_string());

    while !start.is_empty() {
        let output = start.pop().unwrap();
        match output {
            FieldType::Enum(enm) => {
                if checked_types.insert(output.to_string()) {
                    let overrides = ctx.enum_overrides.get(enm);
                    let enm = ir.find_enum(enm)?;

                    let mut values = HashMap::new();
                    if let Some(overrides) = overrides {
                        for (value, attrs) in &overrides.values {
                            if attrs.skip.unwrap_or(false) {
                                values.insert(value.as_str(), None);
                            } else {
                                values.insert(
                                    value.as_str(),
                                    Some((
                                        attrs.alias.clone(),
                                        attrs.meta.get("description").map(|v| v.as_str()),
                                    )),
                                );
                            }
                        }
                    }

                    let values = enm
                        .walk_values()
                        .map(|v| {
                            if v.skip(&ctx.env)? {
                                return Ok(None);
                            }
                            Ok(Some((
                                Name::new_with_alias(v.name().to_string(), v.alias(&ctx.env)?),
                                v.description(&ctx.env)?,
                            )))
                        })
                        .filter_map(|v| v.transpose())
                        .collect::<Result<_>>()?;
                    enums.push(Enum {
                        name: Name::new_with_alias(enm.name().to_string(), enm.alias(&ctx.env)?),
                        values,
                    });
                }
            }
            FieldType::List(inner) | FieldType::Optional(inner) => {
                if checked_types.insert(inner.to_string()) {
                    start.push(inner);
                }
            }
            FieldType::Map(k, v) => {
                if checked_types.insert(k.to_string()) {
                    start.push(k);
                }
                if checked_types.insert(v.to_string()) {
                    start.push(v);
                }
            }
            FieldType::Tuple(options) | FieldType::Union(options) => {
                for option in options {
                    if checked_types.insert(option.to_string()) {
                        start.push(option);
                    }
                }
            }
            FieldType::Class(cls) => {
                if checked_types.insert(cls.clone()) {
                    let class = ir.find_class(cls)?;

                    let fields = class
                        .walk_fields()
                        .map(|field| {
                            Ok((
                                Name::new_with_alias(
                                    field.name().to_string(),
                                    field.alias(&ctx.env)?,
                                ),
                                field.r#type(),
                                field.description(&ctx.env)?,
                            ))
                        })
                        .collect::<Result<_>>()?;

                    classes.push(Class {
                        name: Name::new_with_alias(
                            class.name().to_string(),
                            class.alias(&ctx.env)?,
                        ),
                        fields,
                    });
                }
            }
            FieldType::Primitive(_) => {}
        }
    }

    Ok((enums, classes))
}

fn compute_output_format(
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    output: &FieldType,
) -> Result<OutputFormat> {
    Ok(match output {
        FieldType::Primitive(t) => OutputFormat {
            comment: None,
            output: OutputFormatType::Primitive(t.to_string()),
        },
        FieldType::Enum(enm) => OutputFormat {
            comment: None,
            output: OutputFormatType::Enum(enm.to_string()),
        },
        FieldType::Class(cls) => OutputFormat {
            comment: None,
            output: OutputFormatType::Class(cls.to_string()),
        },

        FieldType::Optional(inner) => {
            let mut inner = compute_output_format(ir, ctx, inner)?;
            OutputFormat {
                comment: inner.comment.take(),
                output: OutputFormatType::Optional(Box::new(inner.output)),
            }
        }
        FieldType::List(inner) => {
            let mut inner = compute_output_format(ir, ctx, inner)?;
            OutputFormat {
                comment: inner.comment.take(),
                output: OutputFormatType::List(Box::new(inner.output)),
            }
        }

        FieldType::Map(k, v) => {
            let key = compute_output_format(ir, ctx, k)?;
            let value = compute_output_format(ir, ctx, v)?;
            OutputFormat {
                comment: None,
                output: OutputFormatType::Map(vec![(key.output, value)]),
            }
        }

        FieldType::Union(options) => {
            let options = options
                .iter()
                .map(|option| compute_output_format(ir, ctx, option))
                .collect::<Result<Vec<_>>>()?;
            OutputFormat {
                comment: None,
                output: OutputFormatType::Union(options),
            }
        }
        FieldType::Tuple(options) => {
            let options = options
                .iter()
                .map(|option| compute_output_format(ir, ctx, option))
                .collect::<Result<Vec<_>>>()?;
            OutputFormat {
                comment: None,
                output: OutputFormatType::Tuple(options),
            }
        }
    })
}
