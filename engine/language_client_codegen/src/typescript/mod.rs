mod generate_types;
mod typescript_language_features;

use std::path::PathBuf;

use anyhow::Result;
use either::Either;
use indexmap::IndexMap;
use internal_baml_core::{
    configuration::GeneratorDefaultClientMode,
    ir::{repr::IntermediateRepr, FieldType, IRHelper},
};

use self::typescript_language_features::{ToTypescript, TypescriptLanguageFeatures};
use crate::dir_writer::FileCollector;

#[derive(askama::Template)]
#[template(path = "async_client.ts.j2", escape = "none")]
struct AsyncTypescriptClient {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "sync_client.ts.j2", escape = "none")]
struct SyncTypescriptClient {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

struct TypescriptClient {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

impl From<TypescriptClient> for AsyncTypescriptClient {
    fn from(value: TypescriptClient) -> Self {
        Self {
            funcs: value.funcs,
            types: value.types,
        }
    }
}

impl From<TypescriptClient> for SyncTypescriptClient {
    fn from(value: TypescriptClient) -> Self {
        Self {
            funcs: value.funcs,
            types: value.types,
        }
    }
}

#[derive(Debug)]
struct TypescriptFunction {
    name: String,
    // partial_return_type: String,
    return_type: String,
    args: Vec<(String, bool, String)>,
}

#[derive(askama::Template)]
#[template(path = "index.ts.j2", escape = "none")]
struct TypescriptInit {
    default_client_mode: GeneratorDefaultClientMode,
}

#[derive(askama::Template)]
#[template(path = "globals.ts.j2", escape = "none")]
struct TypescriptGlobals {
    // In TS, we always have baml_src at ./baml_src
}

#[derive(askama::Template)]
#[template(path = "inlinedbaml.ts.j2", escape = "none")]
struct InlinedBaml {
    file_map: Vec<(String, String)>,
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
    collector.add_template::<generate_types::TypeBuilder>("type_builder.ts", (ir, generator))?;
    collector.add_template::<AsyncTypescriptClient>("async_client.ts", (ir, generator))?;
    collector.add_template::<SyncTypescriptClient>("sync_client.ts", (ir, generator))?;
    collector.add_template::<TypescriptGlobals>("globals.ts", (ir, generator))?;
    collector.add_template::<TypescriptTracing>("tracing.ts", (ir, generator))?;
    collector.add_template::<TypescriptInit>("index.ts", (ir, generator))?;
    collector.add_template::<InlinedBaml>("inlinedbaml.ts", (ir, generator))?;

    collector.commit(&generator.output_dir())
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for AsyncTypescriptClient {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for SyncTypescriptClient {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypescriptClient {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        let functions = ir
            .walk_functions()
            .map(|f| {
                let configs = f.walk_impls();

                let funcs = configs
                    .map(|c| {
                        let (_function, _impl_) = c.item;
                        Ok(TypescriptFunction {
                            name: f.name().to_string(),
                            return_type: f.elem().output().to_type_ref(ir),
                            // partial_return_type: f.elem().output().to_partial_type_ref(ir),
                            args: f
                                .inputs()
                                .iter()
                                .map(|(name, r#type)| {
                                    (
                                        name.to_string(),
                                        r#type.is_optional(),
                                        r#type.to_type_ref(ir),
                                    )
                                })
                                .collect(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<TypescriptFunction>>>>()?
            .into_iter()
            .flatten()
            .collect();

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
            file_map: args.file_map()?,
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

    fn try_from((_, gen): (&IntermediateRepr, &crate::GeneratorArgs)) -> Result<Self> {
        Ok(TypescriptInit {
            default_client_mode: gen.default_client_mode.clone(),
        })
    }
}

trait ToTypeReferenceInClientDefinition {
    fn to_type_ref(&self, ir: &IntermediateRepr) -> String;

    // fn to_partial_type_ref(&self, ir: &IntermediateRepr) -> String;
}

impl ToTypeReferenceInClientDefinition for FieldType {
    // fn to_partial_type_ref(&self, ir: &IntermediateRepr) -> String {
    //     match self {
    //         FieldType::Enum(name) => {
    //             if ir
    //                 .find_enum(name)
    //                 .map(|e| e.item.attributes.get("dynamic_type").is_some())
    //                 .unwrap_or(false)
    //             {
    //                 format!("(string | {name} | null)")
    //             } else {
    //                 format!("({name} | null)")
    //             }
    //         }
    //         FieldType::Class(name) => format!("(RecursivePartialNull<{name}>)"),
    //         FieldType::List(inner) => format!("{}[]", inner.to_partial_type_ref(ir)),
    //         FieldType::Map(key, value) => {
    //             format!(
    //                 "(Record<{}, {}> | null)",
    //                 key.to_type_ref(ir),
    //                 value.to_partial_type_ref(ir)
    //             )
    //         }
    //         FieldType::Primitive(r#type) => format!("({} | null)", r#type.to_typescript()),
    //         FieldType::Union(inner) => format!(
    //             "({} | null)",
    //             inner
    //                 .iter()
    //                 .map(|t| t.to_partial_type_ref(ir))
    //                 .collect::<Vec<_>>()
    //                 .join(" | ")
    //         ),
    //         FieldType::Tuple(inner) => format!(
    //             "([{}] | null)",
    //             inner
    //                 .iter()
    //                 .map(|t| t.to_partial_type_ref(ir))
    //                 .collect::<Vec<_>>()
    //                 .join(", ")
    //         ),
    //         FieldType::Optional(inner) => format!("({} | null)", inner.to_partial_type_ref(ir)),
    //     }
    // }

    fn to_type_ref(&self, ir: &IntermediateRepr) -> String {
        match self {
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    format!("(string | {name})")
                } else {
                    format!("{name}")
                }
            }
            FieldType::Literal(value) => value.to_string(),
            FieldType::Class(name) => format!("{name}"),
            FieldType::List(inner) => match inner.as_ref() {
                FieldType::Union(_) | FieldType::Optional(_) => {
                    format!("({})[]", inner.to_type_ref(ir))
                }
                _ => format!("{}[]", inner.to_type_ref(ir)),
            },
            FieldType::Map(key, value) => {
                format!("Record<{}, {}>", key.to_type_ref(ir), value.to_type_ref(ir))
            }
            FieldType::Primitive(r#type) => r#type.to_typescript(),
            FieldType::Union(inner) => format!(
                "{}",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            FieldType::Tuple(inner) => format!(
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("{} | null", inner.to_type_ref(ir)),
        }
    }
}
