// JSON Schema

use baml_types::TypeValue;
use serde_json::json;

use super::{
    repr::{self},
    Class, Enum, FieldType, FunctionArgs, FunctionNode, IntermediateRepr, Walker,
};

pub trait WithJsonSchema {
    fn json_schema(&self) -> serde_json::Value;
}

impl WithJsonSchema for IntermediateRepr {
    fn json_schema(&self) -> serde_json::Value {
        let enums = self
            .walk_enums()
            .map(|e| (e.elem().name.clone(), e.json_schema()));
        let classes = self
            .walk_classes()
            .map(|c| (c.elem().name.clone(), c.json_schema()));
        let function_inputs = self
            .walk_functions()
            .map(|f| (format!("{}_input", f.name()), (f.item, true).json_schema()));
        let function_outputs = self.walk_functions().map(|f| {
            (
                format!("{}_output", f.name()),
                (f.item, false).json_schema(),
            )
        });

        // Combine all the definitions into one object of key-value pairs
        let definitions = enums
            .chain(classes)
            .chain(function_inputs)
            .chain(function_outputs)
            .collect::<serde_json::Map<_, _>>();

        json!({
            "definitions": definitions,
        })
    }
}

impl WithJsonSchema for (&FunctionNode, bool) {
    fn json_schema(&self) -> serde_json::Value {
        let (f, is_input) = self;

        let mut res = if *is_input {
            f.elem.inputs.json_schema()
        } else {
            f.elem.output.json_schema()
        };

        // Add a title field to the schema
        if let serde_json::Value::Object(res) = &mut res {
            res.insert(
                "title".to_string(),
                json!(format!(
                    "{} {}",
                    f.elem.name(),
                    if *is_input { "input" } else { "output" }
                )),
            );
        }

        res
    }
}

impl WithJsonSchema for FunctionArgs {
    fn json_schema(&self) -> serde_json::Value {
        match self {
            FunctionArgs::UnnamedArg(t) => t.json_schema(),
            FunctionArgs::NamedArgList(args) => {
                let mut properties = json!({});
                let mut required_props = vec![];
                for (name, t) in args.iter() {
                    properties[name] = t.json_schema();
                    match t {
                        FieldType::Optional(_) => {
                            required_props.push(name.clone());
                        }
                        _ => {}
                    }
                }
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required_props,
                })
            }
        }
    }
}

impl WithJsonSchema for Vec<(String, FieldType)> {
    fn json_schema(&self) -> serde_json::Value {
        let mut properties = json!({});
        let mut required_props = vec![];
        for (name, t) in self.iter() {
            properties[name.clone()] = t.json_schema();
            match t {
                FieldType::Optional(_) => {}
                _ => {
                    required_props.push(name.clone());
                }
            }
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": required_props,
        })
    }
}

impl WithJsonSchema for Walker<'_, &Enum> {
    fn json_schema(&self) -> serde_json::Value {
        json!({
                "title": self.elem().name,
                "enum": self.elem().values
                    .iter()
                    .map(|v| json!({
                        "const": v.elem.0.clone()
                    }))
                    .collect::<Vec<_>>(),

        })
    }
}

impl WithJsonSchema for Walker<'_, &Class> {
    fn json_schema(&self) -> serde_json::Value {
        let mut properties = json!({});
        let mut required_props = vec![];
        for field in self.elem().static_fields.iter() {
            properties[field.elem.name.clone()] = field.elem.r#type.elem.json_schema();
            match field.elem.r#type.elem {
                FieldType::Optional(_) => {}
                _ => {
                    required_props.push(field.elem.name.clone());
                }
            }
        }
        json!({
                "title": self.elem().name,
                "type": "object",
                "properties": properties,
                "required": required_props,
        })
    }
}

impl<'db> WithJsonSchema for FieldType {
    fn json_schema(&self) -> serde_json::Value {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => json!({
                "$ref": format!("#/definitions/{}", name),
            }),
            FieldType::Primitive(t) => match t {
                TypeValue::String => json!({
                    "type": "string",
                }),
                TypeValue::Int => json!({
                    "type": "integer",
                }),
                TypeValue::Float => json!({
                    "type": "number",
                }),
                TypeValue::Bool => json!({
                    "type": "boolean",
                }),
                TypeValue::Null => json!({
                    "type": "null",
                }),
                TypeValue::Media(_) => json!({
                    // anyOf either an object that has a uri, or it has a base64 string
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                        }
                    },
                    "required": ["url"],
                }),
            },
            FieldType::List(item) => json!({
                "type": "array",
                "items": (*item).json_schema()
            }),
            FieldType::Map(_k, v) => json!({
                "type": "object",
                "additionalProperties": {
                    "type": v.json_schema(),
                }
            }),
            FieldType::Union(options) => json!({
                "anyOf": options.iter().map(|t| {
                    let mut res = t.json_schema();
                    // if res is a map, add a "title" field
                    if let serde_json::Value::Object(r) = &mut res {
                        r.insert("title".to_string(), json!(t.to_string()));
                    }
                    res
                }
            ).collect::<Vec<_>>(),
            }),
            FieldType::Tuple(options) => json!({
                "type": "array",
                "prefixItems": options.iter().map(|t| t.json_schema()).collect::<Vec<_>>(),
            }),
            // The caller object is responsible for adding the "null" type
            FieldType::Optional(inner) => {
                match **inner {
                    FieldType::Primitive(_) => {
                        let mut res = inner.json_schema();
                        res["type"] = json!([res["type"], "null"]);
                        res["default"] = serde_json::Value::Null;
                        res
                    }
                    _ => {
                        let mut res = inner.json_schema();
                        // if res is a map, add a "title" field
                        if let serde_json::Value::Object(r) = &mut res {
                            r.insert("title".to_string(), json!(inner.to_string()));
                        }
                        json!({
                            "anyOf": [res, json!({"type": "null", "title": "null"})],
                            "default": serde_json::Value::Null,
                        })
                    }
                }
            },
            FieldType::Constrained{ base, .. } => base.json_schema(),
        }
    }
}
