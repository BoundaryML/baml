mod generate_types;
mod python_language_features;

use std::path::PathBuf;

use anyhow::Result;
use generate_types::{to_python_literal, type_name_for_checks};
use indexmap::IndexMap;
use internal_baml_core::{
    configuration::{GeneratorDefaultClientMode, GeneratorOutputType},
    ir::{repr::IntermediateRepr, FieldType, IRHelper, IRHelperExtended},
};

use self::python_language_features::{PythonLanguageFeatures, ToPython};
use crate::{dir_writer::FileCollector, field_type_attributes};

#[derive(askama::Template)]
#[template(path = "config.py.j2", escape = "none")]
struct PythonConfig {}

#[derive(askama::Template)]
#[template(path = "async_client.py.j2", escape = "none")]
struct AsyncPythonClient {
    funcs: Vec<PythonFunction>,
}

#[derive(askama::Template)]
#[template(path = "sync_client.py.j2", escape = "none")]
struct SyncPythonClient {
    funcs: Vec<PythonFunction>,
}

struct PythonClient {
    funcs: Vec<PythonFunction>,
}

impl From<PythonClient> for AsyncPythonClient {
    fn from(value: PythonClient) -> Self {
        Self { funcs: value.funcs }
    }
}

impl From<PythonClient> for SyncPythonClient {
    fn from(value: PythonClient) -> Self {
        Self { funcs: value.funcs }
    }
}

impl From<PythonClient> for PythonLlmResponseParser {
    fn from(value: PythonClient) -> Self {
        Self { funcs: value.funcs }
    }
}

impl From<PythonClient> for PythonAsyncHttpRequest {
    fn from(value: PythonClient) -> Self {
        Self { funcs: value.funcs }
    }
}

impl From<PythonClient> for PythonSyncHttpRequest {
    fn from(value: PythonClient) -> Self {
        Self { funcs: value.funcs }
    }
}

struct PythonFunction {
    name: String,
    partial_return_type: String,
    return_type: String,
    // (name, type, default_value). When default_value is "", it will not be
    // rendered in the template.
    args: Vec<(String, String, Option<&'static str>)>,
}

#[derive(askama::Template)]
#[template(path = "__init__.py.j2", escape = "none")]
struct PythonInit {
    default_client_mode: GeneratorDefaultClientMode,
    version: String,
}

#[derive(askama::Template)]
#[template(path = "globals.py.j2", escape = "none")]
struct PythonGlobals {}

#[derive(askama::Template)]
#[template(path = "tracing.py.j2", escape = "none")]
struct PythonTracing {}

#[derive(askama::Template)]
#[template(path = "parser.py.j2", escape = "none")]
struct PythonLlmResponseParser {
    funcs: Vec<PythonFunction>,
}

#[derive(askama::Template)]
#[template(path = "async_request.py.j2", escape = "none")]
struct PythonAsyncHttpRequest {
    funcs: Vec<PythonFunction>,
}

#[derive(askama::Template)]
#[template(path = "sync_request.py.j2", escape = "none")]
struct PythonSyncHttpRequest {
    funcs: Vec<PythonFunction>,
}

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
    collector.add_template::<SyncPythonClient>("sync_client.py", (ir, generator))?;
    collector.add_template::<PythonGlobals>("globals.py", (ir, generator))?;
    collector.add_template::<PythonLlmResponseParser>("parser.py", (ir, generator))?;
    collector.add_template::<PythonAsyncHttpRequest>("async_request.py", (ir, generator))?;
    collector.add_template::<PythonSyncHttpRequest>("sync_request.py", (ir, generator))?;
    collector.add_template::<PythonTracing>("tracing.py", (ir, generator))?;
    collector.add_template::<InlinedBaml>("inlinedbaml.py", (ir, generator))?;
    collector.add_template::<PythonConfig>("config.py", (ir, generator))?;
    collector.add_template::<PythonInit>("__init__.py", (ir, generator))?;

    collector.commit(&generator.output_dir())
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonConfig {
    type Error = anyhow::Error;

    fn try_from(_: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        Ok(PythonConfig {})
    }
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
            // TODO: Should we use gen.version instead?
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonGlobals {
    type Error = anyhow::Error;

    fn try_from((_, _args): (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
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

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for AsyncPythonClient {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let python_client = PythonClient::try_from(params)?;
        Ok(python_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for SyncPythonClient {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let python_client = PythonClient::try_from(params)?;
        Ok(python_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonLlmResponseParser {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let python_client = PythonClient::try_from(params)?;
        Ok(python_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonAsyncHttpRequest {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let python_client = PythonClient::try_from(params)?;
        Ok(python_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonSyncHttpRequest {
    type Error = anyhow::Error;

    fn try_from(params: (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let python_client = PythonClient::try_from(params)?;
        Ok(python_client.into())
    }
}

impl TryFrom<(&'_ IntermediateRepr, &'_ crate::GeneratorArgs)> for PythonClient {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'_ IntermediateRepr, &'_ crate::GeneratorArgs)) -> Result<Self> {
        let functions = ir
            .walk_functions()
            .map(|f| {
                let configs = f.walk_impls();

                let funcs = configs
                    .map(|c| {
                        let (_function, _impl_) = c.item;
                        let partial_type = f.elem().output().to_partial_type_ref(ir, true);
                        Ok(PythonFunction {
                            name: f.name().to_string(),
                            partial_return_type: partial_type,
                            return_type: f.elem().output().to_type_ref(ir),
                            args: f
                                .inputs()
                                .iter()
                                .map(|(name, r#type)| {
                                    (name.to_string(), r#type.to_type_ref(ir), None)
                                })
                                .collect(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(funcs)
            })
            .collect::<Result<Vec<Vec<PythonFunction>>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(PythonClient { funcs: functions })
    }
}

trait ToTypeReferenceInClientDefinition {
    fn to_type_ref(&self, ir: &IntermediateRepr) -> String;

    /// The string representation of a field type.
    fn to_partial_type_ref(&self, ir: &IntermediateRepr, needed: bool) -> String;
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
            FieldType::Literal(value) => to_python_literal(value),
            FieldType::RecursiveTypeAlias(name) => format!("types.{name}"),
            FieldType::Class(name) => format!("types.{name}"),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_ref(ir)),
            FieldType::Map(key, value) => {
                format!(
                    "Dict[{}, {}]",
                    key.to_type_ref(ir),
                    value.to_type_ref(ir)
                )
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
            FieldType::Optional(inner) => {
                format!("Optional[{}]", inner.to_type_ref(ir))
            }
            FieldType::WithMetadata { base, .. } => match field_type_attributes(self) {
                Some(checks) => {
                    let base_type_ref = base.to_type_ref(ir);
                    let checks_type_ref = type_name_for_checks(&checks);
                    format!("Checked[{base_type_ref}, {checks_type_ref}]")
                }
                None => base.to_type_ref(ir),
            },
            FieldType::Arrow(_) => {
                todo!("Arrow types should not be used in generated type definitions")
            }
        }
    }

    fn to_partial_type_ref(&self, ir: &IntermediateRepr, needed: bool) -> String {
        let (base_type, metadata) = ir.distribute_metadata(self);
        let is_partial_type = !metadata.1.done;
        let use_module_prefix = !is_partial_type;
        let with_state = metadata.1.state;
        let constraints = metadata.0;
        let module_prefix = if is_partial_type {
            "partial_types."
        } else {
            "types."
        };

        let base_rep = match &base_type {
            FieldType::Enum(name) => {
                if ir
                    .find_enum(name)
                    .map(|e| e.item.attributes.get("dynamic_type").is_some())
                    .unwrap_or(false)
                {
                    // Note: The `false` here preserves potentially bugged codegen
                    // from before this commit. As the `false` implies, we always
                    // wrap primitives in `Optional` when generating partial types,
                    // although we should probably only do this when `!needed`.
                    if false {
                        format!("Union[types.{name}, str]")
                    } else {
                        format!("Optional[Union[types.{name}, str]]")
                    }
                } else {
                    // Note: The `false` here preserves potentially bugged codegen
                    // from before this commit. As the `false` implies, we always
                    // wrap primitives in `Optional` when generating partial types,
                    // although we should probably only do this when `!needed`.
                    if false {
                        format!("types.{name}")
                    } else {
                        format!("Optional[types.{name}]")
                    }
                }
            }
            FieldType::Class(name) => {
                if needed {
                    format!("{module_prefix}{name}")
                } else {
                    format!("Optional[{module_prefix}{name}]")
                }
            }
            FieldType::RecursiveTypeAlias(name) => {
                if needed {
                    format!("types.{name}")
                } else {
                    format!("Optional[types.{name}]")
                }
            }
            FieldType::Literal(value) => {
                // Note: The `false` here preserves potentially bugged codegen
                // from before this commit. As the `false` implies, we always
                // wrap primitives in `Optional` when generating partial types,
                // although we should probably only do this when `!needed`.
                if false {
                    to_python_literal(value)
                } else {
                    format!("Optional[{}]", to_python_literal(value))
                }
            }
            FieldType::List(inner) => {
                let inner_type = inner.to_partial_type_ref(ir, true);
                format!("List[{}]", inner_type)
            }
            FieldType::Map(key, value) => {
                let value_type = value.to_partial_type_ref(ir, true);
                format!(
                    "Dict[{}, {}]",
                    key.to_type_ref(ir),
                    value_type
                )
            }
            FieldType::Primitive(r#type) => {
                // Note: The `false` here preserves potentially bugged codegen
                // from before this commit. As the `false` implies, we always
                // wrap primitives in `Optional` when generating partial types,
                // although we should probably only do this when `!needed`.
                if false {
                    r#type.to_python()
                } else {
                    format!("Optional[{}]", r#type.to_python())
                }
            }
            FieldType::Union(inner) => {
                let union_contents = inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, true))
                    .collect::<Vec<_>>()
                    .join(", ");
                // Note: The `false` here preserves potentially bugged codegen
                // from before this commit. As the `false` implies, we always
                // wrap primitives in `Optional` when generating partial types,
                // although we should probably only do this when `!needed`.
                if false {
                    format!("Union[{union_contents}]")
                } else {
                    format!("Optional[Union[{union_contents}]]")
                }
            }
            FieldType::Tuple(inner) => {
                let tuple_contents = inner
                    .iter()
                    .map(|t| t.to_partial_type_ref(ir, false))
                    .collect::<Vec<_>>()
                    .join(", ");
                if needed {
                    format!("Tuple[{tuple_contents}]")
                } else {
                    format!("Optional[Tuple[{tuple_contents}]]")
                }
            }
            FieldType::Optional(inner) => {
                // We do special handling of primitive types here,
                // using to_type_ref, because primitives get unconditionally
                // wrapped in Optional when using partial_type_ref().
                // Using that function here would give us Optional[Optional[P]].
                // After we verify that we don't need to wrap all primitives
                // in optional, we should remove this workaround.
                if let FieldType::Primitive(_) = inner.as_ref() {
                  format!("Optional[{}]", inner.to_type_ref(ir))
                } else {
                  let inner_rep = inner.to_partial_type_ref(ir, true);
                  format!("Optional[{}]", inner_rep)
                }
            }
            FieldType::WithMetadata { base, .. } => match field_type_attributes(self) {
                Some(checks) => {
                    let base_type_ref = base.to_partial_type_ref(ir, needed);
                    let checks_type_ref = type_name_for_checks(&checks);
                    format!("Checked[{base_type_ref}, {checks_type_ref}]")
                }
                None => base.to_partial_type_ref(ir, needed),
            },
            FieldType::Arrow(_) => {
                todo!("Arrow types should not be used in generated type definitions")
            }
        };

        let rep_with_checks = match field_type_attributes(self) {
            Some(checks) => {
                let checks_type_ref = type_name_for_checks(&checks);
                format!("Checked[{}, {checks_type_ref}]", base_rep)
            }
            None => base_rep,
        };

        let rep_with_stream_state = if with_state {
            format!("StreamState[{}]", rep_with_checks)
            // (stream_state(&rep_with_checks.0), rep_with_checks.1)
        } else {
            rep_with_checks
        };
        rep_with_stream_state
    }
}

// The default value to use for parameters of this type:
// def Foo(x: Optional[int] = None, y: int[] = []):
//   ...
fn default_value_for_parameter_type(field_type: &FieldType) -> Option<&'static str> {
    match field_type {
        FieldType::Optional(_) => Some("None"),
        FieldType::List(_) => Some("[]"),
        FieldType::Map(_, _) => Some("{}"),
        FieldType::Class(_) => None,
        FieldType::RecursiveTypeAlias(_) => None,
        FieldType::Literal(_) => None,
        FieldType::Enum(_) => None,
        FieldType::Tuple(_) => None,
        FieldType::Primitive(_) => None,
        FieldType::Union(xs) => None,
        FieldType::WithMetadata { base, .. } => default_value_for_parameter_type(base),
        FieldType::Arrow(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::repr::make_test_ir;
    use baml_types::{FieldType, TypeValue};

    use crate::GeneratorArgs;

    use super::*;

    #[test]
    fn optional_str() { 
        let ir = make_test_ir("").unwrap();
        let field_type = FieldType::Optional(Box::new(FieldType::Primitive(TypeValue::String)));
        let rep = field_type.to_partial_type_ref(&ir, true);
        assert_eq!(rep, "Optional[str]")
    }

    fn mk_ir() -> IntermediateRepr {
        make_test_ir(
            r##"
class Bar {
  inner Foo? @stream.not_null @stream.with_state @check(foo, {{ true }})
}

class Foo {
  s string
}

function MakeBar() -> Bar @stream.done {
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
            Some(GeneratorOutputType::PythonPydantic),
            None,
            None,
        )
        .unwrap()
    }

    // TODO: test is flaky since it seems a dir isnt cleaned up.
    // Only meant to be uncommented and used during development.
    // #[test]
    fn generate_streaming_python() {
        let ir = mk_ir();
        let generator_args = mk_gen();
        let res = generate(&ir, &generator_args).unwrap();
        let partial_types = res.get(&PathBuf::from("partial_types.py")).unwrap();
        let async_client = res.get(&PathBuf::from("async_client.py")).unwrap();
        //eprintln!("{}", partial_types);
        eprintln!("{}", async_client);
        assert!(false);
    }
}
