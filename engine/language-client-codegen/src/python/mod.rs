mod generate_types;
mod python_language_features;

use std::path::PathBuf;

use anyhow::Result;
use askama::Template;
use either::Either;
use indexmap::IndexMap;
use internal_baml_core::ir::{repr::IntermediateRepr, FieldType};

use self::python_language_features::{PythonLanguageFeatures, ToPython};
use crate::dir_writer::FileCollector;

#[derive(askama::Template)]
#[template(path = "client.py.j2", escape = "none")]
struct PythonClient {
    funcs: Vec<PythonFunction>,
}
struct PythonFunction {
    name: String,
    partial_return_type: String,
    return_type: String,
    args: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "__init__.py.j2", escape = "none")]
struct PythonInit {}

#[derive(askama::Template)]
#[template(path = "globals.py.j2", escape = "none")]
struct PythonGlobals {}

#[derive(askama::Template)]
#[template(path = "tracing.py.j2", escape = "none")]
struct PythonTracing {}

#[derive(askama::Template)]
#[template(path = "inlinedbaml.py.j2", escape = "none")]
struct InlinedBaml {
    file_map: Vec<(String, String)>,
}

pub(crate) fn generate(
    ir: &IntermediateRepr,
    generator: &crate::GeneratorArgs,
) -> Result<IndexMap<PathBuf, String>> {
    let mut collector = FileCollector::<PythonLanguageFeatures>::new();

    collector
        .add_template::<generate_types::PythonStreamTypes>("partial_types.py", (ir, generator))?;
    collector.add_template::<generate_types::PythonTypes>("types.py", (ir, generator))?;
    collector.add_template::<PythonClient>("client.py", (ir, generator))?;
    collector.add_template::<PythonGlobals>("globals.py", (ir, generator))?;
    collector.add_template::<PythonTracing>("tracing.py", (ir, generator))?;
    collector.add_template::<InlinedBaml>("inlinedbaml.py", (ir, generator))?;
    collector.add_template::<PythonInit>("__init__.py", (ir, generator))?;

    collector.commit(&generator.output_dir())
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonTracing {
    type Error = anyhow::Error;

    fn try_from(_: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(PythonTracing {})
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonInit {
    type Error = anyhow::Error;

    fn try_from(_: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(PythonInit {})
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonGlobals {
    type Error = anyhow::Error;

    fn try_from((_, args): (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(PythonGlobals {})
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for InlinedBaml {
    type Error = anyhow::Error;

    fn try_from((_ir, args): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(InlinedBaml {
            file_map: args
                .input_file_map
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        serde_json::to_string(v).expect("Failed to serialize file map"),
                    )
                })
                .collect(),
        })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonClient {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let functions = ir
            .walk_functions()
            .map(|f| {
                let Either::Right(configs) = f.walk_impls() else {
                    return Ok(vec![]);
                };
                let funcs = configs
                    .map(|c| {
                        let (_function, _impl_) = c.item;
                        Ok(PythonFunction {
                            name: f.name().to_string(),
                            partial_return_type: f.elem().output().to_partial_type_ref(),
                            return_type: f.elem().output().to_type_ref(),
                            args: match f.inputs() {
                                either::Either::Left(_args) => anyhow::bail!("Python codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
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
            .collect::<Result<Vec<Vec<PythonFunction>>>>()?
            .into_iter()
            .flatten().collect();
        Ok(PythonClient { funcs: functions })
    }
}

trait ToTypeReferenceInClientDefinition {
    fn to_type_ref(&self) -> String;

    fn to_partial_type_ref(&self) -> String;
}

impl ToTypeReferenceInClientDefinition for FieldType {
    fn to_type_ref(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("types.{name}"),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_ref()),
            FieldType::Map(key, value) => {
                format!("Dict[{}, {}]", key.to_type_ref(), value.to_type_ref())
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("Optional[{}]", inner.to_type_ref()),
        }
    }

    fn to_partial_type_ref(&self) -> String {
        match self {
            FieldType::Class(name) => format!("partial_types.{name}"),
            FieldType::Enum(name) => format!("Optional[types.{name}]"),
            FieldType::List(inner) => format!("List[{}]", inner.to_partial_type_ref()),
            FieldType::Map(key, value) => {
                format!(
                    "Dict[{}, {}]",
                    key.to_partial_type_ref(),
                    value.to_type_ref()
                )
            }
            FieldType::Primitive(r#type) => format!("Optional[{}]", r#type.to_python()),
            FieldType::Union(inner) => format!(
                "Optional[Union[{}]]",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Optional[Tuple[{}]]",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => inner.to_partial_type_ref(),
        }
    }
}
