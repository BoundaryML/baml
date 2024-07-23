mod generate_types;
mod python_language_features;

use std::path::PathBuf;

use anyhow::Result;
use either::Either;
use indexmap::IndexMap;
use internal_baml_core::{
    configuration::GeneratorDefaultClientMode,
    ir::{repr::IntermediateRepr, FieldType, IRHelper},
};

use self::python_language_features::{PythonLanguageFeatures, ToPython};
use crate::dir_writer::FileCollector;

struct PythonClient {
    funcs: Vec<PythonFunction>,
}

macro_rules! impl_from_python_client {
    ($($target:ident => $template:expr),+) => {
        $(
            #[derive(askama::Template)]
            #[template(path = $template, escape = "none")]
            struct $target {
                funcs: Vec<PythonFunction>,
            }

            impl From<PythonClient> for $target {
                fn from(client: PythonClient) -> Self {
                    Self {
                        funcs: client.funcs,
                    }
                }
            }

            impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for $target {
                type Error = anyhow::Error;

                fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
                    let python_client = PythonClient::try_from(params)?;
                    Ok(python_client.into())
                }
            }
        )+
    };
}

impl_from_python_client!(
    AsyncPythonClient => "async_client.py.j2",
    SyncPythonClient => "sync_client.py.j2",
    UnstableAsyncPythonClient => "unstable_async_client.py.j2"
);

struct PythonFunction {
    name: String,
    partial_return_type: String,
    return_type: String,
    args: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "__init__.py.j2", escape = "none")]
struct PythonInit {
    default_client_mode: GeneratorDefaultClientMode,
}

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
    collector.add_template::<generate_types::TypeBuilder>("type_builder.py", (ir, generator))?;
    collector.add_template::<AsyncPythonClient>("async_client.py", (ir, generator))?;
    collector
        .add_template::<UnstableAsyncPythonClient>("unstable_async_client.py", (ir, generator))?;
    collector.add_template::<SyncPythonClient>("sync_client.py", (ir, generator))?;
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

    fn try_from((_, gen): (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(PythonInit {
            default_client_mode: gen.default_client_mode.clone(),
        })
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
            file_map: args.file_map()?,
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
                            partial_return_type: f.elem().output().to_partial_type_ref(ir),
                            return_type: f.elem().output().to_type_ref(ir),
                            args: match f.inputs() {
                                either::Either::Left(_args) => anyhow::bail!("Python codegen does not support unnamed args: please add names to all arguments of BAML function '{}'", f.name().to_string()),
                                either::Either::Right(args) => args
                                    .iter()
                                    .map(|(name, r#type)| (name.to_string(), r#type.to_type_ref(ir)))
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
    fn to_type_ref(&self, ir: &IntermediateRepr) -> String;

    fn to_partial_type_ref(&self, ir: &IntermediateRepr) -> String;
}

impl ToTypeReferenceInClientDefinition for FieldType {
    fn to_type_ref(&self, ir: &IntermediateRepr) -> String {
        match self {
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    format!("Union[types.{name}, str]")
                } else {
                    format!("types.{name}")
                }
            }
            FieldType::Class(name) => format!("types.{name}"),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_ref(ir)),
            FieldType::Map(key, value) => {
                format!("Dict[{}, {}]", key.to_type_ref(ir), value.to_type_ref(ir))
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("Optional[{}]", inner.to_type_ref(ir)),
        }
    }

    fn to_partial_type_ref(&self, ir: &IntermediateRepr) -> String {
        match self {
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    format!("Optional[Union[types.{name}, str]]")
                } else {
                    format!("Optional[types.{name}]")
                }
            }
            FieldType::Class(name) => format!("partial_types.{name}"),
            FieldType::List(inner) => format!("List[{}]", inner.to_partial_type_ref(ir)),
            FieldType::Map(key, value) => {
                format!(
                    "Dict[{}, {}]",
                    key.to_type_ref(ir),
                    value.to_partial_type_ref(ir)
                )
            }
            FieldType::Primitive(r#type) => format!("Optional[{}]", r#type.to_python()),
            FieldType::Union(inner) => format!(
                "Optional[Union[{}]]",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Optional[Tuple[{}]]",
                inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => inner.to_partial_type_ref(ir),
        }
    }
}
