use indexmap::{IndexMap, IndexSet};

/// A collection of schemas that are built in to BAML.
/// (Not generated from user BAML files).
///
use crate::{
    r#type::{AdditionalProperties, OpenApiMeta, TypeOpenApi, TypePrimitive},
    TypeName,
};

pub fn builtin_schemas() -> IndexMap<TypeName, TypeOpenApi> {
    IndexMap::from_iter(vec![
        (
            TypeName("BamlImage".to_string()),
            TypeOpenApi::Union {
                one_of: vec![
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("base64".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["base64".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlImageBase64".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("url".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["url".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlImageUrl".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                ],
                meta: OpenApiMeta::default(),
            },
        ),
        (
            TypeName("BamlAudio".to_string()),
            TypeOpenApi::Union {
                one_of: vec![
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("base64".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["base64".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlAudioBase64".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("url".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["url".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlAudioUrl".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                ],
                meta: OpenApiMeta::default(),
            },
        ),
        (
            TypeName("BamlPdf".to_string()),
            TypeOpenApi::Union {
                one_of: vec![
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("base64".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["base64".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlPdfBase64".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("url".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["url".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlPdfUrl".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                ],
                meta: OpenApiMeta::default(),
            },
        ),
        (
            TypeName("BamlVideo".to_string()),
            TypeOpenApi::Union {
                one_of: vec![
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("base64".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["base64".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlVideoBase64".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Object {
                            properties: IndexMap::from_iter(vec![
                                ("url".to_string(), type_string()),
                                ("media_type".to_string(), type_string()),
                            ]),
                            required: IndexSet::from_iter(vec!["url".to_string()]),
                            additional_properties: AdditionalProperties::Closed,
                        },
                        meta: OpenApiMeta {
                            title: Some("BamlVideoUrl".to_string()),
                            ..OpenApiMeta::default()
                        },
                    },
                ],
                meta: OpenApiMeta::default(),
            },
        ),
        (
            TypeName("BamlOptions".to_string()),
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Object {
                    properties: IndexMap::from_iter(vec![(
                        "client_registry".to_string(),
                        TypeOpenApi::Inline {
                            r#type: TypePrimitive::Object {
                                properties: IndexMap::from_iter(vec![
                                    (
                                        "clients".to_string(),
                                        TypeOpenApi::Inline {
                                            r#type: TypePrimitive::Array {
                                                items: Box::new(TypeOpenApi::Ref {
                                                    r#ref: "#/components/schemas/ClientProperty"
                                                        .to_string(),
                                                    meta: OpenApiMeta::default(),
                                                }),
                                            },
                                            meta: OpenApiMeta::default(),
                                        },
                                    ),
                                    ("primary".to_string(), type_string()),
                                ]),
                                required: IndexSet::from_iter(vec!["clients".to_string()]),
                                additional_properties: AdditionalProperties::Closed,
                            },
                            meta: OpenApiMeta::default(),
                        },
                    )]),
                    required: IndexSet::from_iter(vec!["client_registry".to_string()]),
                    additional_properties: AdditionalProperties::Closed,
                },
                meta: OpenApiMeta {
                    nullable: false,
                    ..OpenApiMeta::default()
                },
            },
        ),
        (
            TypeName("ClientProperty".to_string()),
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Object {
                    properties: IndexMap::from_iter(vec![
                        ("name".to_string(), type_string()),
                        ("provider".to_string(), type_string()),
                        (
                            "options".to_string(),
                            TypeOpenApi::Inline {
                                r#type: TypePrimitive::Object {
                                    properties: IndexMap::new(),
                                    required: IndexSet::new(),
                                    additional_properties: AdditionalProperties::Open,
                                },
                                meta: OpenApiMeta::default(),
                            },
                        ),
                    ]),
                    required: IndexSet::from_iter(vec![
                        "name".to_string(),
                        "provider".to_string(),
                        "options".to_string(),
                    ]),
                    additional_properties: AdditionalProperties::Closed,
                },
                meta: OpenApiMeta::default(),
            },
        ),
        (
            TypeName("Check".to_string()),
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Object {
                    properties: IndexMap::from_iter(vec![
                        ("name".to_string(), type_string()),
                        ("expr".to_string(), type_string()),
                        ("status".to_string(), type_string()),
                    ]),
                    required: IndexSet::from_iter(vec!["name".to_string(), "expr".to_string()]),
                    additional_properties: AdditionalProperties::Closed,
                },
                meta: OpenApiMeta::default(),
            },
        ),
    ])
}

/// Helper function to create a TypeOpenApi String with default meta.
fn type_string() -> TypeOpenApi {
    TypeOpenApi::Inline {
        r#type: TypePrimitive::String,
        meta: OpenApiMeta::default(),
    }
}
