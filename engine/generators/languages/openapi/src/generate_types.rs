pub use class::{convert_ir_class, convert_ir_enum};
pub use function::{convert_ir_function, FunctionOpenApi};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::repr;

use crate::{
    builtin_schemas,
    r#type::{convert_ir_type, AdditionalProperties, OpenApiMeta, TypeOpenApi, TypePrimitive},
    ComponentRequestBody, Components, FunctionName, MediaTypeSchema, OpenApiSchema, Path,
    PathRequestBody, Response, TypeName,
};

pub struct OpenApiUserData {
    pub types: IndexMap<TypeName, TypeOpenApi>,
    pub functions: IndexMap<FunctionName, FunctionOpenApi>,
}

impl OpenApiUserData {
    pub fn from_ir(ir: &repr::IntermediateRepr) -> Self {
        let functions = ir
            .walk_functions()
            .map(|function| {
                (
                    FunctionName(function.name().to_string()),
                    convert_ir_function(ir, function.elem()),
                )
            })
            .collect();

        let mut types: IndexMap<TypeName, TypeOpenApi> = ir
            .walk_enums()
            .map(|r#enum| {
                (
                    TypeName(r#enum.name().to_string()),
                    convert_ir_enum(&r#enum.item.elem),
                )
            })
            .chain(ir.walk_classes().map(|class| {
                (
                    TypeName(class.name().to_string()),
                    convert_ir_class(ir, &class.item.elem),
                )
            }))
            .collect();
        types.sort_keys();
        OpenApiUserData { types, functions }
    }

    pub fn render(&self) -> OpenApiSchema {
        let mut schemas = builtin_schemas::builtin_schemas();
        schemas.extend(self.types.clone());
        let openapi_schema = OpenApiSchema {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({
                "description": "baml-cli serve",
                "version": "0.1.0",
                "title": "baml-cli serve",
            }),
            servers: serde_json::json!([{
                "url": "{address}",
                "variables": {
                    "address": {
                        "default": "http://localhost:2024"
                    }
                }
            }]),
            paths: self
                .functions
                .iter()
                .map(|(name, function)| {
                    (
                        format!("/call/{}", name.0),
                        function::render_path(name, function),
                    )
                })
                .collect(),
            components: Components {
                request_bodies: self
                    .functions
                    .iter()
                    .map(|(name, func)| {
                        let mut schema = func.return_type.clone();
                        schema.meta_mut().title = Some(format!("{}Request", name.0));
                        let mut properties: Vec<(String, bool, TypeOpenApi)> =
                            func.args.clone().into_iter().collect();
                        properties.sort_by_key(|(name, _, _)| name.clone());
                        let component_request_body =
                            ComponentRequestBody {
                                content: vec![("application/json".to_string(), MediaTypeSchema {
                                schema: TypeOpenApi::Inline {
                                    r#type: TypePrimitive::Object {
                                        properties: IndexMap::from_iter(
                                            properties
                                                .clone()
                                                .into_iter()
                                                .map(|(n, _, ty)| (n, ty))
                                                .chain(std::iter::once((
                                                    "__baml_options__".to_string(),
                                                    TypeOpenApi::Ref {
                                                        r#ref: "#/components/schemas/BamlOptions"
                                                            .to_string(),
                                                        meta: OpenApiMeta {
                                                            nullable: true,
                                                            ..OpenApiMeta::default()
                                                        },
                                                    },
                                                ))),
                                        ),
                                        required: IndexSet::from_iter(
                                            properties.iter().filter_map(
                                                |(name, is_optional, _ty)| {
                                                    if *is_optional {
                                                        None
                                                    } else {
                                                        Some(name.clone())
                                                    }
                                                },
                                            ),
                                        ), // TODO: Omit optional args?
                                        additional_properties: AdditionalProperties::Closed,
                                    },
                                    meta: OpenApiMeta {
                                        title: Some(format!("{}Request", name.0)),
                                        ..OpenApiMeta::default()
                                    }, // TODO: Correct?
                                },
                            })]
                                .into_iter()
                                .collect(),
                                required: true,
                            };
                        (name.clone(), component_request_body)
                    })
                    .collect(),
                schemas,
            },
        };
        openapi_schema
    }
}

mod class {
    use indexmap::IndexSet;

    use super::*;
    use crate::r#type::{TypeOpenApi, TypePrimitive};

    pub fn convert_ir_class(ir: &repr::IntermediateRepr, class: &repr::Class) -> TypeOpenApi {
        let mut required = IndexSet::new();
        let mut properties: IndexMap<String, TypeOpenApi> = class
            .static_fields
            .iter()
            .map(|field| {
                if field.elem.r#type.elem.is_optional() {
                    (
                        field.elem.name.clone(),
                        convert_ir_type(ir, &field.elem.r#type.elem.to_non_streaming_type(ir)),
                    )
                } else {
                    required.insert(field.elem.name.clone());
                    (
                        field.elem.name.clone(),
                        convert_ir_type(ir, &field.elem.r#type.elem.to_non_streaming_type(ir)),
                    )
                }
            })
            .collect();
        properties.sort_keys();
        required.sort();

        TypeOpenApi::Inline {
            r#type: TypePrimitive::Object {
                properties,
                required,
                additional_properties: AdditionalProperties::Closed,
            },
            meta: OpenApiMeta::default(),
        }
    }

    pub fn convert_ir_enum(r#enum: &repr::Enum) -> TypeOpenApi {
        TypeOpenApi::Inline {
            r#type: TypePrimitive::String,
            meta: OpenApiMeta {
                r#enum: Some(r#enum.values.iter().map(|v| v.0.elem.0.clone()).collect()),
                ..OpenApiMeta::default()
            },
        }
    }
}

mod function {
    use internal_baml_core::ir::repr;

    use super::*;

    pub struct FunctionOpenApi {
        pub name: String,
        pub documentation: Option<String>,
        /// (name, is_optional, type)
        pub args: Vec<(String, bool, TypeOpenApi)>,
        pub return_type: TypeOpenApi,
        // pub stream_return_type: TypeOpenApi, // TODO: Support streaming responses.
    }

    pub fn convert_ir_function(
        ir: &repr::IntermediateRepr,
        function: &repr::Function,
    ) -> FunctionOpenApi {
        let args = function
            .inputs
            .iter()
            .map(|(arg_name, arg_type)| {
                (
                    arg_name.clone(),
                    arg_type.is_optional(),
                    convert_ir_type(ir, &arg_type.to_non_streaming_type(ir)),
                )
            })
            .collect();
        FunctionOpenApi {
            name: function.name.clone(),
            documentation: None,
            args,
            return_type: convert_ir_type(ir, &function.output.to_non_streaming_type(ir)),
        }
    }

    pub fn render_path(name: &FunctionName, function: &FunctionOpenApi) -> IndexMap<String, Path> {
        let mut response_type = function.return_type.clone();
        response_type.meta_mut().title = Some(format!("{}Response", name.0));
        let path = Path {
            request_body: PathRequestBody {
                ref_: format!("#/components/requestBodies/{}", name.0),
            },

            responses: IndexMap::from_iter(vec![(
                "200",
                Response {
                    description: "Successful operation".to_string(),
                    content: IndexMap::from_iter(vec![(
                        "application/json".to_string(),
                        MediaTypeSchema {
                            schema: response_type,
                        },
                    )]),
                },
            )]),
            operation_id: function.name.clone(),
        };
        IndexMap::from_iter(vec![("post".to_string(), path)])
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::repr::make_test_ir;

    use super::*;

    #[test]
    pub fn basic_example() {
        let ir = make_test_ir(
            r##"
          class Foo {
            age int
          }

          function Go(name: string) -> Foo {
            client GPT4
            prompt #"Not important"#
          }

          client<llm> GPT4 {
            provider openai
            options {
              model gpt-4
              api_key env.OPENAI_API_KEY
            }
          }

        "##,
        )
        .expect("Valid IR");
        let file = serde_yaml::to_string(&OpenApiUserData::from_ir(&ir).render())
            .expect("Should serialize");
        eprintln!("{file}");
    }
}
