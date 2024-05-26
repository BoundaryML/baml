mod generate_types;
mod typescript_language_features;

use std::path::PathBuf;

use anyhow::Result;
use either::Either;
use indexmap::IndexMap;
use internal_baml_core::ir::{repr::IntermediateRepr, FieldType};

use self::typescript_language_features::{ToTypescript, TypescriptLanguageFeatures};
use crate::dir_writer::FileCollector;

#[derive(askama::Template)]
#[template(path = "client.ts.j2", escape = "none")]
struct TypescriptClient {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}
struct TypescriptFunction {
    name: String,
    partial_return_type: String,
    return_type: String,
    args: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "index.ts.j2", escape = "none")]
struct TypescriptInit {}

#[derive(askama::Template)]
#[template(path = "globals.ts.j2", escape = "none")]
struct TypescriptGlobals {
    // In TS, we always have baml_src at ./baml_src
}

#[derive(askama::Template)]
#[template(path = "inlinedbaml.ts.j2", escape = "none")]
struct InlinedBaml {
    filemap_base64: String,
}

#[derive(askama::Template)]
#[template(path = "tracing.ts.j2", escape = "none")]
struct TypescriptTracing {}

pub(crate) fn generate(
    ir: &IntermediateRepr,
    generator: &crate::GeneratorArgs,
) -> Result<IndexMap<PathBuf, String>> {
    let mut collector = FileCollector::<TypescriptLanguageFeatures>::new();
    collector.add_template::<generate_types::TypescriptTypes>("types.ts", (ir, generator))?;
    collector.add_template::<TypescriptClient>("client.ts", (ir, generator))?;
    collector.add_template::<TypescriptGlobals>("globals.ts", (ir, generator))?;
    collector.add_template::<TypescriptTracing>("tracing.ts", (ir, generator))?;
    collector.add_template::<TypescriptInit>("index.ts", (ir, generator))?;
    collector.add_template::<InlinedBaml>("inlinedbaml.ts", (ir, generator))?;

    collector.commit(&generator.output_dir())
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypescriptClient {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        let functions = ir
            .walk_functions()
            .map(|f| {
                let Either::Right(configs) = f.walk_impls() else {
                    return Ok(vec![]);
                };
                let funcs = configs
                    .map(|c| {
                        let (_function, _impl_) = c.item;
                        Ok(TypescriptFunction {
                            name: f.name().to_string(),
                            return_type: f.elem().output().to_type_ref(),
                            partial_return_type: f.elem().output().to_partial_type_ref(),
                            args: match f.inputs() {
                                either::Either::Left(_args) => anyhow::bail!("Typescript codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_type_ref()))
                                    .collect(),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<TypescriptFunction>>>>()?
            .into_iter()
            .flatten().collect();

        let types = ir
            .walk_classes()
            .map(|c| c.name().to_string())
            .chain(ir.walk_enums().map(|e| e.name().to_string()))
            .collect();
        Ok(TypescriptClient {
            funcs: functions,
            types,
        })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for InlinedBaml {
    type Error = anyhow::Error;

    fn try_from((_ir, args): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(InlinedBaml {
            filemap_base64: args.input_file_map_base64.to_string(),
        })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypescriptGlobals {
    type Error = anyhow::Error;

    fn try_from((_, _): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(TypescriptGlobals {})
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypescriptTracing {
    type Error = anyhow::Error;

    fn try_from(_: (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(TypescriptTracing {})
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypescriptInit {
    type Error = anyhow::Error;

    fn try_from(_: (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(TypescriptInit {})
    }
}

trait ToTypeReferenceInClientDefinition {
    fn to_type_ref(&self) -> String;

    fn to_partial_type_ref(&self) -> String;
}

impl ToTypeReferenceInClientDefinition for FieldType {
    fn to_partial_type_ref(&self) -> String {
        match self {
            FieldType::Enum(name) => format!("({name} | null)"),
            FieldType::Class(name) => format!("(Partial<{name}> | null)"),
            FieldType::List(inner) => format!("{}[]", inner.to_partial_type_ref()),
            FieldType::Map(key, value) => {
                format!(
                    "(Record<{}, {}> | null)",
                    key.to_type_ref(),
                    value.to_partial_type_ref()
                )
            }
            FieldType::Primitive(r#type) => format!("({} | null)", r#type.to_typescript()),
            FieldType::Union(inner) => format!(
                "({} | null)",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            FieldType::Tuple(inner) => format!(
                "([{}] | null)",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("({} | null)", inner.to_partial_type_ref()),
        }
    }

    fn to_type_ref(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("{name}"),
            FieldType::List(inner) => match inner.as_ref() {
                FieldType::Union(_) | FieldType::Optional(_) => {
                    format!("({})[]", inner.to_type_ref())
                }
                _ => format!("{}[]", inner.to_type_ref()),
            },
            FieldType::Map(key, value) => {
                format!("Record<{}, {}>", key.to_type_ref(), value.to_type_ref())
            }
            FieldType::Primitive(r#type) => r#type.to_typescript(),
            FieldType::Union(inner) => format!(
                "{}",
                inner
                    .iter()
                    .map(|t| t.to_type_ref())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            FieldType::Tuple(inner) => format!(
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("{} | null", inner.to_type_ref()),
        }
    }
}
