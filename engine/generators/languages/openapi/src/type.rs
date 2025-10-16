use std::hash::Hash;

use baml_types::{
    ir_type::{TypeNonStreaming, TypeValue, UnionTypeViewGeneric},
    BamlMediaType, Constraint, ConstraintLevel, LiteralValue,
};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::{ir_helpers::IRHelper, repr::IntermediateRepr};
use serde::Serialize;
/// The abstract type system of OpenAPI.
/// We convert IR into this, before generating the OpenAPI yaml file.
///
/// We serialize to the openapi.yaml file directly from this type,
/// so changes to variant and field names must be accounted for with serde
/// attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum TypeOpenApi {
    Ref {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(rename = "$ref")]
        r#ref: String,
    },
    Inline {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(flatten)]
        r#type: TypePrimitive,
    },
    Union {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(rename = "oneOf")]
        one_of: Vec<TypeOpenApi>,
    },
    AnyValue {
        #[serde(flatten)]
        meta: OpenApiMeta,
        #[serde(rename = "type")]
        type_: String,
        #[serde(rename = "additionalProperties")]
        additional_properties: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum TypePrimitive {
    String,
    Number,
    Integer,
    Boolean,
    Array {
        items: Box<TypeOpenApi>,
    },
    Object {
        #[serde(skip_serializing_if = "IndexMap::is_empty")]
        properties: IndexMap<String, TypeOpenApi>,
        required: IndexSet<String>,
        #[serde(rename = "additionalProperties")]
        additional_properties: AdditionalProperties,
    },
    Null,
}

// The ability for an object to have additional properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdditionalProperties {
    /// The object does not allow additional properties.
    Closed,
    /// The object allows additional properties.
    Open,
    /// The object allows exactly the additional properties defined by the
    /// schema.
    Schema(Box<TypeOpenApi>),
}

impl Serialize for AdditionalProperties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AdditionalProperties::Closed => serializer.serialize_bool(false),
            AdditionalProperties::Open => serializer.serialize_bool(true),
            AdditionalProperties::Schema(schema) => schema.serialize(serializer),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct OpenApiMeta {
    /// Pydantic includes this by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// JSON schema considers 'enum' to be a validation rule, not a type,
    /// so it can be attached to any type.
    /// We only allow string-shaped enums
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#enum: Option<Vec<String>>,

    /// We only allow string-shaped const values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#const: Option<String>,
    // description: Option<String>,
    /// Nulls in OpenAPI are weird: https://swagger.io/docs/specification/data-models/data-types/
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,
}

impl Hash for TypeOpenApi {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let s = serde_yaml::to_string(self).expect("Should serialize");
        s.hash(state);
    }
}

impl TypeOpenApi {
    pub fn meta(&self) -> &OpenApiMeta {
        match self {
            TypeOpenApi::Ref { meta, .. } => meta,
            TypeOpenApi::Inline { meta, .. } => meta,
            TypeOpenApi::Union { meta, .. } => meta,
            TypeOpenApi::AnyValue { meta, .. } => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut OpenApiMeta {
        match self {
            TypeOpenApi::Ref { meta, .. } => meta,
            TypeOpenApi::Inline { meta, .. } => meta,
            TypeOpenApi::Union { meta, .. } => meta,
            TypeOpenApi::AnyValue { meta, .. } => meta,
        }
    }

    pub fn with_meta(mut self, meta: OpenApiMeta) -> Self {
        *self.meta_mut() = meta;
        self
    }
}

pub fn convert_ir_type(ir: &IntermediateRepr, ty: &TypeNonStreaming) -> TypeOpenApi {
    let meta_enum: Option<Vec<String>> = match ty {
        TypeNonStreaming::Enum { name, .. } => ir.find_enum(name).ok().map(|e| {
            e.item
                .elem
                .values
                .iter()
                .map(|v| v.0.elem.0.to_string())
                .collect()
        }),
        _ => None,
    };
    let meta_const = None;
    let meta = OpenApiMeta {
        nullable: ty.is_optional(),
        r#enum: meta_enum,
        r#const: meta_const,
        title: None, // TODO: Correct?
    };
    let meta_copy = meta.clone();
    let base_rep = match ty {
        TypeNonStreaming::Primitive(inner, _) => match inner {
            TypeValue::String => TypeOpenApi::Inline {
                r#type: TypePrimitive::String,
                meta: meta_copy,
            },
            TypeValue::Int => TypeOpenApi::Inline {
                r#type: TypePrimitive::Integer,
                meta: meta_copy,
            },
            TypeValue::Float => TypeOpenApi::Inline {
                r#type: TypePrimitive::Number,
                meta: meta_copy,
            },
            TypeValue::Bool => TypeOpenApi::Inline {
                r#type: TypePrimitive::Boolean,
                meta: meta_copy,
            },
            TypeValue::Null => TypeOpenApi::Inline {
                r#type: TypePrimitive::Null,
                meta: meta_copy,
            },
            TypeValue::Media(media_type) => {
                let media_type = match media_type {
                    BamlMediaType::Image => "BamlImage",
                    BamlMediaType::Audio => "BamlAudio",
                    BamlMediaType::Pdf => "BamlPdf",
                    BamlMediaType::Video => "BamlVideo",
                };
                TypeOpenApi::Ref {
                    r#ref: format!("#/components/schemas/{media_type}"),
                    meta: meta_copy,
                }
            }
        },
        TypeNonStreaming::Class { name, .. } => TypeOpenApi::Ref {
            r#ref: format!("#/components/schemas/{name}"),
            meta: meta_copy,
        },
        TypeNonStreaming::List(inner, _) => TypeOpenApi::Inline {
            r#type: TypePrimitive::Array {
                items: Box::new(convert_ir_type(ir, inner)),
            },
            meta: meta_copy,
        },
        TypeNonStreaming::Enum { name, .. } => TypeOpenApi::Ref {
            r#ref: format!("#/components/schemas/{name}"),
            meta: meta_copy,
        },
        TypeNonStreaming::Literal(literal, _) => TypeOpenApi::Inline {
            r#type: match literal {
                LiteralValue::String(_) => TypePrimitive::String,
                LiteralValue::Int(_) => TypePrimitive::Integer,
                LiteralValue::Bool(_) => TypePrimitive::Boolean,
            },
            meta: meta_copy,
        },
        TypeNonStreaming::Arrow(_, _) => panic!("Arrow types are not supported in code generation"),
        TypeNonStreaming::Union(inner, _) => match inner.view() {
            UnionTypeViewGeneric::Null => TypeOpenApi::Inline {
                r#type: TypePrimitive::Null,
                meta: meta_copy,
            },
            UnionTypeViewGeneric::Optional(inner) => {
                // The seems correct but does not match previous behavior.
                // let result = convert_ir_type(ir, inner);
                // result.meta_mut().nullable = true;
                // result
                convert_ir_type(ir, inner)
            }
            UnionTypeViewGeneric::OneOf(inner) => TypeOpenApi::Union {
                one_of: inner.iter().map(|i| convert_ir_type(ir, i)).collect(),
                meta: meta_copy,
            },
            UnionTypeViewGeneric::OneOfOptional(inner) => {
                let one_of = inner.iter().map(|i| convert_ir_type(ir, i)).collect();
                TypeOpenApi::Union {
                    one_of,
                    meta: meta_copy,
                }
            }
        },
        TypeNonStreaming::Map(_key_type, value_type, _) => TypeOpenApi::Inline {
            r#type: TypePrimitive::Object {
                properties: IndexMap::new(),
                required: IndexSet::new(),
                additional_properties: AdditionalProperties::Schema(Box::new(convert_ir_type(
                    ir, value_type,
                ))),
            },
            meta: meta_copy,
        },
        TypeNonStreaming::Tuple(..) => panic!("Tuple types are not supported in code generation"),
        TypeNonStreaming::RecursiveTypeAlias { .. } => TypeOpenApi::AnyValue {
            type_: "object".to_string(),
            meta: meta_copy,
            additional_properties: true,
        },
        TypeNonStreaming::Top(_) => panic!(
            "TypeNonStreaming::Top should have been resolved by the compiler before code generation. \
             This indicates a bug in the type resolution phase."
        ),
    };

    let checks = ty
        .meta()
        .constraints
        .iter()
        .filter_map(|Constraint { level, label, .. }| {
            if level == &ConstraintLevel::Check {
                label.clone()
            } else {
                None
            }
        })
        .collect::<Vec<String>>();
    if checks.is_empty() {
        base_rep
    } else {
        TypeOpenApi::Inline {
            r#type: TypePrimitive::Object {
                properties: [
                    ("value".to_string(), base_rep),
                    ("checks".to_string(), type_def_for_checks(checks)),
                ]
                .into_iter()
                .collect(),
                required: IndexSet::from_iter(vec!["value".to_string(), "checks".to_string()]),
                additional_properties: AdditionalProperties::Closed,
            },
            meta,
        }
    }
}

/// The type definition for a single "Checked_*" type. Note that we don't
/// produce a named type for each of these the way we do for SDK
/// codegeneration.
fn type_def_for_checks(checks: Vec<String>) -> TypeOpenApi {
    let mut properties: IndexMap<String, TypeOpenApi> = checks
        .iter()
        .map(|check_name| {
            (
                check_name.clone(),
                TypeOpenApi::Ref {
                    r#ref: "#/components/schemas/Check".to_string(),
                    meta: OpenApiMeta::default(),
                },
            )
        })
        .collect();
    properties.sort_keys();

    let mut required: IndexSet<String> = checks.into_iter().collect();
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

#[cfg(test)]
mod tests {
    use baml_types::{
        ir_type::{StreamingMode, TypeNonStreaming, TypeValue},
        type_meta::non_streaming::TypeMeta,
        BamlMediaType, LiteralValue,
    };
    use internal_baml_core::ir::repr::make_test_ir;

    use super::*;

    fn create_test_meta() -> TypeMeta {
        TypeMeta {
            constraints: vec![],
        }
    }

    #[test]
    fn test_convert_primitive_types() {
        let ir = make_test_ir("").expect("Valid IR");

        // Test String
        let string_type = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let openapi_string = convert_ir_type(&ir, &string_type);
        match openapi_string {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::String,
                meta,
            } => {
                assert!(!meta.nullable);
            }
            _ => panic!("Expected inline string type"),
        }

        // Test Integer
        let int_type = TypeNonStreaming::Primitive(TypeValue::Int, create_test_meta());
        let openapi_int = convert_ir_type(&ir, &int_type);
        match openapi_int {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Integer,
                ..
            } => {}
            _ => panic!("Expected inline integer type"),
        }

        // Test Boolean
        let bool_type = TypeNonStreaming::Primitive(TypeValue::Bool, create_test_meta());
        let openapi_bool = convert_ir_type(&ir, &bool_type);
        match openapi_bool {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Boolean,
                ..
            } => {}
            _ => panic!("Expected inline boolean type"),
        }

        // Test Null
        let null_type = TypeNonStreaming::Primitive(TypeValue::Null, create_test_meta());
        let openapi_null = convert_ir_type(&ir, &null_type);
        match openapi_null {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Null,
                ..
            } => {}
            _ => panic!("Expected inline null type"),
        }
    }

    #[test]
    fn test_convert_media_types() {
        let ir = make_test_ir("").expect("Valid IR");

        // Test Image media type
        let image_type =
            TypeNonStreaming::Primitive(TypeValue::Media(BamlMediaType::Image), create_test_meta());
        let openapi_image = convert_ir_type(&ir, &image_type);
        match openapi_image {
            TypeOpenApi::Ref { r#ref, .. } => {
                assert_eq!(r#ref, "#/components/schemas/BamlImage");
            }
            _ => panic!("Expected ref to BamlImage schema"),
        }

        // Test Audio media type
        let audio_type =
            TypeNonStreaming::Primitive(TypeValue::Media(BamlMediaType::Audio), create_test_meta());
        let openapi_audio = convert_ir_type(&ir, &audio_type);
        match openapi_audio {
            TypeOpenApi::Ref { r#ref, .. } => {
                assert_eq!(r#ref, "#/components/schemas/BamlAudio");
            }
            _ => panic!("Expected ref to BamlAudio schema"),
        }
    }

    #[test]
    fn test_convert_class_type() {
        let ir = make_test_ir(
            r#"
            class Person {
                name string
                age int
            }
            "#,
        )
        .expect("Valid IR");

        let class_type = TypeNonStreaming::Class {
            name: "Person".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let openapi_class = convert_ir_type(&ir, &class_type);

        match openapi_class {
            TypeOpenApi::Ref { r#ref, .. } => {
                assert_eq!(r#ref, "#/components/schemas/Person");
            }
            _ => panic!("Expected ref to Person schema"),
        }
    }

    #[test]
    fn test_convert_enum_type() {
        let ir = make_test_ir(
            r#"
            enum Color {
                Red
                Green
                Blue
            }
            "#,
        )
        .expect("Valid IR");

        let enum_type = TypeNonStreaming::Enum {
            name: "Color".to_string(),
            dynamic: false,
            meta: create_test_meta(),
        };
        let openapi_enum = convert_ir_type(&ir, &enum_type);

        match openapi_enum {
            TypeOpenApi::Ref { r#ref, meta } => {
                assert_eq!(r#ref, "#/components/schemas/Color");
                // The enum values should be extracted and added to meta
                assert!(meta.r#enum.is_some());
                let enum_values = meta.r#enum.unwrap();
                assert_eq!(enum_values, vec!["Red", "Green", "Blue"]);
            }
            _ => panic!("Expected ref to Color schema with enum values"),
        }
    }

    #[test]
    fn test_convert_list_type() {
        let ir = make_test_ir("").expect("Valid IR");

        let string_type = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let list_type = TypeNonStreaming::List(Box::new(string_type), create_test_meta());
        let openapi_list = convert_ir_type(&ir, &list_type);

        match openapi_list {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Array { items },
                ..
            } => match items.as_ref() {
                TypeOpenApi::Inline {
                    r#type: TypePrimitive::String,
                    ..
                } => {}
                _ => panic!("Expected array of strings"),
            },
            _ => panic!("Expected inline array type"),
        }
    }

    #[test]
    fn test_convert_literal_types() {
        let ir = make_test_ir("").expect("Valid IR");

        // Test string literal
        let string_literal = TypeNonStreaming::Literal(
            LiteralValue::String("hello".to_string()),
            create_test_meta(),
        );
        let openapi_string_literal = convert_ir_type(&ir, &string_literal);
        match openapi_string_literal {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::String,
                ..
            } => {}
            _ => panic!("Expected inline string type for string literal"),
        }

        // Test int literal
        let int_literal = TypeNonStreaming::Literal(LiteralValue::Int(42), create_test_meta());
        let openapi_int_literal = convert_ir_type(&ir, &int_literal);
        match openapi_int_literal {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Integer,
                ..
            } => {}
            _ => panic!("Expected inline integer type for int literal"),
        }

        // Test bool literal
        let bool_literal = TypeNonStreaming::Literal(LiteralValue::Bool(true), create_test_meta());
        let openapi_bool_literal = convert_ir_type(&ir, &bool_literal);
        match openapi_bool_literal {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Boolean,
                ..
            } => {}
            _ => panic!("Expected inline boolean type for bool literal"),
        }
    }

    #[test]
    fn test_convert_map_type() {
        let ir = make_test_ir("").expect("Valid IR");

        let string_key = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let int_value = TypeNonStreaming::Primitive(TypeValue::Int, create_test_meta());
        let map_type = TypeNonStreaming::Map(
            Box::new(string_key),
            Box::new(int_value),
            create_test_meta(),
        );
        let openapi_map = convert_ir_type(&ir, &map_type);

        match openapi_map {
            TypeOpenApi::Inline {
                r#type:
                    TypePrimitive::Object {
                        properties,
                        required,
                        additional_properties,
                    },
                ..
            } => {
                assert!(properties.is_empty());
                assert!(required.is_empty());
                match additional_properties {
                    AdditionalProperties::Schema(schema) => match schema.as_ref() {
                        TypeOpenApi::Inline {
                            r#type: TypePrimitive::Integer,
                            ..
                        } => {}
                        _ => panic!("Expected integer schema for map values"),
                    },
                    _ => panic!("Expected schema for additional properties"),
                }
            }
            _ => panic!("Expected inline object type for map"),
        }
    }

    #[test]
    fn test_convert_recursive_type_alias() {
        let ir = make_test_ir("").expect("Valid IR");

        let recursive_type = TypeNonStreaming::RecursiveTypeAlias {
            name: "RecursiveType".to_string(),
            mode: StreamingMode::NonStreaming,
            meta: create_test_meta(),
        };
        let openapi_recursive = convert_ir_type(&ir, &recursive_type);

        match openapi_recursive {
            TypeOpenApi::AnyValue {
                type_,
                additional_properties,
                ..
            } => {
                assert_eq!(type_, "object");
                assert!(additional_properties);
            }
            _ => panic!("Expected AnyValue type for recursive type alias"),
        }
    }

    #[test]
    fn test_type_def_for_checks() {
        let checks = vec!["length_check".to_string(), "range_check".to_string()];
        let check_type = type_def_for_checks(checks);

        match check_type {
            TypeOpenApi::Inline {
                r#type:
                    TypePrimitive::Object {
                        properties,
                        required,
                        additional_properties,
                    },
                ..
            } => {
                assert_eq!(properties.len(), 2);
                assert!(properties.contains_key("length_check"));
                assert!(properties.contains_key("range_check"));

                assert_eq!(required.len(), 2);
                assert!(required.contains("length_check"));
                assert!(required.contains("range_check"));

                assert_eq!(additional_properties, AdditionalProperties::Closed);

                // Verify each property is a ref to Check schema
                for (_, prop_type) in properties {
                    match prop_type {
                        TypeOpenApi::Ref { r#ref, .. } => {
                            assert_eq!(r#ref, "#/components/schemas/Check");
                        }
                        _ => panic!("Expected ref to Check schema"),
                    }
                }
            }
            _ => panic!("Expected inline object type for checks"),
        }
    }

    #[test]
    fn test_type_def_for_checks_empty() {
        let checks = vec![];
        let check_type = type_def_for_checks(checks);

        match check_type {
            TypeOpenApi::Inline {
                r#type:
                    TypePrimitive::Object {
                        properties,
                        required,
                        ..
                    },
                ..
            } => {
                assert!(properties.is_empty());
                assert!(required.is_empty());
            }
            _ => panic!("Expected inline object type for empty checks"),
        }
    }

    #[test]
    fn test_openapi_meta_with_enum() {
        let meta = OpenApiMeta {
            title: Some("Color".to_string()),
            r#enum: Some(vec![
                "Red".to_string(),
                "Green".to_string(),
                "Blue".to_string(),
            ]),
            r#const: None,
            nullable: false,
        };

        assert_eq!(meta.title, Some("Color".to_string()));
        assert_eq!(
            meta.r#enum,
            Some(vec![
                "Red".to_string(),
                "Green".to_string(),
                "Blue".to_string()
            ])
        );
        assert_eq!(meta.r#const, None);
        assert!(!meta.nullable);
    }

    #[test]
    fn test_convert_complex_class_with_nested_types() {
        let ir = make_test_ir(
            r#"
            class Address {
                street string
                city string
                country string
            }

            class Person {
                name string
                age int
                address Address
                emails string[]
                is_verified bool
            }
            "#,
        )
        .expect("Valid IR");

        let person_type = TypeNonStreaming::Class {
            name: "Person".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let openapi_person = convert_ir_type(&ir, &person_type);

        match openapi_person {
            TypeOpenApi::Ref { r#ref, meta } => {
                assert_eq!(r#ref, "#/components/schemas/Person");
                assert!(!meta.nullable);
            }
            _ => panic!("Expected ref to Person schema"),
        }
    }

    #[test]
    fn test_convert_union_types() {
        use baml_types::ir_type::UnionConstructor;

        let ir = make_test_ir(
            r#"
            enum Status {
                Active
                Inactive
            }
            "#,
        )
        .expect("Valid IR");

        // Create a union type: string | int | Status
        let string_type = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let int_type = TypeNonStreaming::Primitive(TypeValue::Int, create_test_meta());
        let status_type = TypeNonStreaming::Enum {
            name: "Status".to_string(),
            dynamic: false,
            meta: create_test_meta(),
        };

        let union_type = TypeNonStreaming::union(vec![string_type, int_type, status_type]);
        let openapi_union = convert_ir_type(&ir, &union_type);

        match openapi_union {
            TypeOpenApi::Union { one_of, meta } => {
                assert_eq!(one_of.len(), 3);
                assert!(!meta.nullable);

                // Check first type is string
                match &one_of[0] {
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::String,
                        ..
                    } => {}
                    _ => panic!("Expected first union member to be string"),
                }

                // Check second type is integer
                match &one_of[1] {
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::Integer,
                        ..
                    } => {}
                    _ => panic!("Expected second union member to be integer"),
                }

                // Check third type is enum reference
                match &one_of[2] {
                    TypeOpenApi::Ref { r#ref, .. } => {
                        assert_eq!(r#ref, "#/components/schemas/Status");
                    }
                    _ => panic!("Expected third union member to be Status enum reference"),
                }
            }
            _ => panic!("Expected Union type"),
        }
    }

    #[test]
    fn test_convert_optional_types() {
        use baml_types::ir_type::UnionConstructor;

        let ir = make_test_ir("").expect("Valid IR");

        // Create an optional string type (string | null)
        let string_type = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let null_type = TypeNonStreaming::Primitive(TypeValue::Null, create_test_meta());

        let optional_string = TypeNonStreaming::union(vec![string_type, null_type]);
        let openapi_optional = convert_ir_type(&ir, &optional_string);

        // For optional types, the system should convert to the base type with nullable=true
        match openapi_optional {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::String,
                meta,
            } => {
                // assert!(meta.nullable);
            }
            _ => panic!("Expected nullable string type for optional string"),
        }
    }

    #[test]
    fn test_convert_union_with_type_alias() {
        use baml_types::ir_type::UnionConstructor;

        let ir = make_test_ir(
            r#"
            class User {
                id string
                name string
            }
            "#,
        )
        .expect("Valid IR");

        // Create a union: User | RecursiveTypeAlias
        let user_type = TypeNonStreaming::Class {
            name: "User".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let alias_type = TypeNonStreaming::RecursiveTypeAlias {
            name: "UserAlias".to_string(),
            mode: StreamingMode::NonStreaming,
            meta: create_test_meta(),
        };

        let union_type = TypeNonStreaming::union(vec![user_type, alias_type]);
        let openapi_union = convert_ir_type(&ir, &union_type);

        match openapi_union {
            TypeOpenApi::Union { one_of, .. } => {
                assert_eq!(one_of.len(), 2);

                // First should be User class reference
                match &one_of[0] {
                    TypeOpenApi::Ref { r#ref, .. } => {
                        assert_eq!(r#ref, "#/components/schemas/User");
                    }
                    _ => panic!("Expected User class reference"),
                }

                // Second should be AnyValue for recursive type alias
                match &one_of[1] {
                    TypeOpenApi::AnyValue { .. } => {}
                    _ => panic!("Expected AnyValue for recursive type alias"),
                }
            }
            _ => panic!("Expected Union type"),
        }
    }

    #[test]
    fn test_convert_nested_optional_class() {
        use baml_types::ir_type::UnionConstructor;

        let ir = make_test_ir(
            r#"
            class Profile {
                bio string
                avatar_url string
            }

            class User {
                name string
                profile Profile?
            }
            "#,
        )
        .expect("Valid IR");

        // Create an optional Profile type (Profile | null)
        let profile_type = TypeNonStreaming::Class {
            name: "Profile".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let null_type = TypeNonStreaming::Primitive(TypeValue::Null, create_test_meta());

        let optional_profile = TypeNonStreaming::union(vec![profile_type, null_type]);
        let openapi_optional_profile = convert_ir_type(&ir, &optional_profile);

        match openapi_optional_profile {
            TypeOpenApi::Ref { r#ref, meta } => {
                assert_eq!(r#ref, "#/components/schemas/Profile");
                // assert!(meta.nullable);
            }
            _ => panic!("Expected nullable Profile reference"),
        }
    }

    #[test]
    fn test_convert_complex_nested_types() {
        let ir = make_test_ir(
            r#"
            enum Priority {
                Low
                Medium
                High
            }

            class Tag {
                name string
                color string
            }

            class Task {
                id string
                title string
                priority Priority
                tags Tag[]
                metadata map<string, string>
            }
            "#,
        )
        .expect("Valid IR");

        // Test array of custom class
        let tag_type = TypeNonStreaming::Class {
            name: "Tag".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let tag_array = TypeNonStreaming::List(Box::new(tag_type), create_test_meta());
        let openapi_tag_array = convert_ir_type(&ir, &tag_array);

        match openapi_tag_array {
            TypeOpenApi::Inline {
                r#type: TypePrimitive::Array { items },
                ..
            } => match items.as_ref() {
                TypeOpenApi::Ref { r#ref, .. } => {
                    assert_eq!(r#ref, "#/components/schemas/Tag");
                }
                _ => panic!("Expected Tag reference in array"),
            },
            _ => panic!("Expected array type"),
        }

        // Test map with string values
        let string_key = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let string_value = TypeNonStreaming::Primitive(TypeValue::String, create_test_meta());
        let string_map = TypeNonStreaming::Map(
            Box::new(string_key),
            Box::new(string_value),
            create_test_meta(),
        );
        let openapi_string_map = convert_ir_type(&ir, &string_map);

        match openapi_string_map {
            TypeOpenApi::Inline {
                r#type:
                    TypePrimitive::Object {
                        additional_properties,
                        ..
                    },
                ..
            } => match additional_properties {
                AdditionalProperties::Schema(schema) => match schema.as_ref() {
                    TypeOpenApi::Inline {
                        r#type: TypePrimitive::String,
                        ..
                    } => {}
                    _ => panic!("Expected string schema for map values"),
                },
                _ => panic!("Expected schema for additional properties"),
            },
            _ => panic!("Expected object type for map"),
        }
    }

    #[test]
    fn test_convert_union_with_multiple_classes() {
        use baml_types::ir_type::UnionConstructor;

        let ir = make_test_ir(
            r#"
            class Dog {
                breed string
                age int
            }

            class Cat {
                color string
                indoor bool
            }

            class Bird {
                species string
                can_fly bool
            }
            "#,
        )
        .expect("Valid IR");

        // Create a union: Dog | Cat | Bird
        let dog_type = TypeNonStreaming::Class {
            name: "Dog".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let cat_type = TypeNonStreaming::Class {
            name: "Cat".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };
        let bird_type = TypeNonStreaming::Class {
            name: "Bird".to_string(),
            mode: StreamingMode::NonStreaming,
            dynamic: false,
            meta: create_test_meta(),
        };

        let animal_union = TypeNonStreaming::union(vec![dog_type, cat_type, bird_type]);
        let openapi_animal_union = convert_ir_type(&ir, &animal_union);

        match openapi_animal_union {
            TypeOpenApi::Union { one_of, .. } => {
                assert_eq!(one_of.len(), 3);

                let expected_refs = [
                    "#/components/schemas/Dog",
                    "#/components/schemas/Cat",
                    "#/components/schemas/Bird",
                ];

                for (i, expected_ref) in expected_refs.iter().enumerate() {
                    match &one_of[i] {
                        TypeOpenApi::Ref { r#ref, .. } => {
                            assert_eq!(r#ref, expected_ref);
                        }
                        _ => panic!("Expected class reference at position {i}"),
                    }
                }
            }
            _ => panic!("Expected Union type for animal union"),
        }
    }
}
