use anyhow::Context;
use anyhow::Result;
use baml_types::FieldType;
use baml_types::TypeValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::types as jt;
use internal_baml_jinja::types::{OutputFormatContent, RenderOptions};
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::f32::consts::E;

use crate::internal::prompt_renderer;
use crate::RuntimeContext;

use super::TypeBuilder;

pub enum OutputFormatMode {
    JsonSchema,
    TsInterface,
}

// can you model a list directly in pydantic?

// a dict is modelled as "additionalProperties" wtf?
//   - humans don't understand this, why would an LLM?

// TODO:
// - maps, unions, tuples
// - errors.is_empty() is a bad pattern, should use whether or not new errors were added as a signal
// - root def should use schema.title as the type name
// - handle inline types? need to figure out a schema for the refs
#[derive(Debug, Deserialize)]
pub struct JsonSchema {
    #[serde(default, rename = "$defs")]
    defs: HashMap<String, TypeDef>,

    #[serde(flatten)]
    type_spec_with_meta: TypeSpecWithMeta,
}

#[derive(Debug, Deserialize)]
struct TypeSpecWithMeta {
    /// Pydantic includes this by default.
    #[serde(rename = "title")]
    _title: Option<String>,

    #[serde(flatten)]
    type_spec: TypeSpec,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TypeSpec {
    #[serde(rename = "string")]
    Ref(TypeRef),
    Inline(TypeDef),
    Union(UnionRef),
}

#[derive(Debug, Deserialize)]
struct UnionRef {
    #[serde(rename = "anyOf")]
    any_of: Vec<TypeSpecWithMeta>,
}

#[derive(Debug, Deserialize)]
struct TypeRef {
    #[serde(rename = "$ref")]
    r#ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum TypeDef {
    #[serde(rename = "string")]
    StringOrEnum(StringOrEnumDef),

    #[serde(rename = "object")]
    Class(ClassDef),

    #[serde(rename = "array")]
    Array(Box<ArrayDef>),

    #[serde(rename = "integer")]
    Int,

    #[serde(rename = "number")]
    Float,

    #[serde(rename = "boolean")]
    Bool,

    #[serde(rename = "null")]
    Null,
}

#[derive(Debug, Deserialize)]
struct StringOrEnumDef {
    r#enum: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ClassDef {
    #[serde(default)]
    properties: HashMap<String, TypeSpecWithMeta>,

    #[serde(default)]
    required: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ArrayDef {
    items: TypeSpecWithMeta,
}

//----------------------------------------------------------------------

#[derive(Debug)]
struct SerializationError {
    position: Vec<String>,
    message: String,
}

#[derive(Debug)]
enum RefinedType {
    Class,
    Enum,
}

#[derive(Debug)]
struct RefinedTypeResolver {
    refined: HashMap<String, RefinedType>,
}

impl RefinedTypeResolver {
    fn record_type(&mut self, position: &Vec<String>, refined_type: RefinedType) {
        self.refined.insert(position.join("/"), refined_type);
    }

    fn resolve_ref(&self, name: &str) -> Result<FieldType> {
        // TODO: this does not handle inline-defined types
        let type_name = name.strip_prefix("#/$defs/").unwrap_or(name);
        match self.refined.get(name) {
            Some(RefinedType::Class) => Ok(FieldType::Class(type_name.to_string())),
            Some(RefinedType::Enum) => Ok(FieldType::Enum(type_name.to_string())),
            None => anyhow::bail!("Unresolved ref: {}", name),
        }
    }
}

// impl JsonSchema {
//     pub fn classes_and_enums(&self) -> Result<()> {
//         let position = vec!["#".to_string()];
//         let mut resolver = RefinedTypeResolver {
//             refined: HashMap::new(),
//         };
//         let mut errors = vec![];

//         match self.visit1(position, &mut resolver, &mut errors) {
//             Ok(_) => {
//                 log::info!("Resolved classes and enums: {:?}", resolver.refined);
//                 Ok(())
//             }
//             Err(_) => {
//                 for e in errors.iter() {
//                     log::error!("Error: {:?}", e);
//                 }
//                 anyhow::bail!("Failed to resolve classes and enums");
//             }
//         }
//     }
// }

// trait JsonSchemaVisitor {
//     fn visit_enum(&mut self, position: Vec<String>, name: &str, values: &Vec<String>)
//         -> Result<()>;

//     fn visit_class(&mut self, position: Vec<String>, name: &str) -> Result<()>;
// }

// impl JsonSchemaVisitor for RefinedTypeResolver {
//     fn visit_enum(
//         &mut self,
//         position: Vec<String>,
//         name: &str,
//         values: &Vec<String>,
//     ) -> Result<()> {
//         self.refined.insert(position.join("/"), RefinedType::Enum);
//         Ok(())
//     }

//     fn visit_class(&mut self, position: Vec<String>, name: &str) -> Result<()> {
//         self.refined.insert(position.join("/"), RefinedType::Class);
//         Ok(())
//     }
// }

// trait Visit0<V> {
//     fn visit0(
//         &self,
//         position: Vec<String>,
//         visitor: &mut V,
//         errors: &mut Vec<SerializationError>,
//     ) -> core::result::Result<(), ()>;
// }

// impl<V> Visit0<V> for &JsonSchema
// where
//     V: JsonSchemaVisitor,
// {
//     fn visit0(
//         &self,
//         position: Vec<String>,
//         visitor: &mut V,
//         errors: &mut Vec<SerializationError>,
//     ) -> core::result::Result<(), ()> {
//         for (name, type_def) in self.defs.iter() {
//             let mut position = position.clone();
//             position.push("$defs".to_string());
//             position.push(name.clone());

//             let _ = type_def.visit0(position, visitor, errors);
//         }

//         for (name, prop) in self.properties.iter() {
//             let mut position = position.clone();
//             position.push("properties".to_string());
//             position.push(name.clone());

//             let _ = Visit0::visit0(&(prop.type_spec), position, visitor, errors);
//         }

//         if !errors.is_empty() {
//             return Err(());
//         }

//         Ok(())
//     }
// }

// impl<V> Visit0<V> for &TypeSpec
// where
//     V: JsonSchemaVisitor,
// {
//     fn visit0(
//         &self,
//         position: Vec<String>,
//         visitor: &mut V,
//         errors: &mut Vec<SerializationError>,
//     ) -> core::result::Result<(), ()> {
//         match self {
//             TypeSpec::Inline(type_def) => {
//                 let mut position = position.clone();
//                 position.push("???inline???".to_string());

//                 let _ = type_def.visit0(position, visitor, errors);
//             }
//             TypeSpec::Ref(_) => return Ok(()),
//             TypeSpec::Union(union_ref) => {
//                 for (i, t) in union_ref.any_of.iter().enumerate() {
//                     let mut position = position.clone();
//                     position.push(format!("anyOf[{}]", i));

//                     let _ = &(t.type_spec).visit0(position, visitor, errors);
//                 }
//             }
//         }
//         if !errors.is_empty() {
//             return Err(());
//         }
//         Ok(())
//     }
// }

// impl<V> Visit0<V> for &TypeDef
// where
//     V: JsonSchemaVisitor,
// {
//     fn visit0(
//         &self,
//         position: Vec<String>,
//         visitor: &mut V,
//         errors: &mut Vec<SerializationError>,
//     ) -> core::result::Result<(), ()> {
//         match self {
//             TypeDef::StringOrEnum(StringOrEnumDef {
//                 r#enum: Some(enum_values),
//             }) => {
//                 visitor.visit_enum(position, "<TODO-name>", enum_values);
//                 // resolver
//                 //     .refined
//                 //     .insert(position.join("/"), RefinedType::Enum);
//             }
//             TypeDef::Class(class_def) => {
//                 visitor.visit_class(position, "<TODO-name>");
//                 // resolver
//                 //     .refined
//                 //     .insert(position.join("/"), RefinedType::Class);

//                 for (field_name, field_type) in class_def.properties.iter() {
//                     let mut position = position.clone();
//                     position.push(format!("properties:{}", field_name));

//                     let _ = field_type.type_spec.visit0(position, visitor, errors);
//                 }
//             }
//             TypeDef::Array(array_def) => {
//                 let mut position = position.clone();
//                 position.push("items".to_string());
//                 let _ = array_def.items.type_spec.visit0(position, visitor, errors);
//             }
//             _ => {}
//         }
//         if !errors.is_empty() {
//             return Err(());
//         }
//         Ok(())
//     }
// }

trait Visit1 {
    /// Discover all enums and class refs
    fn visit1(
        &self,
        position: Vec<String>,
        resolver: &mut RefinedTypeResolver,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()>;
}

impl Visit1 for JsonSchema {
    fn visit1(
        &self,
        position: Vec<String>,
        resolver: &mut RefinedTypeResolver,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        for (name, type_def) in self.defs.iter() {
            let mut position = position.clone();
            position.push("$defs".to_string());
            position.push(name.clone());

            let _ = type_def.visit1(position, resolver, errors);
        }

        let _ = self
            .type_spec_with_meta
            .visit1(position.clone(), resolver, errors);

        if !errors.is_empty() {
            return Err(());
        }

        Ok(())
    }
}

fn position_to_type_name(position: &Vec<String>) -> Result<String> {
    if position.len() == 3 && position[0] == "#" && position[1] == "$defs" {
        return Ok(position[2].clone());
    }

    if position.len() == 1 && position[0] == "#" {
        return Ok("#".to_string());
    }

    anyhow::bail!("Only top-level defs are supported: {:?}", position)
}

impl Visit1 for TypeSpecWithMeta {
    fn visit1(
        &self,
        position: Vec<String>,
        resolver: &mut RefinedTypeResolver,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        match &self.type_spec {
            TypeSpec::Inline(type_def) => {
                let _ = type_def.visit1(position, resolver, errors);
            }
            TypeSpec::Ref(_) => {}
            TypeSpec::Union(union_ref) => {
                for (i, t) in union_ref.any_of.iter().enumerate() {
                    let mut position = position.clone();
                    position.push(format!("anyOf[{}]", i));

                    let _ = t.visit1(position, resolver, errors);
                }
            }
        }
        if !errors.is_empty() {
            return Err(());
        }
        Ok(())
    }
}

impl Visit1 for TypeDef {
    fn visit1(
        &self,
        position: Vec<String>,
        resolver: &mut RefinedTypeResolver,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        match self {
            TypeDef::StringOrEnum(StringOrEnumDef { r#enum: Some(_) }) => {
                resolver.record_type(&position, RefinedType::Enum);
                Ok(())
            }
            TypeDef::Class(class_def) => {
                resolver.record_type(&position, RefinedType::Class);

                let mut ret = Ok(());

                for (field_name, field_type) in class_def.properties.iter() {
                    let mut position = position.clone();
                    position.push(format!("properties:{}", field_name));

                    if let Err(field_err) = field_type.visit1(position, resolver, errors) {
                        ret = Err(field_err);
                    }
                }

                ret
            }
            TypeDef::Array(array_def) => {
                let mut position = position.clone();
                position.push("items".to_string());
                array_def.items.visit1(position, resolver, errors)
            }
            _ => Ok(()),
        }
    }
}

//----------------------------------------------------------------------

struct TypeCollector {
    tb: TypeBuilder,
    resolver: RefinedTypeResolver,
}

impl TypeCollector {
    fn add_class(
        &self,
        position: &Vec<String>,
        fields: Vec<(String, FieldType)>,
    ) -> Result<FieldType> {
        let class_name = position_to_type_name(&position)?;
        let arc = self.tb.class(class_name.as_str());
        let cb = arc.lock().unwrap();

        for (field_name, field_type) in fields {
            cb.property(&field_name).lock().unwrap().r#type(field_type);
        }

        Ok(FieldType::class(class_name.as_str()))
    }

    fn add_enum(&self, position: &Vec<String>, enums: &[String]) -> Result<FieldType> {
        let enum_name = position_to_type_name(&position)?;
        let arc = self.tb.r#enum(enum_name.as_str());
        let eb = arc.lock().unwrap();

        for v in enums.iter() {
            eb.value(v);
        }

        Ok(FieldType::r#enum(enum_name.as_str()))
    }
}
trait Visit2 {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<FieldType, ()>;
}

impl Visit2 for JsonSchema {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<FieldType, ()> {
        for (name, type_def) in self.defs.iter() {
            let mut position = position.clone();
            position.push("$defs".to_string());
            position.push(name.clone());

            let _ = type_def.visit2(position, v, errors);
        }

        self.type_spec_with_meta.visit2(position.clone(), v, errors)
    }
}

impl Visit2 for TypeSpecWithMeta {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<FieldType, ()> {
        match &self.type_spec {
            TypeSpec::Inline(type_def) => type_def.visit2(position, v, errors),
            TypeSpec::Ref(TypeRef { ref r#ref }) => match v.resolver.resolve_ref(r#ref) {
                Ok(t) => Ok(t),
                Err(e) => {
                    errors.push(SerializationError {
                        position: position.clone(),
                        message: format!("{:?}", e),
                    });
                    Err(())
                }
            },
            TypeSpec::Union(union_ref) => {
                let mut any_of = vec![];

                for (i, t) in union_ref.any_of.iter().enumerate() {
                    let mut position = position.clone();
                    position.push(format!("anyOf[{}]", i));

                    if let Ok(one_of) = t.visit2(position, v, errors) {
                        any_of.push(one_of);
                    }
                }

                if !errors.is_empty() {
                    return Err(());
                }
                Ok(FieldType::union(any_of))
            }
        }
    }
}

impl Visit2 for TypeDef {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<FieldType, ()> {
        Ok(match self {
            TypeDef::Class(class_def) => {
                let fields = class_def
                    .properties
                    .iter()
                    .map(|(field_name, field_type)| {
                        let mut position = position.clone();
                        position.push(format!("properties:{}", field_name));

                        match field_type.visit2(position, v, errors) {
                            Ok(t) => Ok((
                                field_name.clone(),
                                if class_def.required.contains(&field_name) {
                                    t
                                } else {
                                    FieldType::Optional(Box::new(t))
                                },
                            )),
                            Err(()) => Err(()),
                        }
                    })
                    .collect::<Result<Vec<_>, ()>>()?;

                match v.add_class(&position, fields) {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(SerializationError {
                            position: position.clone(),
                            message: format!("Failed to add class: {:?}", e),
                        });
                        return Err(());
                    }
                }
            }
            TypeDef::Array(array_def) => {
                let mut position = position.clone();
                position.push("items".to_string());
                array_def
                    .items
                    .visit2(position, v, errors)
                    .map(|t| FieldType::List(Box::new(t)))?
            }
            TypeDef::StringOrEnum(StringOrEnumDef {
                r#enum: Some(enum_values),
            }) => match v.add_enum(&position, enum_values.as_slice()) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(SerializationError {
                        position: position.clone(),
                        message: format!("Failed to add class: {:?}", e),
                    });
                    return Err(());
                }
            },
            TypeDef::StringOrEnum(StringOrEnumDef { r#enum: None }) => {
                FieldType::Primitive(TypeValue::String)
            }
            TypeDef::Int => FieldType::Primitive(TypeValue::Int),
            TypeDef::Float => FieldType::Primitive(TypeValue::Float),
            TypeDef::Bool => FieldType::Primitive(TypeValue::Bool),
            TypeDef::Null => FieldType::Primitive(TypeValue::Null),
        })
    }
}

pub struct JsonSchemaType {
    inner: FieldType,
}

impl JsonSchemaType {
    pub fn output_format(&self, tb: &TypeBuilder) -> Result<String> {
        let (class_overrides, enum_overrides) = tb.to_overrides();
        let ctx = RuntimeContext {
            env: HashMap::new(),
            tags: HashMap::new(),
            class_override: class_overrides,
            enum_overrides: enum_overrides,
        };

        let ir = IntermediateRepr::create_empty();

        let output_format = prompt_renderer::render_output_format(&ir, &ctx, &self.inner)
            .context("Failed to render output format")?;

        match output_format.render(RenderOptions::default()) {
            Ok(Some(s)) => Ok(s),
            Ok(None) => anyhow::bail!("Failed to render output format"),
            Err(e) => anyhow::bail!("Failed to render output format: {:?}", e),
        }
    }
}

pub trait AddJsonSchema {
    fn add_json_schema(&self, schema: String) -> Result<JsonSchemaType>;
}

impl AddJsonSchema for TypeBuilder {
    fn add_json_schema(&self, schema: String) -> Result<JsonSchemaType> {
        let schema: JsonSchema = serde_json::from_str(&schema)?;
        println!("{:#?}", schema);

        let position = vec!["#".to_string()];
        let mut errors = Vec::new();

        let mut resolver = RefinedTypeResolver {
            refined: HashMap::new(),
        };
        let Ok(_) = schema.visit1(position.clone(), &mut resolver, &mut errors) else {
            anyhow::bail!("Errors happened during visit1: {:#?}", errors)
        };

        println!("{:#?}", resolver);

        let mut tc = TypeCollector {
            tb: TypeBuilder::new(),
            resolver,
        };

        let Ok(field_type) = schema.visit2(position.clone(), &mut tc, &mut errors) else {
            anyhow::bail!("Errors happened during visit2: {:#?}", errors)
        };

        self.classes.lock().unwrap().extend(
            tc.tb
                .classes
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        self.enums.lock().unwrap().extend(
            tc.tb
                .enums
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );

        println!("{:#?}", self);

        Ok(JsonSchemaType { inner: field_type })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_output_format() -> Result<()> {
        let model_json_schema = serde_json::json!({
          "$defs": {
            "Role": {
              "enum": [
                "admin",
                "user",
                "guest"
              ],
              "title": "Role",
              "type": "string"
            },
            "__main____Address": {
              "properties": {
                "street": {
                  "title": "Street",
                  "type": "string"
                },
                "city": {
                  "title": "City",
                  "type": "string"
                },
                "postal_code": {
                  "title": "Postal Code",
                  "type": "string"
                }
              },
              "required": [
                "street",
                "city",
                "postal_code"
              ],
              "title": "Address",
              "type": "object"
            },
            "other_demo__Address": {
              "properties": {
                "street": {
                  "title": "Street",
                  "type": "string"
                },
                "city": {
                  "title": "City",
                  "type": "string"
                },
                "postal_code": {
                  "title": "Postal Code",
                  "type": "string"
                }
              },
              "required": [
                "street",
                "city",
                "postal_code"
              ],
              "title": "Address",
              "type": "object"
            },
            "zebra__Address": {
              "properties": {
                "wrapped": {
                  "$ref": "#/$defs/other_demo__Address"
                }
              },
              "required": [
                "wrapped"
              ],
              "title": "Address",
              "type": "object"
            }
          },
          "properties": {
            "name": {
              "title": "Name",
              "type": "string"
            },
            "age": {
              "title": "Age",
              "type": "integer"
            },
            "roles": {
              "items": {
                "$ref": "#/$defs/Role"
              },
              "title": "Roles",
              "type": "array"
            },
            "primary_address": {
              "$ref": "#/$defs/__main____Address"
            },
            "secondary_addresses": {
              "anyOf": [
                {
                  "$ref": "#/$defs/other_demo__Address"
                },
                {
                  "items": {
                    "$ref": "#/$defs/other_demo__Address"
                  },
                  "type": "array"
                }
              ],
              "title": "Secondary Addresses"
            },
            "zebra_addresses": {
              "items": {
                "$ref": "#/$defs/zebra__Address"
              },
              "title": "Zebra Addresses",
              "type": "array"
            },
            "gpa": {
              "title": "Gpa",
              "type": "number"
            },
            "alive": {
              "title": "Alive",
              "type": "boolean"
            },
            "nope": {
              "title": "Nope",
              "type": "null"
            }
          },
          "required": [
            "name",
            "age",
            "roles",
            //"primary_address",
            "secondary_addresses",
            "zebra_addresses",
            "gpa",
            "alive",
            "nope"
          ],
          "title": "User",
          "type": "object"
        });

        let tb = TypeBuilder::new();
        let output_format = tb
            .add_json_schema(model_json_schema.to_string())?
            .output_format(&tb)?;

        println!("{}", output_format);

        Ok(())
    }

    #[test]
    fn test1() -> Result<()> {
        let model_json_schema = serde_json::json!({
          "enum": [
            "admin",
            "user",
            "guest"
          ],
          "title": "Role",
          "type": "string"
        });

        let tb = TypeBuilder::new();
        let output_format = tb
            .add_json_schema(model_json_schema.to_string())?
            .output_format(&tb)?;

        println!("{}", output_format);

        Ok(())
    }
}
