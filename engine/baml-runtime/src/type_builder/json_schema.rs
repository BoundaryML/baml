use anyhow::Context;
use anyhow::Result;
use baml_types::FieldType;
use baml_types::TypeValue;
use internal_baml_jinja::types as jt;
use internal_baml_jinja::types::{OutputFormatContent, RenderOptions};
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;

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
#[derive(Debug, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "$defs")]
    defs: HashMap<String, TypeDef>,

    #[serde(default)]
    properties: HashMap<String, TypeSpecWithMeta>,

    #[serde(default)]
    required: Vec<String>,

    r#type: String,
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
    fn resolve_ref(&self, name: &str) -> Result<FieldType> {
        match self.refined.get(name) {
            Some(RefinedType::Class) => Ok(FieldType::Class(name.to_string())),
            Some(RefinedType::Enum) => Ok(FieldType::Enum(name.to_string())),
            None => anyhow::bail!("Unknown ref: {}", name),
        }
    }
}

impl JsonSchema {
    pub fn classes_and_enums(&self) -> Result<()> {
        let position = vec!["#".to_string()];
        let mut resolver = RefinedTypeResolver {
            refined: HashMap::new(),
        };
        let mut errors = vec![];

        match self.visit1(position, &mut resolver, &mut errors) {
            Ok(_) => {
                log::info!("Resolved classes and enums: {:?}", resolver.refined);
                Ok(())
            }
            Err(_) => {
                for e in errors.iter() {
                    log::error!("Error: {:?}", e);
                }
                anyhow::bail!("Failed to resolve classes and enums");
            }
        }
    }
}

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

        for (name, prop) in self.properties.iter() {
            let mut position = position.clone();
            position.push("properties".to_string());
            position.push(name.clone());

            let _ = prop.type_spec.visit1(position, resolver, errors);
        }

        if !errors.is_empty() {
            return Err(());
        }

        Ok(())
    }
}

fn position_to_type_name(position: &Vec<String>) -> Result<String> {
    if position.len() == 2 {
        if position[0] == "$defs" {
            return Ok(position[1].clone());
        }
    }

    Ok(position.join("___"))
}

impl Visit1 for TypeSpec {
    fn visit1(
        &self,
        position: Vec<String>,
        resolver: &mut RefinedTypeResolver,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        match self {
            TypeSpec::Inline(type_def) => {
                let mut position = position.clone();
                position.push("???inline???".to_string());

                let _ = type_def.visit1(position, resolver, errors);
            }
            TypeSpec::Ref(_) => {}
            TypeSpec::Union(union_ref) => {
                for (i, t) in union_ref.any_of.iter().enumerate() {
                    let mut position = position.clone();
                    position.push(format!("anyOf[{}]", i));

                    let _ = t.type_spec.visit1(position, resolver, errors);
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
                match position_to_type_name(&position) {
                    Ok(name) => {
                        resolver.refined.insert(name, RefinedType::Enum);
                    }
                    Err(e) => {
                        errors.push(SerializationError {
                            position: position.clone(),
                            message: format!("{:?}", e),
                        });
                    }
                }
            }
            TypeDef::Class(class_def) => {
                match position_to_type_name(&position) {
                    Ok(name) => {
                        resolver.refined.insert(name, RefinedType::Class);
                    }
                    Err(e) => {
                        errors.push(SerializationError {
                            position: position.clone(),
                            message: format!("{:?}", e),
                        });
                    }
                }

                for (field_name, field_type) in class_def.properties.iter() {
                    let mut position = position.clone();
                    position.push(format!("properties_{}", field_name));

                    let _ = field_type.type_spec.visit1(position, resolver, errors);
                }
            }
            TypeDef::Array(array_def) => {
                let mut position = position.clone();
                position.push("items".to_string());
                let _ = array_def.items.type_spec.visit1(position, resolver, errors);
            }
            _ => {}
        }
        if !errors.is_empty() {
            return Err(());
        }
        Ok(())
    }
}

//----------------------------------------------------------------------

struct TypeCollector {
    tb: TypeBuilder,
    resolver: RefinedTypeResolver,
}

impl TypeCollector {
    fn schema_to_field_type(&self, type_spec: &TypeSpecWithMeta) -> Result<FieldType> {
        Ok(match type_spec.type_spec {
            TypeSpec::Inline(ref type_def) => match type_def {
                TypeDef::StringOrEnum(StringOrEnumDef { r#enum: None }) => {
                    FieldType::Primitive(TypeValue::String)
                }
                TypeDef::StringOrEnum(StringOrEnumDef { r#enum: Some(_) }) => {
                    anyhow::bail!("inline TypeDef for enum not allowed")
                }
                TypeDef::Int => FieldType::Primitive(TypeValue::Int),
                TypeDef::Float => FieldType::Primitive(TypeValue::Float),
                TypeDef::Bool => FieldType::Primitive(TypeValue::Bool),
                TypeDef::Null => FieldType::Primitive(TypeValue::Null),
                TypeDef::Array(array_def) => {
                    FieldType::List(Box::new(self.schema_to_field_type(&array_def.items)?))
                }
                TypeDef::Class(class_def) => anyhow::bail!("inline TypeDef for class not allowed"),
            },
            TypeSpec::Ref(TypeRef { ref r#ref }) => match r#ref.strip_prefix("#/$defs/") {
                Some(ref_name) => self.resolver.resolve_ref(ref_name)?,
                None => anyhow::bail!("Invalid ref: {}", r#ref),
            },
            TypeSpec::Union(UnionRef { ref any_of }) => FieldType::Union(
                any_of
                    .iter()
                    .map(|t| self.schema_to_field_type(t))
                    .collect::<Result<_>>()?,
            ),
        })
    }

    fn add_class(&self, position: &Vec<String>, class_def: &ClassDef) -> Result<()> {
        let arc = self.tb.class(position_to_type_name(&position)?.as_str());
        let cb = arc.lock().unwrap();

        for (field_name, field_type) in class_def.properties.iter() {
            let field_type: FieldType = self.schema_to_field_type(field_type)?;

            cb.property(&field_name).lock().unwrap().r#type(field_type);
        }

        Ok(())
    }

    fn add_enum(&self, position: &Vec<String>, enums: &[String]) -> Result<()> {
        let arc = self.tb.r#enum(position_to_type_name(&position)?.as_str());
        let eb = arc.lock().unwrap();

        for v in enums.iter() {
            eb.value(v);
        }

        Ok(())
    }
}
trait Visit2 {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()>;
}

impl Visit2 for JsonSchema {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        for (name, type_def) in self.defs.iter() {
            let mut position = position.clone();
            position.push("$defs".to_string());
            position.push(name.clone());

            let _ = type_def.visit2(position, v, errors);
        }

        let cb_arc = v.tb.class("OutputFormat");
        let cb = cb_arc.lock().unwrap();

        for (name, prop) in self.properties.iter() {
            let mut position = position.clone();
            position.push("properties".to_string());
            position.push(name.clone());

            let _ = prop.type_spec.visit2(position.clone(), v, errors);

            let cb_prop = cb.property(&name);
            match v.schema_to_field_type(prop) {
                Ok(t) => {
                    cb_prop.lock().unwrap().r#type(t);
                }
                Err(e) => {
                    errors.push(SerializationError {
                        position: position,
                        message: format!("{:?}", e),
                    });
                }
            }
        }

        if !errors.is_empty() {
            return Err(());
        }

        Ok(())
    }
}

impl Visit2 for TypeSpec {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        match self {
            TypeSpec::Inline(type_def) => {
                let mut position = position.clone();
                position.push("???inline???".to_string());

                let _ = type_def.visit2(position, v, errors);
            }
            TypeSpec::Ref(type_ref) => {}
            TypeSpec::Union(union_ref) => {
                for (i, t) in union_ref.any_of.iter().enumerate() {
                    let mut position = position.clone();
                    position.push(format!("anyOf[{}]", i));

                    let _ = t.type_spec.visit2(position, v, errors);
                }
            }
        }
        if !errors.is_empty() {
            return Err(());
        }
        Ok(())
    }
}

impl Visit2 for TypeDef {
    fn visit2(
        &self,
        position: Vec<String>,
        v: &mut TypeCollector,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        match self {
            TypeDef::StringOrEnum(StringOrEnumDef {
                r#enum: Some(enum_values),
            }) => {
                // resolver
                //     .refined
                //     .insert(position.join("/"), RefinedType::Enum);
                if let Err(e) = v.add_enum(&position, enum_values.as_slice()) {
                    errors.push(SerializationError {
                        position: position.clone(),
                        message: format!("{:?}", e),
                    });
                }
            }
            TypeDef::Class(class_def) => {
                if let Err(e) = v.add_class(&position, class_def) {
                    errors.push(SerializationError {
                        position: position.clone(),
                        message: format!("{:?}", e),
                    });
                }

                for (field_name, field_type) in class_def.properties.iter() {
                    let mut position = position.clone();
                    position.push(format!("properties:{}", field_name));

                    let _ = field_type.type_spec.visit2(position, v, errors);
                }
            }
            TypeDef::Array(array_def) => {
                let mut position = position.clone();
                position.push("items".to_string());
                let _ = array_def.items.type_spec.visit2(position, v, errors);
            }
            _ => {}
        }
        if !errors.is_empty() {
            return Err(());
        }
        Ok(())
    }
}

//----------------------------------------------------------------------
trait AddClassOrEnum {
    fn add_class(&self, name: &str, class_def: &ClassDef) -> Result<()>;
    fn add_enum(&self, name: &str, enum_values: &Vec<String>) -> Result<()>;

    /// Add refs to classes and enums
    fn visit2(&self) -> Result<()>;

    fn to_field_type(&self, type_spec: &TypeSpecWithMeta) -> Result<FieldType>;
    fn resolve_ref(&self, name: &str) -> Result<FieldType>;
}

impl AddClassOrEnum for TypeBuilder {
    fn add_class(&self, class_name: &str, class_def: &ClassDef) -> Result<()> {
        let class_builder = self.class(&class_name);
        let class_builder = class_builder.lock().unwrap();
        for (property_name, property_type) in class_def.properties.iter() {
            class_builder
                .property(&property_name)
                .lock()
                .unwrap()
                .r#type(property_type.try_into()?);
        }

        Ok(())
    }

    fn add_enum(&self, enum_name: &str, enum_values: &Vec<String>) -> Result<()> {
        let enum_builder = self.r#enum(&enum_name);
        let enum_builder = enum_builder.lock().unwrap();
        for v in enum_values.iter() {
            enum_builder.value(&v);
        }
        Ok(())
    }

    fn visit2(&self) -> Result<()> {
        todo!()
    }

    fn to_field_type(&self, type_spec: &TypeSpecWithMeta) -> Result<FieldType> {
        Ok(match &type_spec.type_spec {
            TypeSpec::Inline(type_def) => match type_def {
                TypeDef::StringOrEnum(StringOrEnumDef { r#enum: None }) => {
                    FieldType::Primitive(TypeValue::String)
                }
                TypeDef::StringOrEnum(StringOrEnumDef { r#enum: Some(_) }) => {
                    anyhow::bail!("inline TypeDef for enum not allowed")
                }
                TypeDef::Int => FieldType::Primitive(TypeValue::Int),
                TypeDef::Float => FieldType::Primitive(TypeValue::Float),
                TypeDef::Bool => FieldType::Primitive(TypeValue::Bool),
                TypeDef::Null => FieldType::Primitive(TypeValue::Null),
                TypeDef::Array(array_def) => {
                    FieldType::List(Box::new(self.to_field_type(&array_def.items)?))
                }
                TypeDef::Class(class_def) => anyhow::bail!("inline TypeDef for class not allowed"),
            },
            TypeSpec::Ref(TypeRef { r#ref }) => match r#ref.strip_prefix("#/$defs/") {
                Some(ref_name) => self.resolve_ref(ref_name)?,
                None => anyhow::bail!("Invalid ref: {}", r#ref),
            },
            TypeSpec::Union(UnionRef { any_of }) => FieldType::Union(
                any_of
                    .iter()
                    .map(|t| self.to_field_type(t))
                    .collect::<Result<_>>()?,
            ),
        })
    }
    fn resolve_ref(&self, name: &str) -> Result<FieldType> {
        let classes = self.classes.clone();
        let classes = classes.lock().unwrap();
        let enums = self.enums.clone();
        let enums = enums.lock().unwrap();

        if classes.contains_key(name) {
            return Ok(FieldType::Class(name.to_string()));
        }
        if enums.contains_key(name) {
            return Ok(FieldType::Enum(name.to_string()));
        }

        anyhow::bail!("Unknown ref: {}", name)
    }
}

impl TryInto<TypeBuilder> for &JsonSchema {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<TypeBuilder> {
        log::debug!("Converting JsonSchema to TypeBuilder: {:#?}", self);

        let t = TypeBuilder::new();

        for (type_name, type_def) in self.defs.iter() {
            match type_def {
                TypeDef::StringOrEnum(string_or_enum_def) => {
                    if let Some(ref enum_values) = string_or_enum_def.r#enum {
                        t.add_enum(type_name, enum_values)?;
                    }
                }
                TypeDef::Class(class_def) => t.add_class(type_name, class_def)?,
                _ => {}
            }
        }

        let output_type = t.class("OutputFormat");
        let output_type = output_type.lock().unwrap();
        for (property_name, property_type) in self.properties.iter() {
            output_type
                .property(&property_name)
                .lock()
                .unwrap()
                .r#type(property_type.try_into()?);
        }

        Ok(t)
    }
}

impl TryInto<FieldType> for &TypeSpecWithMeta {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<FieldType> {
        Ok(match &self.type_spec {
            TypeSpec::Inline(type_def) => match type_def {
                TypeDef::StringOrEnum(StringOrEnumDef { r#enum: None }) => {
                    FieldType::Primitive(TypeValue::String)
                }
                TypeDef::StringOrEnum(StringOrEnumDef { r#enum: Some(_) }) => {
                    anyhow::bail!("inline TypeDef for enum not allowed")
                }
                TypeDef::Int => FieldType::Primitive(TypeValue::Int),
                TypeDef::Float => FieldType::Primitive(TypeValue::Float),
                TypeDef::Bool => FieldType::Primitive(TypeValue::Bool),
                TypeDef::Null => FieldType::Primitive(TypeValue::Null),
                TypeDef::Array(array_def) => {
                    FieldType::List(Box::new((&array_def.items).try_into()?))
                }
                TypeDef::Class(class_def) => anyhow::bail!("inline TypeDef for class not allowed"),
            },
            TypeSpec::Ref(TypeRef { r#ref }) => match r#ref.strip_prefix("#/$defs/") {
                //Some(ref_name) => self.resolve_ref(ref_name)?,
                Some(ref_name) => todo!(),
                None => anyhow::bail!("Invalid ref: {}", r#ref),
            },
            TypeSpec::Union(UnionRef { any_of }) => {
                FieldType::Union(any_of.iter().map(|t| t.try_into()).collect::<Result<_>>()?)
            }
        })
    }
}

// impl Into<OutputFormatContent> for &JsonSchema {
//     fn into(self) -> OutputFormatContent {
//         let mut enums = vec![];
//         let mut classes = vec![];

//         for (name, type_def) in self.defs.iter() {
//             match type_def {
//                 TypeDef::StringOrEnum(string_or_enum_def) => {
//                     if let Some(enum_values) = &string_or_enum_def.r#enum {
//                         enums.push(jt::Enum {
//                             name: jt::Name::new(name.clone()),
//                             values: enum_values
//                                 .iter()
//                                 .map(|v| (jt::Name::new(v.clone()), None))
//                                 .collect(),
//                         });
//                     }
//                 }
//                 TypeDef::Class(class_def) => {
//                     classes.push(jt::Class {
//                         name: jt::Name::new(name.clone()),
//                         fields: class_def
//                             .properties
//                             .iter()
//                             .map(|(field_name, field_type)| {
//                                 (jt::Name::new(field_name.clone()), field_type.into(), None)
//                             })
//                             .collect(),
//                     });
//                 }
//                 _ => {}
//             }
//         }
//         todo!()
//     }
// }

pub fn create_output_format(
    from_schema: OutputFormatContent,
    mode: OutputFormatMode,
) -> Result<String> {
    let rendered = from_schema
        .render(RenderOptions::default())
        .context("Failed to render output format")?;
    Ok("".to_string())
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
            "primary_address",
            "secondary_addresses",
            "zebra_addresses",
            "gpa",
            "alive",
            "nope"
          ],
          "title": "User",
          "type": "object"
        });

        let schema = JsonSchema::deserialize(&model_json_schema)?;
        println!("{:#?}", schema);

        let mut resolver = RefinedTypeResolver {
            refined: HashMap::new(),
        };
        let _ = schema.visit1(vec![], &mut resolver, &mut vec![]);

        println!("{:#?}", resolver);

        let mut tc = TypeCollector {
            tb: TypeBuilder::new(),
            resolver,
        };

        let _ = schema.visit2(vec![], &mut tc, &mut vec![]);
        println!("{:#?}", tc.tb);
        println!("{}", tc.tb.output_format()?);

        Ok(())
    }
}
