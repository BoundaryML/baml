mod generate_types;
mod python_language_features;

use anyhow::Result;
use askama::Template;
use either::Either;
use internal_baml_core::ir::{repr::IntermediateRepr, FieldType};
use std::path::Path;

use self::python_language_features::{PythonLanguageFeatures, ToPython};
use crate::dir_writer::FileCollector;

#[derive(askama::Template)]
#[template(path = "client.py.j2", escape = "none")]
struct PythonClient {
    funcs: Vec<PythonFunction>,
}
struct PythonFunction {
    name: String,
    return_type: String,
    args: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "__init__.py.j2", escape = "none")]
struct PythonInit {
    encoded_baml_src: String,
}

pub(crate) fn generate(ir: &IntermediateRepr, generator: &crate::GeneratorArgs) -> Result<()> {
    let mut collector = FileCollector::<PythonLanguageFeatures>::new();

    collector.add_file(
        "types.py",
        TryInto::<generate_types::PythonTypes>::try_into(ir)
            .map_err(|e| e.context("Error while building types.py"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "/*\n\n{:?}\n\n*/",
                        anyhow::Error::new(e).context("Error while rendering types.py")
                    )
                },
                |r| r,
            ),
    );

    collector.add_file(
        "client.py",
        TryInto::<PythonClient>::try_into(ir)
            .map_err(|e| e.context("Error while building client.py"))?
            .render()
            .map_or_else(
                |e| {
                    format!(
                        "/*\n\n{:?}\n\n*/",
                        anyhow::Error::new(e).context("Error while rendering client.py")
                    )
                },
                |r| r,
            ),
    );

    collector.add_file(
        "__init__.py",
        PythonInit {
            encoded_baml_src: generator
                .encoded_baml_files
                .clone()
                .unwrap_or("".to_string()),
        }
        .render()
        .map_or_else(
            |e| {
                format!(
                    "/*\n\n{:?}\n\n*/",
                    anyhow::Error::new(e).context("Error while rendering client.py")
                )
            },
            |r| r,
        ),
    );

    collector.commit(&generator.output_root)?;

    Ok(())
}

impl TryFrom<&IntermediateRepr> for PythonClient {
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
                        let (function, impl_) = c.item;
                        Ok(PythonFunction {
                            name: f.name().to_string(),
                            return_type: f.elem().output().to_type_hint(),
                            args: match f.inputs() {
                                either::Either::Left(args) => anyhow::bail!("Python codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_type_hint()))
                                    .collect(),
                            },
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<PythonFunction>>>>()?
            .into_iter()
            .flatten().collect();
        Ok(PythonClient { funcs: functions })
    }
}

trait ToTypeHint {
    fn to_type_hint(&self) -> String;
}

impl ToTypeHint for FieldType {
    fn to_type_hint(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("types.{name}"),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_hint()),
            FieldType::Map(key, value) => {
                format!("Dict[{}, {}]", key.to_type_hint(), value.to_type_hint())
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_hint())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_hint())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("Optional[{}]", inner.to_type_hint()),
        }
    }
}
