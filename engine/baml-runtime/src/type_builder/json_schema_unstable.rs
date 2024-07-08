use anyhow::Context;
use anyhow::Result;
use baml_types::BamlValue;
use baml_types::FieldType;
use baml_types::TypeValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::types::RenderOptions;
use serde::de;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::internal::prompt_renderer;
use crate::RuntimeContext;

use super::TypeBuilder;
use super::WithMeta;

pub enum OutputFormatMode {
    JsonSchema,
    TsInterface,
}

// can you model a list directly in pydantic?

// a dict is modelled as "additionalProperties" wtf?
//   - humans don't understand this, why would an LLM?

// TODO:
// - maps, tuples (BAML does not support these)
// - need to add errors to assert uniqueness
// - errors.is_empty() is a bad pattern, should use whether or not new errors were added as a signal
// - root def should use schema.title as the type name
// - ban duplicate symbols
#[derive(Debug, Deserialize)]
pub struct JsonSchema {
    #[serde(default, rename = "$defs")]
    defs1: HashMap<String, TypeSpecWithMeta>,

    #[serde(default, rename = "definitions")]
    defs2: HashMap<String, TypeSpecWithMeta>,

    #[serde(flatten)]
    type_spec_with_meta: TypeSpecWithMeta,
}

#[derive(Debug, Deserialize)]
struct TypeSpecWithMeta {
    #[serde(flatten)]
    meta: TypeMetadata,

    #[serde(flatten)]
    type_spec: TypeSpec,
}

#[derive(Clone, Debug, Deserialize)]
struct TypeMetadata {
    /// Pydantic includes this by default.
    #[serde(rename = "title")]
    _title: Option<String>,

    /// JSON schema considers 'enum' to be a validation rule, not a type,
    /// so it can be attached to any type.
    /// We only allow string-shaped enums
    r#enum: Option<Vec<String>>,

    /// We only allow string-shaped const values
    r#const: Option<String>,

    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TypeSpec {
    Ref(TypeRef),
    Inline(TypeDef),
    InlineUnion(InlineUnion),
    Union(UnionRef),
    Unknown(serde_json::Value),
}

#[derive(Debug, Deserialize)]
struct UnionRef {
    #[serde(rename = "anyOf", alias = "oneOf")]
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
    String,

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
struct InlineUnion {
    r#type: Vec<PrimitiveTypeDef>,
}

#[derive(Debug, Deserialize)]
enum PrimitiveTypeDef {
    #[serde(rename = "string")]
    String,

    #[serde(rename = "integer")]
    Int,

    #[serde(rename = "number")]
    Float,

    #[serde(rename = "boolean")]
    Bool,

    #[serde(rename = "null")]
    Null,
}

impl From<&PrimitiveTypeDef> for TypeValue {
    fn from(pt: &PrimitiveTypeDef) -> Self {
        match pt {
            PrimitiveTypeDef::String => TypeValue::String,
            PrimitiveTypeDef::Int => TypeValue::Int,
            PrimitiveTypeDef::Float => TypeValue::Float,
            PrimitiveTypeDef::Bool => TypeValue::Bool,
            PrimitiveTypeDef::Null => TypeValue::Null,
        }
    }
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
    Primitive(TypeValue),
}
#[derive(Clone, Debug)]
enum LazyTypeRef {
    Class,
    Enum,
    Union(Vec<LazyTypeRef>),
    Array(Box<LazyTypeRef>),
    Ref(String),
    Primitive(TypeValue),
}

#[derive(Debug)]
struct TypeIndex {
    index: HashMap<String, (LazyTypeRef, TypeMetadata)>,
}

impl TypeIndex {
    fn record_type(
        &mut self,
        position: &Vec<String>,
        lazy_ref: LazyTypeRef,
        meta: &TypeMetadata,
    ) -> LazyTypeRef {
        self.index
            .insert(position.join("/"), (lazy_ref.clone(), meta.clone()));
        lazy_ref
    }

    fn resolve_ref_recursive2(
        &self,
        name: String,
        type_ref: &LazyTypeRef,
        meta: &TypeMetadata,
    ) -> Result<FieldType> {
        Ok(match &type_ref {
            LazyTypeRef::Class => FieldType::Class(TypeCollector::to_type_name(
                &name.split("/").map(|s| s.to_string()).collect(),
                meta,
            )?),
            LazyTypeRef::Enum => FieldType::Enum(TypeCollector::to_type_name(
                &name.split("/").map(|s| s.to_string()).collect(),
                meta,
            )?),
            LazyTypeRef::Union(union) => {
                let mut any_of = vec![];
                for t in union.iter() {
                    any_of.push(self.resolve_ref_recursive2(name.clone(), type_ref, meta)?);
                }
                FieldType::Union(any_of)
            }
            LazyTypeRef::Array(t) => {
                FieldType::List(Box::new(self.resolve_ref_recursive2(name, type_ref, meta)?))
            }
            LazyTypeRef::Ref(name) => self.resolve_ref_recursive(name.to_string())?,
            LazyTypeRef::Primitive(type_value) => FieldType::Primitive(type_value.clone()),
        })
    }

    fn resolve_ref_recursive(&self, name: String) -> Result<FieldType> {
        match &self.index.get(&name) {
            Some((lazy_ref, meta)) => Ok(self.resolve_ref_recursive2(name, lazy_ref, meta)?),
            None => anyhow::bail!("Unresolved ref: {}", name),
        }
    }

    fn resolve_ref(&self, name: &str) -> Result<FieldType> {
        Ok(self.resolve_ref_recursive(name.to_string())?)
    }
}

impl JsonSchema {
    fn build_type_index(
        &self,
        position: Vec<String>,
        index: &mut TypeIndex,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<(), ()> {
        for (name, type_def) in self.defs1.iter() {
            let mut position = position.clone();
            position.push("$defs".to_string());
            position.push(name.clone());

            if let Ok(t) = type_def.build_type_index(position.clone(), index, errors) {
                index.record_type(&position, t, &type_def.meta);
            }
        }
        for (name, type_def) in self.defs2.iter() {
            let mut position = position.clone();
            position.push("definitions".to_string());
            position.push(name.clone());

            if let Ok(t) = type_def.build_type_index(position.clone(), index, errors) {
                index.record_type(&position, t, &type_def.meta);
            }
        }

        let _ = self
            .type_spec_with_meta
            .build_type_index(position.clone(), index, errors);

        log::trace!("type index: {:#?}", index);
        log::trace!("type index errors: {:#?}", errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl TypeSpecWithMeta {
    fn as_enum(
        &self,
        position: &Vec<String>,
        errors: &mut Vec<SerializationError>,
    ) -> Option<Result<Vec<String>, ()>> {
        let enum_values = match (&self.meta.r#enum, &self.meta.r#const) {
            (Some(enum_values), None) => enum_values.clone(),
            (None, Some(const_value)) => {
                vec![const_value.clone()]
            }
            (Some(enum_values), Some(const_value)) => {
                if enum_values.len() != 1 {
                    // Pydantic generates both 'const' and 'enum' for single-valued enums
                    errors.push(SerializationError {
                        position: position.clone(),
                        message:
                            "Type is 'const' and multi-valued 'enum', expected singly-valued enum"
                                .to_string(),
                    });
                    return Some(Err(()));
                }
                vec![const_value.clone()]
            }
            (None, None) => {
                return None;
            }
        };

        Some(match self.type_spec {
            // Allow unknown here, because if the enums/const are strings, then we can treat
            // the enum value as a string
            TypeSpec::Inline(TypeDef::String) | TypeSpec::Unknown(_) => Ok(enum_values),
            _ => {
                errors.push(SerializationError {
                    position: position.clone(),
                    message: "Enums are only supported for type=string fields".to_string(),
                });
                Err(())
            }
        })
    }
}

impl TypeSpecWithMeta {
    fn build_type_index(
        &self,
        position: Vec<String>,
        resolver: &mut TypeIndex,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<LazyTypeRef, ()> {
        if let Some(enum_values) = self.as_enum(&position, errors) {
            let _ = enum_values?;
            return Ok(resolver.record_type(&position, LazyTypeRef::Enum, &self.meta));
        }
        match &self.type_spec {
            TypeSpec::Inline(type_def) => {
                type_def.build_type_index(position, &self.meta, resolver, errors)
            }
            TypeSpec::InlineUnion(InlineUnion { r#type }) => Ok(LazyTypeRef::Union(
                r#type
                    .iter()
                    .map(|t| LazyTypeRef::Primitive(t.into()))
                    .collect(),
            )),
            TypeSpec::Union(union_ref) => {
                let mut any_of = vec![];
                let mut errs = vec![];

                for (i, t) in union_ref.any_of.iter().enumerate() {
                    let mut position = position.clone();
                    position.push("anyOf".to_string());
                    position.push(format!("{}", i));

                    match t.build_type_index(position, resolver, errors) {
                        Ok(t) => any_of.push(t),
                        Err(e) => errs.push(e),
                    }
                }
                if !errs.is_empty() {
                    return Err(());
                }
                Ok(resolver.record_type(&position, LazyTypeRef::Union(any_of), &self.meta))
            }
            TypeSpec::Ref(TypeRef { r#ref }) => {
                Ok(resolver.record_type(&position, LazyTypeRef::Ref(r#ref.clone()), &self.meta))
            }
            TypeSpec::Unknown(_) => {
                // TODO- how should this actually be handled?
                errors.push(SerializationError {
                    position: position.clone(),
                    message: "Unknown type schema".to_string(),
                });
                Err(())
            }
        }
    }
}
impl TypeDef {
    fn build_type_index(
        &self,
        position: Vec<String>,
        meta: &TypeMetadata,
        resolver: &mut TypeIndex,
        errors: &mut Vec<SerializationError>,
    ) -> core::result::Result<LazyTypeRef, ()> {
        match &self {
            TypeDef::Class(class_def) => {
                for (field_name, field_type) in class_def.properties.iter() {
                    let mut position = position.clone();
                    position.push("properties".to_string());
                    position.push(field_name.clone());

                    let _ = field_type.build_type_index(position, resolver, errors);
                }

                Ok(resolver.record_type(&position, LazyTypeRef::Class, meta))
            }
            TypeDef::Array(array_def) => {
                let mut position = position.clone();
                position.push("items".to_string());
                array_def.items.build_type_index(position, resolver, errors)
            }
            TypeDef::String => Ok(resolver.record_type(
                &position,
                LazyTypeRef::Primitive(TypeValue::String),
                meta,
            )),
            TypeDef::Int => {
                Ok(resolver.record_type(&position, LazyTypeRef::Primitive(TypeValue::Int), meta))
            }
            TypeDef::Float => {
                Ok(resolver.record_type(&position, LazyTypeRef::Primitive(TypeValue::Float), meta))
            }
            TypeDef::Bool => {
                Ok(resolver.record_type(&position, LazyTypeRef::Primitive(TypeValue::Bool), meta))
            }
            TypeDef::Null => {
                Ok(resolver.record_type(&position, LazyTypeRef::Primitive(TypeValue::Null), meta))
            }
        }
    }
}

//----------------------------------------------------------------------

struct TypeCollector {
    tb: TypeBuilder,
    index: TypeIndex,
}

impl TypeCollector {
    fn to_type_name(position: &Vec<String>, meta: &TypeMetadata) -> Result<String> {
        if position.len() == 1 && position[0] == "#" {
            return match &meta._title {
                Some(title) => Ok(title.clone()),
                None => Ok("#".to_string()),
            };
        }

        let position = if position.len() >= 3
            && position[0] == "#"
            && (position[1] == "$defs" || position[1] == "definitions")
        {
            &position[2..]
        } else {
            position.as_slice()
        };

        Ok(position.join("_"))
    }

    fn add_class(
        &self,
        position: &Vec<String>,
        meta: &TypeMetadata,
        fields: Vec<(&String, FieldType, &TypeMetadata)>,
    ) -> Result<FieldType> {
        let class_name = Self::to_type_name(position, meta)?;
        let arc = self.tb.class(class_name.as_str());
        let cb = arc.lock().unwrap();

        for (field_name, field_type, meta) in fields {
            let prop = cb.property(&field_name);
            let prop = prop.lock().unwrap();
            prop.r#type(field_type);
            if let Some(ref description) = meta.description {
                prop.with_meta("description", BamlValue::String(description.clone()));
            }
        }

        Ok(FieldType::class(class_name.as_str()))
    }

    fn add_enum(
        &self,
        position: &Vec<String>,
        meta: &TypeMetadata,
        enums: &[String],
    ) -> Result<FieldType> {
        let enum_name = Self::to_type_name(position, meta)?;
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
        for (name, type_def) in self.defs1.iter() {
            let mut position = position.clone();
            position.push("$defs".to_string());
            position.push(name.clone());

            let _ = type_def.visit2(position, v, errors);
        }
        for (name, type_def) in self.defs2.iter() {
            let mut position = position.clone();
            position.push("definitions".to_string());
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
        if let Some(enum_values) = self.as_enum(&position, errors) {
            let enum_values = enum_values?;
            return match v.add_enum(&position, &self.meta, enum_values.as_slice()) {
                Ok(t) => Ok(t),
                Err(e) => {
                    errors.push(SerializationError {
                        position: position.clone(),
                        message: format!("Failed to add enum: {:?}", e),
                    });
                    Err(())
                }
            };
        }
        match &self.type_spec {
            TypeSpec::Inline(type_def) => type_def.visit_type_def(position, &self.meta, v, errors),
            TypeSpec::Ref(TypeRef { ref r#ref }) => match v.index.resolve_ref(r#ref) {
                Ok(t) => Ok(t),
                Err(e) => {
                    errors.push(SerializationError {
                        position: position.clone(),
                        message: format!("{:?}", e),
                    });
                    Err(())
                }
            },
            TypeSpec::InlineUnion(InlineUnion { r#type }) => Ok(FieldType::Union(
                r#type
                    .iter()
                    .map(|t| FieldType::Primitive(t.into()))
                    .collect(),
            )),
            TypeSpec::Union(union_ref) => {
                let mut any_of = vec![];

                for (i, t) in union_ref.any_of.iter().enumerate() {
                    let mut position = position.clone();
                    position.push("anyOf".to_string());
                    position.push(format!("{}", i));

                    if let Ok(one_of) = t.visit2(position, v, errors) {
                        any_of.push(one_of);
                    }
                }

                if !errors.is_empty() {
                    return Err(());
                }
                Ok(FieldType::union(any_of))
            }
            TypeSpec::Unknown(_) => {
                errors.push(SerializationError {
                    position: position.clone(),
                    message: format!("Unknown type schema"),
                });
                Err(())
            }
        }
    }
}

impl TypeDef {
    fn visit_type_def(
        &self,
        position: Vec<String>,
        meta: &TypeMetadata,
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
                        position.push("properties".to_string());
                        position.push(field_name.clone());

                        match field_type.visit2(position, v, errors) {
                            Ok(t) => Ok((
                                field_name,
                                if class_def.required.contains(&field_name) {
                                    t
                                } else {
                                    FieldType::Optional(Box::new(t))
                                },
                                &field_type.meta,
                            )),
                            Err(()) => Err(()),
                        }
                    })
                    .collect::<Result<Vec<_>, ()>>()?;

                match v.add_class(&position, meta, fields) {
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
            TypeDef::String => FieldType::Primitive(TypeValue::String),
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
        log::trace!(
            "output format for JsonSchemaType: {:#?}\ntype builder: {:?}",
            self.inner,
            tb
        );
        let (class_overrides, enum_overrides) = tb.to_overrides();
        let ctx = RuntimeContext {
            env: HashMap::new(),
            tags: HashMap::new(),
            class_override: class_overrides,
            enum_overrides: enum_overrides,
        };

        let ir = IntermediateRepr::create_empty();

        let output_format = prompt_renderer::render_output_format(&ir, &ctx, &self.inner)
            .context("Failed to build output format renderer")?;

        match output_format.render(RenderOptions::default()) {
            Ok(Some(s)) => anyhow::Ok(s),
            Ok(None) => Err(anyhow::anyhow!("Failed to render output format (none)")),
            Err(e) => Err(anyhow::anyhow!("Failed to render output format: {:?}", e)),
        }
        .context(format!(
            "while attempting to render output format for {:?}",
            self.inner
        ))
    }
}

pub trait AddJsonSchema {
    fn add_json_schema_from_str(&self, schema: String) -> Result<JsonSchemaType> {
        let schema: JsonSchema = serde_json::from_str(&schema)?;
        self.add_json_schema(schema)
    }

    fn add_json_schema_from_value(&self, schema: serde_json::Value) -> Result<JsonSchemaType> {
        let schema: JsonSchema = serde_json::from_value(schema)?;
        self.add_json_schema(schema)
    }

    fn add_json_schema(&self, schema: JsonSchema) -> Result<JsonSchemaType>;
}

impl AddJsonSchema for TypeBuilder {
    fn add_json_schema(&self, schema: JsonSchema) -> Result<JsonSchemaType> {
        let position = vec!["#".to_string()];
        let mut errors = Vec::new();

        let mut resolver = TypeIndex {
            index: HashMap::new(),
        };
        let Ok(_) = schema.build_type_index(position.clone(), &mut resolver, &mut errors) else {
            anyhow::bail!("Errors happened during visit1: {:#?}", errors)
        };

        log::trace!("{:#?}", resolver);

        let mut errors = Vec::new();
        let mut tc = TypeCollector {
            tb: TypeBuilder::new(),
            index: resolver,
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

        log::trace!("{:#?}", self);

        Ok(JsonSchemaType { inner: field_type })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_output_format(schema: &serde_json::Value) -> Result<String> {
        let tb = TypeBuilder::new();
        tb.add_json_schema_from_str(schema.to_string())?
            .output_format(&tb)
    }

    macro_rules! output_format_test {
        ($name:ident, $schema:tt) => {
            // ($name:ident, $schema:tt, $expected:expr) => {
            #[test]
            fn $name() {
                let schema = serde_json::json!($schema);
                match to_output_format(&schema).context(format!("JSON schema: {:#?}", schema)) {
                    Ok(s) => {
                        println!("{}", s);
                    }
                    Err(e) => panic!("Failed to convert JSON schema to output format: {:?}", e),
                }
            }
        };
    }

    // output_format_test!(root_is_string, {
    //     "type": "string"
    // });

    output_format_test!(root_is_array, {
        "items": {
            "type": "string"
        },
        "type": "array"
    });

    output_format_test!(root_is_enum, {
        "enum": ["admin", "user", "guest"],
        "type": "string"
    });

    output_format_test!(root_is_enum_defined_with_const, {
        "const": "admin",
        "type": "string"
    });

    output_format_test!(root_is_enum_singly_valued_from_pydantic, {
        "const": "admin",
        "enum": ["admin"],
        "type": "string"
    });

    output_format_test!(root_is_object, {
        "properties": {
            "name": { "type": "string" },
        },
        "type": "object"
    });

    output_format_test!(root_is_union, {
        "anyOf": [
            { "type": "string" },
            { "type": "integer" }
        ]
    });

    // zod relies on this, and it's in the spec
    // we only allow a subset of what the JSON schema spec allows though;
    // we only support primitive types in this syntax
    output_format_test!(root_is_inline_union, {
        "type": ["string", "integer"]
    });

    output_format_test!(all_primitive_types, {
        "properties": {
            "name":   { "type": "string" },
            "count":  { "type": "integer" },
            "score":  { "type": "number" },
            "exists": { "type": "boolean" },
            "nah":    { "type": "null" },
        },
        "type": "object"
    });

    output_format_test!(root_refs_enum_in_defs, {
        "$defs": {
            "Role2": {
                "enum": ["admin2", "user2", "guest2"],
                "type": "string"
            }
        },
        "$ref": "#/$defs/Role2",
    });

    output_format_test!(root_refs_object_in_defs, {
        "$defs": {
            "Person": {
                "properties": {
                    "name": { "type": "string" },
                },
                "type": "object"
            }
        },
        "$ref": "#/$defs/Person",
    });

    // output_format_test!(root_refs_union_in_defs, {
    //     "$defs": {
    //         "Label": {
    //             "anyOf": [
    //                 { "type": "string" },
    //                 { "type": "integer" }
    //             ]
    //         }
    //     },
    //     "$ref": "#/$defs/Label",
    // });

    output_format_test!(inline_enum, {
        "properties": {
            "color": {
                "type": "string",
                "enum": ["red", "green", "blue"]
            },
        },
        "type": "object"
    });

    output_format_test!(inline_object, {
        "properties": {
            "prop1": {
                "properties": {
                    "prop2": { "type": "string" },
                },
                "type": "object",
            },
        },
        "type": "object"
    });

    output_format_test!(inline_enum_in_union, {
        "anyOf": [
            {
                "type": "string",
                "enum": ["red", "green", "blue"]
            },
            {
                "type": "integer"
            }
        ]
    });

    output_format_test!(inline_object_in_union, {
        "anyOf": [
            {
                "properties": {
                    "prop": { "type": "string" },
                },
                "type": "object",
            },
            {
                "type": "integer"
            }
        ]
    });

    #[test]
    fn test_root_uses_title() -> Result<()> {
        let schema = serde_json::json!({
            "title": "Role",
            "enum": ["admin", "user", "guest"],
            "type": "string"
        });

        let output_format = to_output_format(&schema)?;

        assert!(output_format.contains("Role"));
        assert!(!output_format.contains("#"));

        Ok(())
    }

    #[test]
    fn test_complex() -> Result<()> {
        let schema = serde_json::json!({
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

        let output_format = to_output_format(&schema)?;

        println!("{}", output_format);

        Ok(())
    }

    #[test]
    #[ignore]
    /// There are recursive data models in this schema, which we don't get
    fn test_complex_fhir() {
        let schema_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/type_builder/test-data-fhir.schema.json"
        );
        let fhir_schema =
            std::fs::read_to_string(schema_path).expect("Failed to read FHIR schema from file");

        let tb = TypeBuilder::new();
        let output_format = tb
            .add_json_schema_from_str(fhir_schema)
            .expect("add json schema should succeed")
            .output_format(&tb)
            .expect("output format of json schema should succeed");

        println!("{}", output_format);
    }
}
