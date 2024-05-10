mod generate_types;
mod typescript_language_features;

use std::path::PathBuf;

use anyhow::Result;
use askama::Template;
use either::Either;
use internal_baml_core::ir::{repr::IntermediateRepr, FieldType};

use self::typescript_language_features::{ToTypescript, TypescriptLanguageFeatures};
use crate::dir_writer::FileCollector;

#[derive(askama::Template)]
#[template(path = "client.ts.j2", escape = "none")]
struct TypescriptClient {
    funcs: Vec<TypescriptFunction>,
}
struct TypescriptFunction {
    name: String,
    return_type: String,
    args: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "index.ts.j2", escape = "none")]
struct TypescriptInit {
    encoded_baml_src: String,
}

pub(crate) fn generate(
    ir: &IntermediateRepr,
    generator: &crate::GeneratorArgs,
) -> Result<Vec<PathBuf>> {
    let mut collector = FileCollector::<TypescriptLanguageFeatures>::new();

    collector.add_file(
        "types.ts",
        TryInto::<generate_types::TypescriptTypes>::try_into(ir)
            .map_err(|e| e.context("Error while building types.ts"))?
            .render()
            .map_err(|e| anyhow::Error::from(e).context("Error while rendering types.ts"))?,
    );

    collector.add_file(
        "client.ts",
        TryInto::<TypescriptClient>::try_into(ir)
            .map_err(|e| e.context("Error while building client.ts"))?
            .render()
            .map_err(|e| anyhow::Error::from(e).context("Error while rendering client.ts"))?,
    );

    collector.add_file(
        "index.ts",
        TypescriptInit {
            encoded_baml_src: generator
                .encoded_baml_files
                .clone()
                .unwrap_or("".to_string()),
        }
        .render()
        .map_err(|e| anyhow::Error::from(e).context("Error while rendering index.ts"))?,
    );

    collector.commit(&generator.output_root)
}

impl TryFrom<&IntermediateRepr> for TypescriptClient {
    type Error = anyhow::Error;

    fn try_from(ir: &IntermediateRepr) -> Result<Self> {
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
                            return_type: f.elem().output().to_type_reference(),
                            args: match f.inputs() {
                                either::Either::Left(_args) => anyhow::bail!("Typescript codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_type_reference()))
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
        Ok(TypescriptClient { funcs: functions })
    }
}

pub(super) trait ToTypeReference {
    fn to_type_reference(&self) -> String;
}

impl ToTypeReference for FieldType {
    fn to_type_reference(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("{name}"),
            FieldType::List(inner) => match inner.as_ref() {
                FieldType::Union(_) | FieldType::Optional(_) => {
                    format!("({})[]", inner.to_type_reference())
                }
                _ => format!("{}[]", inner.to_type_reference()),
            },
            FieldType::Map(key, value) => {
                format!(
                    "Record<{}, {}>",
                    key.to_type_reference(),
                    value.to_type_reference()
                )
            }
            FieldType::Primitive(r#type) => r#type.to_typescript(),
            FieldType::Union(inner) => format!(
                "{}",
                inner
                    .iter()
                    .map(|t| t.to_type_reference())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            FieldType::Tuple(inner) => format!(
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_reference())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("{} | null", inner.to_type_reference()),
        }
    }
}
