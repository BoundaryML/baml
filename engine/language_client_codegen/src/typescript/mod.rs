mod generate_types;
mod typescript_language_features;

use std::path::PathBuf;

use anyhow::Result;
use baml_types::LiteralValue;
use generate_types::{render_docstring, type_name_for_checks};
use indexmap::IndexMap;
use internal_baml_core::{
    configuration::GeneratorDefaultClientMode,
    ir::{repr::IntermediateRepr, FieldType, IRHelper},
};

use self::typescript_language_features::{ToTypescript, TypescriptLanguageFeatures};
use crate::{dir_writer::FileCollector, field_type_attributes};

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
    return_type: String,
    partial_return_type: String,
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
    collector.add_template::<generate_types::TypescriptStreamTypes>(
        "partial_types.ts",
        (ir, generator),
    )?;
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
                            return_type: f.elem().output().to_type_ref(ir, false),
                            partial_return_type: f.elem().output().to_partial_type_ref(ir, true).0,
                            args: f
                                .inputs()
                                .iter()
                                .map(|(name, r#type)| {
                                    (
                                        name.to_string(),
                                        r#type.is_optional(),
                                        r#type.to_type_ref(ir, false),
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
    fn to_type_ref(&self, ir: &IntermediateRepr, use_module_prefix: bool) -> String;
    /// The string representation of a field type, and whether the field is optional.
    fn to_partial_type_ref(&self, ir: &IntermediateRepr, needed: bool) -> (String, bool);
}

impl ToTypeReferenceInClientDefinition for FieldType {
    /// How to serialize a type for use in a function's type signature.
    fn to_partial_type_ref(&self, ir: &IntermediateRepr, needed: bool) -> (String, bool) {
        let (base_type, metadata) = ir.distribute_metadata(self);
        let is_partial_type = !metadata.1.done;
        let use_module_prefix = !is_partial_type;
        let with_state = metadata.1.state;
        let constraints = metadata.0;
        let module_prefix = if use_module_prefix { "types." } else { "partial_types." };
        let (base_rep, optional) = match base_type {
            FieldType::Class(name) => {
                if needed {
                    (format!("{module_prefix}{name}"), false)
                } else {
                    (format!("({module_prefix}{name} | null)"), true)
                }
            }
            FieldType::RecursiveTypeAlias(name) => (name.to_owned(), !needed),
            FieldType::Enum(name) => {
                let res = if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    if needed {
                        (format!("(string | {name})"), false)
                    } else {
                        (format!("(string | {name} | null)"), true)
                    }
                } else {
                    if needed {
                        (format!("types.{name}"), false)
                    } else {
                        (format!("({name} | null)"), true)
                    }
                };
                res
            }
            FieldType::Literal(value) => {
                (value.to_string(), false)
            }
            FieldType::List(inner) => (
                format!("{}[]", inner.to_partial_type_ref(ir, false).0),
                true,
            ),
            FieldType::Map(key, value) => {
                let or_null = if needed { "" } else { "| null" };
                (
                    format!(
                        "(Record<{}, {}> {or_null})",
                        key.to_type_ref(ir, false),
                        value.to_partial_type_ref(ir, false).0
                    ),
                    !needed,
                )
            }
            FieldType::Primitive(r#type) => {
                if needed {
                    (r#type.to_typescript(), false)
                } else {
                    (format!("({} | null)", r#type.to_typescript()), true)
                }
            }
            FieldType::Union(inner) => {
                let union_contents = inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, false).0)
                    .collect::<Vec<_>>()
                    .join(" | ");
                if needed {
                    (format!("({})", union_contents), false)
                } else {
                    (format!("({} | null)", union_contents), true)
                }
            }
            FieldType::Tuple(inner) => {
                let tuple_contents = inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, false).0)
                    .collect::<Vec<_>>()
                    .join(", ");
                if needed {
                    (format!("[{tuple_contents}]"), false)
                } else {
                    (format!("([{tuple_contents}] | null)"), true)
                }
            }
            FieldType::Optional(inner) => (
                format!("({} | null)", inner.to_partial_type_ref(ir, false).0),
                false,
            ),
            FieldType::WithMetadata { .. } => {
                unreachable!("distribute_metadata makes this field unreachable.")
            }
        };
        let base_type_ref = if is_partial_type {
            base_rep
        } else {
            if needed {
                base_type.to_type_ref(ir, use_module_prefix)
            } else {
                base_rep
            }
        };
        let rep_with_checks = match field_type_attributes(self) {
            Some(checks) => {
                let checks_type_ref = type_name_for_checks(&checks);
                format!("Checked<{base_type_ref},{checks_type_ref}>")
            }
            None => base_type_ref,
        };
        let rep_with_stream_state = if with_state {
            format!("StreamState<{rep_with_checks}>")
        } else {
            rep_with_checks
        };
        (rep_with_stream_state, optional)
    }

    fn to_type_ref(&self, ir: &IntermediateRepr, use_module_prefix: bool) -> String {
        let module_prefix = if use_module_prefix { "types." } else { "" };
        match self {
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    format!("(string | {module_prefix}{name})")
                } else {
                    format!("{module_prefix}{name}")
                }
            }
            FieldType::RecursiveTypeAlias(name) => name.to_owned(),
            FieldType::Class(name) => format!("{module_prefix}{name}"),
            FieldType::List(inner) => match inner.as_ref() {
                FieldType::Union(_) | FieldType::Optional(_) => {
                    format!("({})[]", inner.to_type_ref(ir, use_module_prefix))
                }
                _ => format!("{}[]", inner.to_type_ref(ir, use_module_prefix)),
            },
            FieldType::Map(key, value) => {
                let k = key.to_type_ref(ir, true);
                let v = value.to_type_ref(ir, use_module_prefix);

                match key.as_ref() {
                    FieldType::Enum(_)
                    | FieldType::Union(_)
                    | FieldType::Literal(LiteralValue::String(_)) => {
                        format!("Partial<Record<{k}, {v}>>")
                    }
                    _ => format!("Record<{k}, {v}>"),
                }
            }
            FieldType::Primitive(r#type) => r#type.to_typescript(),
            // In typescript we can just use literal values as type defs.
            FieldType::Literal(value) => value.to_string(),
            FieldType::Union(inner) => inner
                .iter()
                .map(|t| t.to_type_ref(ir, use_module_prefix))
                .collect::<Vec<_>>()
                .join(" | ")
                .to_string(),
            FieldType::Tuple(inner) => format!(
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_ref(ir, use_module_prefix))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => {
                format!("{} | null", inner.to_type_ref(ir, use_module_prefix))
            }
            FieldType::WithMetadata { base, .. } => match field_type_attributes(self) {
                Some(checks) => {
                    let base_type_ref = base.to_type_ref(ir, use_module_prefix);
                    let checks_type_ref = type_name_for_checks(&checks);
                    format!("Checked<{base_type_ref},{checks_type_ref}>")
                }
                None => base.to_type_ref(ir, use_module_prefix),
            },
        }
    }
}
