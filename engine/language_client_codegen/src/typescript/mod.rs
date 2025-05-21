mod generate_types;
mod typescript_language_features;

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use anyhow::Result;
use baml_types::LiteralValue;
use generate_types::{render_docstring, type_name_for_checks};
use indexmap::IndexMap;
use internal_baml_core::{
    configuration::{GeneratorDefaultClientMode, GeneratorOutputType, ModuleFormat},
    ir::{repr::IntermediateRepr, FieldType, IRHelper, IRHelperExtended},
};
use regex::Regex;

use self::typescript_language_features::{ToTypescript, TypescriptLanguageFeatures};
use crate::{dir_writer::FileCollector, field_type_attributes};

mod framework {
    use internal_baml_core::configuration::GeneratorOutputType;

    #[derive(Debug, Clone, Copy)]
    pub enum TypescriptFramework {
        None,
        React,
    }

    impl TypescriptFramework {
        pub fn from_generator_type(output_type: GeneratorOutputType) -> Option<Self> {
            match output_type {
                GeneratorOutputType::TypescriptReact => Some(Self::React),
                GeneratorOutputType::Typescript => Some(Self::None),
                GeneratorOutputType::OpenApi => None,
                GeneratorOutputType::PythonPydantic | GeneratorOutputType::PythonPydanticV1 => None,
                GeneratorOutputType::RubySorbet => None,
                GeneratorOutputType::Go => None,
            }
        }
    }
}

use framework::TypescriptFramework;

mod filters {
    pub fn length<T>(v: &Vec<T>) -> Result<usize, askama::Error> {
        Ok(v.len())
    }
}

#[derive(askama::Template)]
#[template(path = "react/server.ts.j2", escape = "none")]
struct ReactServerActions {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "react/server_streaming.ts.j2", escape = "none")]
struct ReactServerStreaming {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "react/server_streaming_types.ts.j2", escape = "none")]
struct ReactServerStreamingTypes {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
    partial_return_types: IndexMap<String, String>,
}

#[derive(askama::Template)]
#[template(path = "react/hooks.tsx.j2", escape = "none")]
struct ReactClientHooks {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "react/media.ts.j2", escape = "none")]
struct ReactClientMedia {}

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
    partial_return_types: IndexMap<String, String>,
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

impl From<TypescriptClient> for ReactServerActions {
    fn from(value: TypescriptClient) -> Self {
        Self {
            funcs: value.funcs,
            types: value.types,
        }
    }
}

impl From<TypescriptClient> for ReactServerStreaming {
    fn from(value: TypescriptClient) -> Self {
        Self {
            funcs: value.funcs,
            types: value.types,
        }
    }
}

impl From<TypescriptClient> for ReactServerStreamingTypes {
    fn from(value: TypescriptClient) -> Self {
        Self {
            funcs: value.funcs,
            types: value.types,
            partial_return_types: value.partial_return_types,
        }
    }
}

impl From<TypescriptClient> for ReactClientHooks {
    fn from(value: TypescriptClient) -> Self {
        Self {
            funcs: value.funcs,
            types: value.types,
        }
    }
}

impl From<TypescriptClient> for ReactClientMedia {
    fn from(_: TypescriptClient) -> Self {
        Self {}
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
    version: String,
}

#[derive(askama::Template)]
#[template(path = "globals.ts.j2", escape = "none")]
struct TypescriptGlobals {
    // In TS, we always have baml_src at ./baml_src
}

#[derive(askama::Template)]
#[template(path = "config.ts.j2", escape = "none")]
struct TypescriptConfig {}

#[derive(askama::Template)]
#[template(path = "inlinedbaml.ts.j2", escape = "none")]
struct InlinedBaml {
    file_map: Vec<(String, String)>,
}

#[derive(askama::Template)]
#[template(path = "tracing.ts.j2", escape = "none")]
struct TypescriptTracing {}

#[derive(askama::Template)]
#[template(path = "parser.ts.j2", escape = "none")]
struct TypscriptLlmParser {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

impl From<TypescriptClient> for TypscriptLlmParser {
    fn from(ts_client: TypescriptClient) -> Self {
        Self {
            funcs: ts_client.funcs,
            types: ts_client.types,
        }
    }
}

#[derive(askama::Template)]
#[template(path = "async_request.ts.j2", escape = "none")]
struct AsyncTypescriptRequest {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

impl From<TypescriptClient> for AsyncTypescriptRequest {
    fn from(ts_client: TypescriptClient) -> Self {
        Self {
            funcs: ts_client.funcs,
            types: ts_client.types,
        }
    }
}

#[derive(askama::Template)]
#[template(path = "sync_request.ts.j2", escape = "none")]
struct SyncTypescriptRequest {
    funcs: Vec<TypescriptFunction>,
    types: Vec<String>,
}

impl From<TypescriptClient> for SyncTypescriptRequest {
    fn from(ts_client: TypescriptClient) -> Self {
        Self {
            funcs: ts_client.funcs,
            types: ts_client.types,
        }
    }
}

pub(crate) fn generate(
    ir: &IntermediateRepr,
    generator: &crate::GeneratorArgs,
) -> Result<IndexMap<PathBuf, String>> {
    let framework = TypescriptFramework::from_generator_type(generator.client_type)
        .expect("Invalid generator type for TypeScript framework");
    let mut collector = FileCollector::<TypescriptLanguageFeatures>::new();

    // Add base TypeScript files
    collector.add_template::<generate_types::TypescriptTypes>("types.ts", (ir, generator))?;
    collector.add_template::<generate_types::TypescriptStreamTypes>(
        "partial_types.ts",
        (ir, generator),
    )?;
    collector.add_template::<generate_types::TypeBuilder>("type_builder.ts", (ir, generator))?;
    collector.add_template::<AsyncTypescriptClient>("async_client.ts", (ir, generator))?;
    collector.add_template::<SyncTypescriptClient>("sync_client.ts", (ir, generator))?;
    collector.add_template::<TypescriptGlobals>("globals.ts", (ir, generator))?;
    collector.add_template::<TypescriptConfig>("config.ts", (ir, generator))?;
    collector.add_template::<TypscriptLlmParser>("parser.ts", (ir, generator))?;
    collector.add_template::<AsyncTypescriptRequest>("async_request.ts", (ir, generator))?;
    collector.add_template::<SyncTypescriptRequest>("sync_request.ts", (ir, generator))?;
    collector.add_template::<TypescriptTracing>("tracing.ts", (ir, generator))?;
    collector.add_template::<TypescriptInit>("index.ts", (ir, generator))?;
    collector.add_template::<InlinedBaml>("inlinedbaml.ts", (ir, generator))?;

    // Add framework-specific files
    match framework {
        TypescriptFramework::React => {
            collector.add_template::<ReactServerActions>("react/server.ts", (ir, generator))?;
            collector.add_template::<ReactServerStreaming>(
                "react/server_streaming.ts",
                (ir, generator),
            )?;
            collector.add_template::<ReactServerStreamingTypes>(
                "react/server_streaming_types.ts",
                (ir, generator),
            )?;
            collector.add_template::<ReactClientHooks>("react/hooks.tsx", (ir, generator))?;
            collector.add_template::<ReactClientMedia>("react/media.ts", (ir, generator))?;
        }
        TypescriptFramework::None => {}
    }

    // if it's typescriopt and generator.esm is enabled, we need to change the imports in each file to use the .js extension
    if generator.module_format == Some(ModuleFormat::Esm) {
        collector.modify_files(|content: &mut String| {
            *content = add_js_suffix_to_imports(content);
        });
    }

    collector.commit(&generator.output_dir())
}

fn add_js_suffix_to_imports(content: &str) -> String {
    // Regex to find import/export statements with module specifiers.
    // It captures the import/export part, quotes, and the path itself.
    // Escaped curly braces in the character set just in case.
    let re = Regex::new(r#"(import(?:["'\s]*(?:[\w\*\{\}\n\r\t, ]+)from\s*)?|export(?:["'\s]*(?:[\w\*\{\}\n\r\t, ]+)from\s*)?)(["'])([^"']+)(["'])"#).unwrap();

    re.replace_all(content, |caps: &regex::Captures| {
        let import_export_part = &caps[1];
        let quote = &caps[2];
        let path = &caps[3];
        let closing_quote = &caps[4];

        // Check if it's a relative path (starts with ./ or ../)
        if path.starts_with("./") || path.starts_with("../") {
            // Check if it already has a common JS/TS/CSS extension
            if !path.ends_with(".js") &&
               !path.ends_with(".mjs") &&
               !path.ends_with(".cjs") &&
               !path.ends_with(".jsx") && // Consider react specific extensions too
               !path.ends_with(".tsx") &&
               !path.ends_with(".css") && // Ignore CSS files
               !path.ends_with(".json")
            {
                // Remove existing .ts if present before adding .js
                let base_path = if path.ends_with(".ts") {
                    &path[..path.len() - 3]
                } else {
                    path
                };
                // Append .js
                format!("{import_export_part}{quote}{base_path}.js{closing_quote}")
            } else {
                // Already has a recognized extension, leave it as is.
                caps[0].to_string()
            }
        } else {
            // Not a relative path (e.g., external package like 'react' or '@boundaryml/baml'), leave it as is.
            caps[0].to_string()
        }
    })
    .to_string()
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypescriptConfig {
    type Error = anyhow::Error;

    fn try_from(_: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(TypescriptConfig {})
    }
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
        let mut functions: Vec<TypescriptFunction> = ir
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

        // Sort functions by name
        functions.sort_by(|a, b| a.name.cmp(&b.name));

        // Collect and sort all types including recursive aliases
        let mut types: Vec<String> = ir
            .walk_classes()
            .map(|c| c.name().to_string())
            .chain(ir.walk_enums().map(|e| e.name().to_string()))
            .chain(ir.walk_alias_cycles().map(|a| a.item.0.clone()))
            .collect();
        types.sort();

        let mut partial_return_types: IndexMap<String, String> = functions
            .iter()
            .map(|f| (f.name.clone(), f.partial_return_type.clone()))
            .collect();
        partial_return_types.sort_keys();

        Ok(TypescriptClient {
            funcs: functions,
            partial_return_types,
            types,
        })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for TypscriptLlmParser {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for AsyncTypescriptRequest {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for SyncTypescriptRequest {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
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
            // TODO: Should we use gen.version
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for ReactServerActions {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for ReactServerStreaming {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for ReactServerStreamingTypes {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for ReactClientHooks {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for ReactClientMedia {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let typscript_client = TypescriptClient::try_from(params)?;
        Ok(typscript_client.into())
    }
}

trait ToTypeReferenceInClientDefinition {
    fn to_type_ref(&self, ir: &IntermediateRepr, use_module_prefix: bool) -> String;
    /// The string representation of a field type, and whether the field is optional.
    fn to_partial_type_ref(&self, ir: &IntermediateRepr, needed: bool) -> (String, bool);
}

impl ToTypeReferenceInClientDefinition for FieldType {
    /// How to serialize a type for use in a function's type signature.
    /// Also returns whether the field is optional during streaming.
    fn to_partial_type_ref(&self, ir: &IntermediateRepr, needed: bool) -> (String, bool) {
        let (base_type, metadata) = ir.distribute_metadata(self);
        let is_partial_type = !metadata.1.done;
        let use_module_prefix = !is_partial_type;
        let with_state = metadata.1.state;
        let constraints = metadata.0;
        let module_prefix = if use_module_prefix {
            "types."
        } else {
            "partial_types."
        };
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
            FieldType::Literal(value) => (value.to_string(), false),
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
            FieldType::Arrow(_) => {
                todo!("Arrow types should not be used in generated type definitions")
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
            FieldType::Arrow(_) => {
                todo!("Arrow types should not be used in generated type definitions")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeneratorArgs;
    use internal_baml_core::ir::repr::make_test_ir;

    #[test]
    fn test_replace_ts_imports_with_js() {
        // Add .js to relative paths without extension
        assert_eq!(
            add_js_suffix_to_imports("import { Foo } from './bar';"),
            "import { Foo } from './bar.js';"
        );
        assert_eq!(
            add_js_suffix_to_imports("export * from \"../baz/qux\";"),
            "export * from \"../baz/qux.js\";"
        );
        assert_eq!(
            add_js_suffix_to_imports("import type { Bar } from './bar'"),
            "import type { Bar } from './bar.js'"
        );
        assert_eq!(
            add_js_suffix_to_imports("import {\n  Thing1,\n  Thing2\n} from \"./things\";"),
            "import {\n  Thing1,\n  Thing2\n} from \"./things.js\";"
        );

        // Replace .ts with .js in relative paths
        assert_eq!(
            add_js_suffix_to_imports("import { Foo } from './bar.ts';"),
            "import { Foo } from './bar.js';"
        );
        assert_eq!(
            add_js_suffix_to_imports("export * from \"../baz/qux.ts\";"),
            "export * from \"../baz/qux.js\";"
        );

        // Should ignore already correct .js paths
        assert_eq!(
            add_js_suffix_to_imports("import { Foo } from './bar.js';"),
            "import { Foo } from './bar.js';"
        );
        // Should ignore other extensions like .css, .mjs, .cjs
        assert_eq!(
            add_js_suffix_to_imports("import styles from './styles.css';"),
            "import styles from './styles.css';"
        );
        assert_eq!(
            add_js_suffix_to_imports("import config from './config.json';"),
            "import config from './config.json';"
        );
        assert_eq!(
            add_js_suffix_to_imports("import { util } from './util.mjs';"),
            "import { util } from './util.mjs';"
        );
        assert_eq!(
            add_js_suffix_to_imports("import { main } from '../main.cjs';"),
            "import { main } from '../main.cjs';"
        );
        assert_eq!(
            add_js_suffix_to_imports("import { Comp } from './Comp.tsx';"),
            "import { Comp } from './Comp.tsx';"
        );
        assert_eq!(
            add_js_suffix_to_imports("import { Button } from './Button.jsx';"),
            "import { Button } from './Button.jsx';"
        );

        // Should ignore absolute paths or URLs
        assert_eq!(
            add_js_suffix_to_imports("import React from 'react';"),
            "import React from 'react';"
        );
        assert_eq!(
            add_js_suffix_to_imports("import { BamlClient } from '@boundaryml/baml';"),
            "import { BamlClient } from '@boundaryml/baml';"
        );
        assert_eq!(
            add_js_suffix_to_imports("const path = '/path/to/file.ts';"),
            "const path = '/path/to/file.ts';" // This is not an import/export statement
        );

        // Empty string
        assert_eq!(add_js_suffix_to_imports(""), "");
        // String with no imports
        assert_eq!(
            add_js_suffix_to_imports("const x = 10; function y() {}"),
            "const x = 10; function y() {}"
        );
        // Mixed content
        assert_eq!(
            add_js_suffix_to_imports(
                "console.log('hello');\nimport { a } from './a.ts';\nimport { b } from './b';\nimport { c } from './c.js';\nimport { d } from 'd-lib';\nexport { e } from '../e.ts';\nconsole.log('world');"
            ),
            "console.log('hello');\nimport { a } from './a.js';\nimport { b } from './b.js';\nimport { c } from './c.js';\nimport { d } from 'd-lib';\nexport { e } from '../e.js';\nconsole.log('world');"
        );
    }

    fn mk_ir() -> IntermediateRepr {
        make_test_ir(
            r##"
class Bar {
  inner Foo? @stream.not_null @stream.with_state @check(foo, {{ true }})
  @@stream.done
}

class Foo {
  s string
}

function MakeBar() -> Bar {
  client GPT35
  prompt #"
    {{ ctx.output_format }}
  "#
}

client<llm> GPT35 {
  provider openai
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
} 

// class Foo {
//   i int @stream.not_null @stream.with_state
//   b Bar @stream.done
// }

// class Foo {
//   str string @stream.with_state
// }
//
// class Inner {
//   inner_int int
//   inner_string string @stream.not_null
//   inner_string_2 string @stream.not_null @stream.done
// }
//
// class InnerDone {
//   inner_done_inner Inner @stream.done
//   inner_done_int int
//   inner_done_str string
//   @@stream.done
// }
        "##,
        )
        .unwrap()
    }

    fn mk_gen() -> GeneratorArgs {
        GeneratorArgs::new(
            "baml_client",
            "baml_src",
            vec![],
            "no_version".to_string(),
            true,
            GeneratorDefaultClientMode::Async,
            Vec::new(),
            GeneratorOutputType::Typescript,
            None,
            None,
        )
        .unwrap()
    }

    // Test is flaky since it seems a dir isnt cleaned up.
    // Only meant to be uncommented and used during development.
    // #[test]
    fn generate_streaming_typescript() {
        let ir = mk_ir();
        let generator_args = mk_gen();
        let res = generate(&ir, &generator_args).unwrap();
        // let partial_types = res.get(&PathBuf::from("partial_types.ts")).unwrap();
        let async_client = res.get(&PathBuf::from("async_client.ts")).unwrap();
        //eprintln!("{}", partial_types);
        eprintln!("{}", async_client);
        assert!(false);
    }
}
